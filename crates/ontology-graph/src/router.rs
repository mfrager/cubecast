//! `MultiStore`: one `RdfStore` over several physical stores.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex, PoisonError};

use ontology_core::config::UpdatePolicy;
use ontology_core::error::{Error, Result};
use ontology_core::format::RdfFormat;
use ontology_core::id::Id;
use ontology_core::model::{GraphName, Quad, QuadPattern, QueryResult, Term};
use ontology_core::rdf::{RdfStore, RdfTransaction};
use ontology_rdf::LocalFederation;
use spargebra::algebra::GraphTarget;
use spargebra::term::{
    GraphName as SparqlGraphName, GraphNamePattern, GroundQuad, Quad as SparqlQuad,
};
use spargebra::update::{DeleteDataOperation, GraphUpdateOperation, InsertDataOperation, Update};
use spargebra::SparqlParser;

use crate::catalog::{
    CatalogSnapshot, GraphCatalogImpl, GraphStoreId, StoreAccess, StoreHandle, StoreLayout,
    StoreRole,
};
use crate::rewrite::{self, PlanCache, QueryPlan};

/// RDF store that routes across the main store and its satellites.
pub struct MultiStore {
    catalog: Arc<GraphCatalogImpl>,
    federation: LocalFederation,
    plans: Mutex<PlanCache>,
    policy: UpdatePolicy,
}

impl MultiStore {
    /// Creates the router for a catalog.
    #[must_use]
    pub fn new(catalog: Arc<GraphCatalogImpl>, policy: UpdatePolicy) -> Self {
        let generation = catalog.current().generation;
        Self {
            catalog,
            federation: LocalFederation::new(),
            plans: Mutex::new(PlanCache::new(generation)),
            policy,
        }
    }

    /// Returns the catalog behind the router.
    #[must_use]
    pub fn catalog(&self) -> &Arc<GraphCatalogImpl> {
        &self.catalog
    }

    /// Plans a query for diagnostics without evaluating it.
    ///
    /// # Errors
    ///
    /// Propagates parse and routing failures.
    pub fn plan_text(&self, text: &str) -> Result<Arc<QueryPlan>> {
        let snapshot = self.catalog.current();
        self.plan(text, &snapshot)
    }

    fn plan(&self, text: &str, snapshot: &CatalogSnapshot) -> Result<Arc<QueryPlan>> {
        let mut cache = self.plans.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(plan) = cache.get(text, snapshot.generation) {
            return Ok(plan);
        }
        let plan = Arc::new(rewrite::plan_query(snapshot, text)?);
        cache.insert(text.to_owned(), Arc::clone(&plan));
        Ok(plan)
    }

    fn owner_for_quad(&self, snapshot: &CatalogSnapshot, quad: &Quad) -> Result<Arc<StoreHandle>> {
        match &quad.graph {
            GraphName::Iri(graph) => snapshot.resolve_graph(graph),
            GraphName::Default | GraphName::BlankNode(_) => Ok(Arc::clone(&snapshot.main)),
        }
    }

    fn write_quads(&self, snapshot: &CatalogSnapshot, quads: &[Quad], delete: bool) -> Result<()> {
        if snapshot.single {
            return snapshot.main.with_store(|store| {
                if delete {
                    store.delete(quads)
                } else {
                    store.insert(quads)
                }
            });
        }
        let mut groups: BTreeMap<GraphStoreId, (Arc<StoreHandle>, Vec<Quad>)> = BTreeMap::new();
        for quad in quads {
            let handle = self.owner_for_quad(snapshot, quad)?;
            if handle.role == StoreRole::Satellite {
                if handle.access != StoreAccess::ReadWrite {
                    return Err(Error::GraphReadOnly {
                        graph: quad.graph.to_string(),
                        store: handle.id.to_string(),
                    });
                }
                if matches!(quad.subject, Term::BlankNode(_)) {
                    return Err(Error::validation(format!(
                        "blank-node subjects cannot be written to satellite store {}",
                        handle.id
                    )));
                }
            }
            let translated = translate_for_store(&handle, quad.clone());
            groups
                .entry(handle.id.clone())
                .or_insert_with(|| (Arc::clone(&handle), Vec::new()))
                .1
                .push(translated);
        }
        for (_, (handle, quads)) in groups {
            if handle.role == StoreRole::Main {
                handle.with_store(|store| {
                    if delete {
                        store.delete(&quads)
                    } else {
                        store.insert(&quads)
                    }
                })?;
            } else {
                handle.with_writable(|store| {
                    if delete {
                        store.delete(&quads)
                    } else {
                        store.insert(&quads)
                    }
                })?;
            }
        }
        Ok(())
    }

    fn read_store(&self, handle: &StoreHandle, pattern: &QuadPattern) -> Result<Vec<Quad>> {
        if !handle.is_ready() {
            tracing::warn!(store = %handle.id, "query routed to a degraded store");
            return Ok(Vec::new());
        }
        handle.with_store(|store| store.get(pattern.clone()))
    }

    fn execute_update(&self, snapshot: &CatalogSnapshot, update: Update) -> Result<()> {
        if update.operations.is_empty() {
            return Ok(());
        }
        let mut stores = BTreeSet::new();
        for operation in &update.operations {
            self.collect_write_stores(snapshot, operation, &mut stores)?;
        }
        if stores.is_empty() {
            stores.insert(snapshot.main.id.clone());
        }
        if stores.len() == 1 {
            let id = stores.into_iter().next().expect("one store");
            return self.execute_on_store(snapshot, &update, &id);
        }
        if self.policy == UpdatePolicy::Strict {
            return Err(Error::validation(format!(
                "update touches stores {}; set graphs.update_policy = \"split\" to allow it",
                stores
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
        if update.operations.iter().any(|operation| {
            !matches!(
                operation,
                GraphUpdateOperation::InsertData(_) | GraphUpdateOperation::DeleteData(_)
            )
        }) {
            return Err(Error::Unsupported {
                operation: "cross-store DELETE/INSERT".to_owned(),
            });
        }

        let mut inserts: BTreeMap<GraphStoreId, Vec<SparqlQuad>> = BTreeMap::new();
        let mut deletes: BTreeMap<GraphStoreId, Vec<GroundQuad>> = BTreeMap::new();
        for operation in update.operations {
            match operation {
                GraphUpdateOperation::InsertData(data) => {
                    for quad in data.data {
                        let handle = resolve_sparql_graph(snapshot, &quad.graph_name)?;
                        let mut quad = quad;
                        quad.graph_name = translate_sparql_graph(&handle, quad.graph_name);
                        inserts.entry(handle.id.clone()).or_default().push(quad);
                    }
                }
                GraphUpdateOperation::DeleteData(data) => {
                    for quad in data.data {
                        let handle = resolve_sparql_graph(snapshot, &quad.graph_name)?;
                        let mut quad = quad;
                        quad.graph_name = translate_sparql_graph(&handle, quad.graph_name);
                        deletes.entry(handle.id.clone()).or_default().push(quad);
                    }
                }
                _ => unreachable!("checked above"),
            }
        }

        let mut ids: BTreeSet<GraphStoreId> = BTreeSet::new();
        ids.extend(inserts.keys().cloned());
        ids.extend(deletes.keys().cloned());
        let mut applied = Vec::new();
        for id in ids {
            let mut operations = Vec::new();
            if let Some(data) = deletes.remove(&id) {
                operations.push(GraphUpdateOperation::DeleteData(DeleteDataOperation {
                    data,
                }));
            }
            if let Some(data) = inserts.remove(&id) {
                operations.push(GraphUpdateOperation::InsertData(InsertDataOperation {
                    data,
                }));
            }
            let group = Update {
                base_iri: None,
                operations,
            };
            match self.execute_on_store(snapshot, &group, &id) {
                Ok(()) => applied.push(id.to_string()),
                Err(error) => {
                    return Err(Error::GraphPartial {
                        applied,
                        failed: id.to_string(),
                        reason: error.to_string(),
                    })
                }
            }
        }
        Ok(())
    }

    fn collect_write_stores(
        &self,
        snapshot: &CatalogSnapshot,
        operation: &GraphUpdateOperation,
        stores: &mut BTreeSet<GraphStoreId>,
    ) -> Result<()> {
        match operation {
            GraphUpdateOperation::InsertData(data) => {
                for quad in &data.data {
                    stores.insert(resolve_write_graph(snapshot, &quad.graph_name)?.id.clone());
                }
            }
            GraphUpdateOperation::DeleteData(data) => {
                for quad in &data.data {
                    stores.insert(resolve_write_graph(snapshot, &quad.graph_name)?.id.clone());
                }
            }
            GraphUpdateOperation::DeleteInsert(data) => {
                for name in data
                    .delete
                    .iter()
                    .map(|quad| &quad.graph_name)
                    .chain(data.insert.iter().map(|quad| &quad.graph_name))
                {
                    match name {
                        GraphNamePattern::NamedNode(node) => {
                            let graph =
                                Id::new(node.as_str()).map_err(|error| Error::GraphRegistry {
                                    detail: error.to_string(),
                                })?;
                            stores.insert(snapshot.resolve_graph(&graph)?.id.clone());
                        }
                        GraphNamePattern::DefaultGraph => {
                            stores.insert(snapshot.main.id.clone());
                        }
                        GraphNamePattern::Variable(_) => {
                            return Err(Error::Unsupported {
                                operation: "variable graph in a DELETE/INSERT template".to_owned(),
                            })
                        }
                    }
                }
            }
            GraphUpdateOperation::Load(data) => {
                stores.insert(resolve_write_graph(snapshot, &data.destination)?.id.clone());
            }
            GraphUpdateOperation::Clear(data) => {
                collect_target_stores(snapshot, &data.graph, stores)?;
            }
            GraphUpdateOperation::Drop(data) => {
                collect_target_stores(snapshot, &data.graph, stores)?;
            }
            GraphUpdateOperation::Create(data) => {
                let graph = Id::new(data.graph.as_str()).map_err(|error| Error::GraphRegistry {
                    detail: error.to_string(),
                })?;
                stores.insert(snapshot.resolve_graph(&graph)?.id.clone());
            }
        }
        Ok(())
    }

    fn execute_on_store(
        &self,
        snapshot: &CatalogSnapshot,
        update: &Update,
        id: &GraphStoreId,
    ) -> Result<()> {
        let handle = snapshot
            .store(id)
            .cloned()
            .ok_or_else(|| Error::GraphRegistry {
                detail: format!("store {id} is not registered"),
            })?;
        if handle.role == StoreRole::Satellite && handle.access != StoreAccess::ReadWrite {
            return Err(Error::GraphReadOnly {
                graph: handle.id.to_string(),
                store: handle.id.to_string(),
            });
        }
        let rewritten = rewrite_update(snapshot, &handle, update);
        if handle.role == StoreRole::Main {
            handle.with_store(|store| store.update_with_services(&rewritten, &self.federation))
        } else {
            handle.with_writable(|store| store.update_with_services(&rewritten, &self.federation))
        }
    }
}

fn translate_for_store(handle: &StoreHandle, quad: Quad) -> Quad {
    match (&quad.graph, handle.layout) {
        (GraphName::Iri(_), StoreLayout::Single) => quad.with_graph(GraphName::Default),
        _ => quad,
    }
}

fn resolve_write_graph(
    snapshot: &CatalogSnapshot,
    name: &SparqlGraphName,
) -> Result<Arc<StoreHandle>> {
    resolve_sparql_graph(snapshot, name)
}

fn translate_sparql_graph(handle: &StoreHandle, name: SparqlGraphName) -> SparqlGraphName {
    match (handle.layout, name) {
        (StoreLayout::Single, SparqlGraphName::NamedNode(_)) => SparqlGraphName::DefaultGraph,
        (_, other) => other,
    }
}

fn translate_pattern(handle: &StoreHandle, name: GraphNamePattern) -> GraphNamePattern {
    match (handle.layout, name) {
        (StoreLayout::Single, GraphNamePattern::NamedNode(_)) => GraphNamePattern::DefaultGraph,
        (_, other) => other,
    }
}

fn translate_target(handle: &StoreHandle, target: GraphTarget) -> GraphTarget {
    match (handle.layout, target) {
        (StoreLayout::Single, GraphTarget::NamedNode(_)) => GraphTarget::DefaultGraph,
        (_, other) => other,
    }
}

fn rewrite_update(snapshot: &CatalogSnapshot, handle: &StoreHandle, update: &Update) -> Update {
    let mut update = update.clone();
    for operation in &mut update.operations {
        match operation {
            GraphUpdateOperation::InsertData(data) => {
                for quad in &mut data.data {
                    quad.graph_name = translate_sparql_graph(handle, quad.graph_name.clone());
                }
            }
            GraphUpdateOperation::DeleteData(data) => {
                for quad in &mut data.data {
                    quad.graph_name = translate_sparql_graph(handle, quad.graph_name.clone());
                }
            }
            GraphUpdateOperation::DeleteInsert(data) => {
                let mut fallbacks = Vec::new();
                *data.pattern = rewrite::rewrite_pattern(
                    snapshot,
                    &handle.id,
                    (*data.pattern).clone(),
                    &mut fallbacks,
                );
                for quad in &mut data.delete {
                    quad.graph_name = translate_pattern(handle, quad.graph_name.clone());
                }
                for quad in &mut data.insert {
                    quad.graph_name = translate_pattern(handle, quad.graph_name.clone());
                }
            }
            GraphUpdateOperation::Load(data) => {
                data.destination = translate_sparql_graph(handle, data.destination.clone());
            }
            GraphUpdateOperation::Clear(data) => {
                data.graph = translate_target(handle, data.graph.clone());
            }
            GraphUpdateOperation::Drop(data) => {
                data.graph = translate_target(handle, data.graph.clone());
            }
            GraphUpdateOperation::Create(_) => {}
        }
    }
    update
}

impl RdfStore for MultiStore {
    fn get(&self, pattern: QuadPattern) -> Result<Vec<Quad>> {
        let snapshot = self.catalog.current();
        if snapshot.single {
            return snapshot.main.with_store(|store| store.get(pattern));
        }
        match &pattern.graph {
            Some(GraphName::Default) => snapshot.main.with_store(|store| store.get(pattern)),
            Some(GraphName::BlankNode(_)) => snapshot.main.with_store(|store| store.get(pattern)),
            Some(GraphName::Iri(graph)) => {
                let handle = snapshot.resolve_graph(graph)?;
                let engine_graph = snapshot.engine_graph(graph, &handle);
                let engine_pattern = pattern.clone().with_graph(engine_graph);
                let quads = self.read_store(&handle, &engine_pattern)?;
                Ok(quads
                    .into_iter()
                    .map(|quad| quad.with_graph(GraphName::Iri(graph.clone())))
                    .collect())
            }
            None => {
                let mut quads = Vec::new();
                for handle in snapshot.stores.values() {
                    let stored = self.read_store(handle, &pattern)?;
                    for quad in stored {
                        quads.push(logical_quad(handle, quad));
                    }
                }
                quads.sort();
                quads.dedup();
                Ok(quads)
            }
        }
    }

    fn insert(&self, quads: &[Quad]) -> Result<()> {
        self.write_quads(&self.catalog.current(), quads, false)
    }

    fn delete(&self, quads: &[Quad]) -> Result<()> {
        self.write_quads(&self.catalog.current(), quads, true)
    }

    fn query(&self, sparql: &str) -> Result<QueryResult> {
        let snapshot = self.catalog.current();
        if snapshot.single {
            return snapshot.main.with_store(|store| store.query(sparql));
        }
        let plan = self.plan(sparql, &snapshot)?;
        let host = snapshot
            .store(&plan.host)
            .cloned()
            .ok_or_else(|| Error::GraphRegistry {
                detail: format!("host store {} is not registered", plan.host),
            })?;
        host.with_store(|store| store.query_with_services(&plan.query, &self.federation))
    }

    fn update(&self, sparql: &str) -> Result<()> {
        let snapshot = self.catalog.current();
        if snapshot.single {
            return snapshot.main.with_store(|store| store.update(sparql));
        }
        let update = SparqlParser::new()
            .parse_update(sparql)
            .map_err(Error::update)?;
        self.execute_update(&snapshot, update)
    }

    fn graphs(&self) -> Result<Vec<GraphName>> {
        let snapshot = self.catalog.current();
        if snapshot.single {
            return snapshot.main.with_store(|store| store.graphs());
        }
        Ok(snapshot
            .enabled_graphs()
            .into_iter()
            .map(GraphName::Iri)
            .collect())
    }

    fn load_from_slice(
        &self,
        format: RdfFormat,
        data: &[u8],
        base: Option<&str>,
        graph: Option<&GraphName>,
    ) -> Result<u64> {
        let snapshot = self.catalog.current();
        let (handle, engine_graph) = match graph {
            Some(GraphName::Iri(iri)) if !snapshot.single => {
                let handle = snapshot.resolve_graph(iri)?;
                let engine = snapshot.engine_graph(iri, &handle);
                (handle, Some(engine))
            }
            Some(name) => (Arc::clone(&snapshot.main), Some(name.clone())),
            None => (Arc::clone(&snapshot.main), None),
        };
        if handle.role == StoreRole::Satellite {
            if handle.access != StoreAccess::ReadWrite {
                return Err(Error::GraphReadOnly {
                    graph: graph.map(ToString::to_string).unwrap_or_default(),
                    store: handle.id.to_string(),
                });
            }
            return handle.with_writable(|store| {
                store.load_from_slice(format, data, base, engine_graph.as_ref())
            });
        }
        handle.with_store(|store| store.load_from_slice(format, data, base, engine_graph.as_ref()))
    }

    fn serialize(&self, format: RdfFormat, graph: Option<&GraphName>) -> Result<String> {
        let snapshot = self.catalog.current();
        let (handle, engine_graph) = match graph {
            Some(GraphName::Iri(iri)) if !snapshot.single => {
                let handle = snapshot.resolve_graph(iri)?;
                let engine = snapshot.engine_graph(iri, &handle);
                (handle, Some(engine))
            }
            Some(name) => (Arc::clone(&snapshot.main), Some(name.clone())),
            None => (Arc::clone(&snapshot.main), None),
        };
        handle.with_store(|store| store.serialize(format, engine_graph.as_ref()))
    }

    fn transaction(&self) -> Result<Box<dyn RdfTransaction + '_>> {
        // Changes are buffered and routed on commit, so every transaction works
        // in registry mode. A single-store deployment uses the engine's own
        // transaction through [`crate::GraphBackend::rdf_store`].
        Ok(Box::new(BufferedTransaction {
            router: self,
            inserts: Vec::new(),
            deletes: Vec::new(),
        }))
    }

    fn len(&self) -> Result<usize> {
        let snapshot = self.catalog.current();
        if snapshot.single {
            return snapshot.main.with_store(|store| store.len());
        }
        Ok(snapshot
            .stores
            .values()
            .map(|handle| handle.quad_count() as usize)
            .sum())
    }
}

/// Buffers a transaction and routes its quads when it commits.
struct BufferedTransaction<'a> {
    router: &'a MultiStore,
    inserts: Vec<Quad>,
    deletes: Vec<Quad>,
}

impl RdfTransaction for BufferedTransaction<'_> {
    fn insert(&mut self, quads: &[Quad]) -> Result<()> {
        self.inserts.extend_from_slice(quads);
        Ok(())
    }

    fn delete(&mut self, quads: &[Quad]) -> Result<()> {
        self.deletes.extend_from_slice(quads);
        Ok(())
    }

    fn commit(self: Box<Self>) -> Result<()> {
        let snapshot = self.router.catalog.current();
        if !self.deletes.is_empty() {
            self.router.write_quads(&snapshot, &self.deletes, true)?;
        }
        if !self.inserts.is_empty() {
            self.router.write_quads(&snapshot, &self.inserts, false)?;
        }
        Ok(())
    }
}

fn logical_quad(handle: &StoreHandle, quad: Quad) -> Quad {
    if handle.layout == StoreLayout::Single {
        if let Some(graph) = handle.graphs.first() {
            return quad.with_graph(GraphName::Iri(graph.clone()));
        }
    }
    quad
}

fn resolve_sparql_graph(
    snapshot: &CatalogSnapshot,
    name: &SparqlGraphName,
) -> Result<Arc<StoreHandle>> {
    match name {
        SparqlGraphName::DefaultGraph => Ok(Arc::clone(&snapshot.main)),
        SparqlGraphName::NamedNode(node) => {
            let graph = Id::new(node.as_str()).map_err(|error| Error::GraphRegistry {
                detail: error.to_string(),
            })?;
            snapshot.resolve_graph(&graph)
        }
    }
}

fn collect_target_stores(
    snapshot: &CatalogSnapshot,
    target: &GraphTarget,
    stores: &mut BTreeSet<GraphStoreId>,
) -> Result<()> {
    match target {
        GraphTarget::NamedNode(node) => {
            let graph = Id::new(node.as_str()).map_err(|error| Error::GraphRegistry {
                detail: error.to_string(),
            })?;
            stores.insert(snapshot.resolve_graph(&graph)?.id.clone());
        }
        GraphTarget::DefaultGraph => {
            stores.insert(snapshot.main.id.clone());
        }
        GraphTarget::NamedGraphs | GraphTarget::AllGraphs => {
            stores.extend(snapshot.stores.keys().cloned());
        }
    }
    Ok(())
}
