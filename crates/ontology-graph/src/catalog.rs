//! Catalog snapshots: an immutable view of stores, graphs, and namespaces.
//!
//! The catalog is published as a single `Arc<CatalogSnapshot>` so a query sees
//! one consistent registry generation even while a sync is running. Store
//! handles are shared between generations and keep their engine handle behind a
//! lock, which lets a satellite be quiesced for writing without invalidating
//! in-flight readers.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::PathBuf;
use std::sync::{Arc, PoisonError, RwLock};

use ontology_core::error::{Error, Result};
use ontology_core::graph::GraphKind;
use ontology_core::id::Id;
use ontology_core::model::GraphName;
use ontology_rdf::OxigraphStore;
use serde::Serialize;

use crate::registry::RegistryModel;

/// Registry subject of one physical store, e.g. `urn:onto:store:dbpedia`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct GraphStoreId(String);

impl GraphStoreId {
    /// Validates and wraps an absolute IRI.
    ///
    /// # Errors
    ///
    /// Returns [`Error::GraphRegistry`] when the value is not an absolute IRI.
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        Id::new(value.clone()).map_err(|_| Error::GraphRegistry {
            detail: format!("store id {value:?} is not an absolute IRI"),
        })?;
        Ok(Self(value))
    }

    /// Wraps a value that was already validated.
    #[must_use]
    pub fn from_validated(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Returns the IRI text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for GraphStoreId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Monotonic registry generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize)]
#[serde(transparent)]
pub struct Generation(pub u64);

impl Generation {
    /// Returns the next generation.
    #[must_use]
    pub fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

impl fmt::Display for Generation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

/// Whether a store is the single writer or a read-only satellite.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum StoreRole {
    /// Main graph directory; hosts the registry and reserved graphs.
    Main,
    /// Additional read-mostly graph directory.
    Satellite,
}

impl StoreRole {
    /// Canonical name used by the registry vocabulary.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Main => "main",
            Self::Satellite => "satellite",
        }
    }

    /// Parses the vocabulary value.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "main" => Some(Self::Main),
            "satellite" => Some(Self::Satellite),
            _ => None,
        }
    }
}

/// Registered write access.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreAccess {
    /// Queries and writes are allowed.
    ReadWrite,
    /// Queries are allowed; writes are rejected.
    ReadOnly,
}

impl StoreAccess {
    /// Canonical name used by the registry vocabulary.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ReadWrite => "rw",
            Self::ReadOnly => "ro",
        }
    }

    /// Parses the vocabulary value.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "rw" | "readwrite" | "read-write" => Some(Self::ReadWrite),
            "ro" | "readonly" | "read-only" => Some(Self::ReadOnly),
            _ => None,
        }
    }
}

/// How a satellite maps its registered graph onto the engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreLayout {
    /// Data lives in the unnamed graph, which lets the engine fuse `SERVICE`.
    Single,
    /// Data lives in its own named graph.
    Named,
}

impl StoreLayout {
    /// Canonical name used by the registry vocabulary.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Single => "single",
            Self::Named => "named",
        }
    }

    /// Parses the vocabulary value.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "single" => Some(Self::Single),
            "named" => Some(Self::Named),
            _ => None,
        }
    }
}

/// Runtime health of a store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StoreHealth {
    /// The store is open and serving.
    Ready,
    /// The store could not be opened; its graphs read as empty.
    Degraded {
        /// Why the store is unavailable.
        reason: String,
    },
}

impl StoreHealth {
    /// Returns whether the store is ready.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready)
    }
}

/// One open engine handle plus its registry metadata.
pub struct StoreHandle {
    /// Registry subject.
    pub id: GraphStoreId,
    /// Absolute directory.
    pub path: PathBuf,
    /// Path as written in the registry document.
    pub declared_path: PathBuf,
    /// Main or satellite.
    pub role: StoreRole,
    /// Registered access.
    pub access: StoreAccess,
    /// Logical layout.
    pub layout: StoreLayout,
    /// Host-election tiebreaker.
    pub mount_order: i64,
    /// Logical graphs hosted here.
    pub graphs: Vec<Id>,
    /// Namespaces owned through this store's graphs.
    pub namespaces: Vec<String>,
    health: RwLock<StoreHealth>,
    inner: RwLock<Option<Arc<OxigraphStore>>>,
}

impl fmt::Debug for StoreHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StoreHandle")
            .field("id", &self.id)
            .field("path", &self.path)
            .field("role", &self.role)
            .field("layout", &self.layout)
            .finish()
    }
}

impl StoreHandle {
    /// Builds a handle around an already-open store.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        id: GraphStoreId,
        path: PathBuf,
        declared_path: PathBuf,
        role: StoreRole,
        access: StoreAccess,
        layout: StoreLayout,
        mount_order: i64,
        graphs: Vec<Id>,
        namespaces: Vec<String>,
        health: StoreHealth,
        store: Option<Arc<OxigraphStore>>,
    ) -> Self {
        Self {
            id,
            path,
            declared_path,
            role,
            access,
            layout,
            mount_order,
            graphs,
            namespaces,
            health: RwLock::new(health),
            inner: RwLock::new(store),
        }
    }

    /// Returns the current health.
    #[must_use]
    pub fn health(&self) -> StoreHealth {
        self.health
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Returns whether the store is open.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.health().is_ready()
    }

    /// Runs `operation` with a shared handle held for its whole duration.
    ///
    /// Holding the read guard is what lets [`StoreHandle::with_writable`] wait
    /// for in-flight queries before it closes the read-only handle.
    ///
    /// # Errors
    ///
    /// Propagates the operation's error, or returns [`Error::GraphOpen`] when
    /// the store is degraded.
    pub fn with_store<T>(&self, operation: impl FnOnce(&OxigraphStore) -> Result<T>) -> Result<T> {
        let guard = self.inner.read().unwrap_or_else(PoisonError::into_inner);
        match guard.as_ref() {
            Some(store) => operation(store),
            None => Err(Error::GraphOpen {
                path: self.path.clone(),
                detail: format!("store {} is not available", self.id),
            }),
        }
    }

    /// Closes the read-only handle, reopens read-write, runs `operation`,
    /// flushes, and reopens read-only.
    ///
    /// The whole sequence holds the write guard, so no query can observe the
    /// directory while it is writable.
    ///
    /// # Errors
    ///
    /// Propagates open, operation, and flush failures.
    pub fn with_writable<T>(
        &self,
        operation: impl FnOnce(&OxigraphStore) -> Result<T>,
    ) -> Result<T> {
        if self.role == StoreRole::Main {
            return self.with_store(operation);
        }
        let mut slot = self.inner.write().unwrap_or_else(PoisonError::into_inner);
        if let Some(previous) = slot.take() {
            drop(previous);
        }
        let opened = OxigraphStore::open(&self.path)?;
        *slot = Some(Arc::new(opened));
        let outcome = {
            let store = slot.as_ref().expect("store was just opened");
            operation(store.as_ref())
        };
        let flush = slot.as_ref().map_or(Ok(()), |store| store.flush());
        slot.take();
        let reopened = OxigraphStore::open_read_only(&self.path).map(Arc::new);
        match reopened {
            Ok(store) => {
                *slot = Some(store);
                *self.health.write().unwrap_or_else(PoisonError::into_inner) = StoreHealth::Ready;
            }
            Err(error) => return Err(error),
        }
        let value = outcome?;
        flush?;
        Ok(value)
    }

    /// Number of stored quads, or zero when the store is unavailable.
    #[must_use]
    pub fn quad_count(&self) -> u64 {
        self.with_store(|store| store.quad_count())
            .map_or(0, |count| count as u64)
    }
}

/// One namespace-to-graph binding.
#[derive(Debug, Clone)]
pub struct NamespaceBinding {
    /// Namespace IRI.
    pub namespace: String,
    /// Graph that owns it.
    pub graph: Id,
    /// Owning store.
    pub store: GraphStoreId,
    /// Longer namespaces win first; then higher priority.
    pub priority: i32,
}

/// A graph and the store that answers for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphRoute {
    /// Logical graph IRI.
    pub graph: Id,
    /// Owning store.
    pub store: GraphStoreId,
}

/// Operator-facing description of one store.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreStatus {
    /// Store identifier.
    pub id: String,
    /// Absolute directory.
    pub path: String,
    /// `main` or `satellite`.
    pub role: String,
    /// `rw` or `ro`.
    pub access: String,
    /// `single` or `named`.
    pub layout: String,
    /// Runtime health.
    pub health: StoreHealth,
    /// Graphs hosted here.
    pub graphs: Vec<String>,
    /// Namespaces owned here.
    pub namespaces: Vec<String>,
    /// Stored quads.
    pub quads: u64,
}

/// Immutable registry view at one generation.
#[derive(Debug)]
pub struct CatalogSnapshot {
    /// Generation this view was published at.
    pub generation: Generation,
    /// Main store, always open.
    pub main: Arc<StoreHandle>,
    /// Every registered store, including main.
    pub stores: BTreeMap<GraphStoreId, Arc<StoreHandle>>,
    /// The parsed registry, when running in registry mode.
    pub model: Option<RegistryModel>,
    /// True when no registry is configured.
    pub single: bool,
    graph_index: BTreeMap<Id, Arc<StoreHandle>>,
    graph_namespaces: BTreeMap<Id, Vec<String>>,
    namespace_index: Vec<NamespaceBinding>,
}

impl CatalogSnapshot {
    /// Builds the snapshot used when no registry is configured.
    #[must_use]
    pub fn single(main: Arc<StoreHandle>, generation: Generation) -> Self {
        let mut stores = BTreeMap::new();
        stores.insert(main.id.clone(), Arc::clone(&main));
        Self {
            generation,
            main,
            stores,
            model: None,
            single: true,
            graph_index: BTreeMap::new(),
            graph_namespaces: BTreeMap::new(),
            namespace_index: Vec::new(),
        }
    }

    /// Assembles a registry snapshot from its indexes.
    #[must_use]
    pub fn assemble(
        generation: Generation,
        main: Arc<StoreHandle>,
        stores: BTreeMap<GraphStoreId, Arc<StoreHandle>>,
        graph_index: BTreeMap<Id, Arc<StoreHandle>>,
        graph_namespaces: BTreeMap<Id, Vec<String>>,
        mut namespace_index: Vec<NamespaceBinding>,
        model: RegistryModel,
    ) -> Self {
        namespace_index.sort_by(|left, right| {
            right
                .namespace
                .len()
                .cmp(&left.namespace.len())
                .then_with(|| right.priority.cmp(&left.priority))
                .then_with(|| left.graph.cmp(&right.graph))
        });
        Self {
            generation,
            main,
            stores,
            model: Some(model),
            single: false,
            graph_index,
            graph_namespaces,
            namespace_index,
        }
    }

    /// Looks up a store by identifier.
    #[must_use]
    pub fn store(&self, id: &GraphStoreId) -> Option<&Arc<StoreHandle>> {
        self.stores.get(id)
    }

    /// Resolves a graph to its owning store.
    ///
    /// # Errors
    ///
    /// Returns [`Error::GraphRegistry`] when the graph is not registered.
    pub fn resolve_graph(&self, graph: &Id) -> Result<Arc<StoreHandle>> {
        self.graph_index
            .get(graph)
            .cloned()
            .ok_or_else(|| Error::GraphRegistry {
                detail: format!("graph {graph} is not registered"),
            })
    }

    /// Resolves an instance IRI through its longest matching namespace.
    ///
    /// # Errors
    ///
    /// Propagates graph resolution failures.
    pub fn resolve_namespace(&self, iri: &Id) -> Result<Arc<StoreHandle>> {
        let text = iri.as_str();
        match self
            .namespace_index
            .iter()
            .find(|binding| text.starts_with(&binding.namespace))
        {
            Some(binding) => self.resolve_graph(&binding.graph),
            None => Ok(Arc::clone(&self.main)),
        }
    }

    /// Returns the graph that owns `namespace` exactly, falling back to the
    /// asserted graph.
    ///
    /// # Errors
    ///
    /// Returns [`Error::GraphRegistry`] when the fallback graph is malformed.
    pub fn graph_for_namespace(&self, namespace: &str) -> Result<Id> {
        if let Some(binding) = self
            .namespace_index
            .iter()
            .find(|binding| binding.namespace == namespace)
        {
            return Ok(binding.graph.clone());
        }
        self.fallback_graph()
    }

    /// Returns the fallback graph (`data:asserted`).
    ///
    /// # Errors
    ///
    /// Returns [`Error::GraphRegistry`] when the reserved IRI is malformed.
    pub fn fallback_graph(&self) -> Result<Id> {
        Ok(GraphKind::Asserted.iri())
    }

    /// Namespaces bound to a graph.
    #[must_use]
    pub fn graph_namespaces(&self, graph: &Id) -> Vec<String> {
        self.graph_namespaces
            .get(graph)
            .cloned()
            .unwrap_or_default()
    }

    /// Every enabled registered graph, in IRI order.
    #[must_use]
    pub fn enabled_graphs(&self) -> Vec<Id> {
        self.graph_index.keys().cloned().collect()
    }

    /// Routes a set of graphs to their stores.
    #[must_use]
    pub fn routes_for(&self, graphs: &BTreeSet<Id>) -> Vec<GraphRoute> {
        graphs
            .iter()
            .filter_map(|graph| {
                self.graph_index.get(graph).map(|handle| GraphRoute {
                    graph: graph.clone(),
                    store: handle.id.clone(),
                })
            })
            .collect()
    }

    /// Translates a logical graph into the engine graph used by `handle`.
    #[must_use]
    pub fn engine_graph(&self, graph: &Id, handle: &StoreHandle) -> GraphName {
        match handle.layout {
            StoreLayout::Single => GraphName::Default,
            StoreLayout::Named => GraphName::Iri(graph.clone()),
        }
    }

    /// Builds the absolute `oxigraph://` IRI the router emits for `handle`.
    #[must_use]
    pub fn service_iri(&self, handle: &StoreHandle) -> String {
        format!("oxigraph://{}", handle.path.display())
    }

    /// Operator-facing status for every store.
    #[must_use]
    pub fn status(&self) -> Vec<StoreStatus> {
        self.stores
            .values()
            .map(|handle| {
                let graphs = self
                    .graph_index
                    .iter()
                    .filter(|(_, owner)| owner.id == handle.id)
                    .map(|(graph, _)| graph.to_string())
                    .collect();
                let namespaces = self
                    .namespace_index
                    .iter()
                    .filter(|binding| binding.store == handle.id)
                    .map(|binding| binding.namespace.clone())
                    .collect();
                StoreStatus {
                    id: handle.id.to_string(),
                    path: handle.path.display().to_string(),
                    role: handle.role.as_str().to_owned(),
                    access: handle.access.as_str().to_owned(),
                    layout: handle.layout.as_str().to_owned(),
                    health: handle.health(),
                    graphs,
                    namespaces,
                    quads: handle.quad_count(),
                }
            })
            .collect()
    }
}

/// Read-side discovery, cheap after one lock acquisition.
pub trait GraphCatalog: Send + Sync {
    /// Current snapshot.
    fn snapshot(&self) -> Arc<CatalogSnapshot>;

    /// Current generation.
    fn generation(&self) -> Generation;

    /// Resolves a graph to its store.
    ///
    /// # Errors
    ///
    /// Returns [`Error::GraphRegistry`] when the graph is not registered.
    fn resolve_graph(&self, graph: &Id) -> Result<Arc<StoreHandle>>;

    /// Resolves an instance IRI to the store that owns its namespace.
    ///
    /// # Errors
    ///
    /// Propagates resolution failures.
    fn resolve_namespace(&self, iri: &Id) -> Result<Arc<StoreHandle>>;

    /// Returns the graph owning a namespace.
    ///
    /// # Errors
    ///
    /// Propagates resolution failures.
    fn graph_for_namespace(&self, namespace: &str) -> Result<Id>;

    /// Routes a set of graphs.
    fn routes_for(&self, graphs: &BTreeSet<Id>) -> Vec<GraphRoute>;

    /// Operator-facing status.
    fn status(&self) -> Vec<StoreStatus>;
}

/// Shared catalog with a publish/read snapshot cell.
pub struct GraphCatalogImpl {
    snapshot: RwLock<Arc<CatalogSnapshot>>,
}

impl GraphCatalogImpl {
    /// Starts from `snapshot`.
    #[must_use]
    pub fn new(snapshot: Arc<CatalogSnapshot>) -> Self {
        Self {
            snapshot: RwLock::new(snapshot),
        }
    }

    /// Returns the current snapshot.
    #[must_use]
    pub fn current(&self) -> Arc<CatalogSnapshot> {
        Arc::clone(&self.snapshot.read().unwrap_or_else(PoisonError::into_inner))
    }

    /// Publishes a new snapshot; in-flight readers keep the previous one.
    pub fn publish(&self, snapshot: Arc<CatalogSnapshot>) {
        *self
            .snapshot
            .write()
            .unwrap_or_else(PoisonError::into_inner) = snapshot;
    }
}

impl GraphCatalog for GraphCatalogImpl {
    fn snapshot(&self) -> Arc<CatalogSnapshot> {
        self.current()
    }

    fn generation(&self) -> Generation {
        self.current().generation
    }

    fn resolve_graph(&self, graph: &Id) -> Result<Arc<StoreHandle>> {
        self.current().resolve_graph(graph)
    }

    fn resolve_namespace(&self, iri: &Id) -> Result<Arc<StoreHandle>> {
        self.current().resolve_namespace(iri)
    }

    fn graph_for_namespace(&self, namespace: &str) -> Result<Id> {
        self.current().graph_for_namespace(namespace)
    }

    fn routes_for(&self, graphs: &BTreeSet<Id>) -> Vec<GraphRoute> {
        self.current().routes_for(graphs)
    }

    fn status(&self) -> Vec<StoreStatus> {
        self.current().status()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::snapshot;

    #[test]
    fn resolves_namespaces_with_the_longest_prefix() {
        let snapshot = snapshot();
        let team = Id::new("https://acme.example/team/alice").expect("iri");
        assert_eq!(
            snapshot
                .resolve_namespace(&team)
                .expect("store")
                .id
                .as_str(),
            "urn:onto:store:main"
        );

        let product = Id::new("https://products.example/Widget").expect("iri");
        assert_eq!(
            snapshot
                .resolve_namespace(&product)
                .expect("store")
                .id
                .as_str(),
            "urn:onto:store:products"
        );

        let unmapped = Id::new("https://unknown.example/Thing").expect("iri");
        assert_eq!(
            snapshot
                .resolve_namespace(&unmapped)
                .expect("store")
                .id
                .as_str(),
            "urn:onto:store:main"
        );
    }

    #[test]
    fn maps_namespaces_to_graphs_and_falls_back() {
        let snapshot = snapshot();
        assert_eq!(
            snapshot
                .graph_for_namespace("https://products.example/")
                .expect("graph")
                .as_str(),
            "urn:onto:data:products"
        );
        assert_eq!(
            snapshot
                .graph_for_namespace("https://nothing.example/")
                .expect("fallback")
                .as_str(),
            GraphKind::Asserted.iri_str()
        );
    }

    #[test]
    fn reports_status_for_every_store() {
        let status = snapshot().status();
        assert_eq!(status.len(), 3);
        assert!(status.iter().any(|store| {
            store.id == "urn:onto:store:products"
                && store.layout == "named"
                && store.graphs == vec!["urn:onto:data:products".to_owned()]
        }));
    }

    #[test]
    fn translates_single_layout_graphs_to_the_default_graph() {
        let snapshot = snapshot();
        let dbpedia = snapshot
            .resolve_graph(&Id::new("urn:onto:external:dbpedia").expect("graph"))
            .expect("store");
        assert_eq!(
            snapshot.engine_graph(
                &Id::new("urn:onto:external:dbpedia").expect("graph"),
                &dbpedia
            ),
            GraphName::Default
        );
        assert_eq!(
            snapshot.service_iri(&dbpedia),
            "oxigraph:///srv/onto/data/graphs/dbpedia"
        );
    }
}
