//! The RDF/XML root configuration: model, extraction, validation, and diff.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use ontology_core::error::{Error, Result};
use ontology_core::format::RdfFormat;
use ontology_core::graph::GraphKind;
use ontology_core::id::Id;
use ontology_core::model::{Quad, QuadPattern, Term};
use ontology_core::namespace::{NamespaceRegistry, RDF_TYPE, SYSTEM};
use ontology_core::rdf::RdfStore;
use ontology_rdf::OxigraphStore;

use crate::catalog::{GraphStoreId, StoreAccess, StoreLayout, StoreRole};
use crate::pool;

/// Registry schema version understood by this build.
pub const REGISTRY_VERSION: u32 = 1;

type Document = BTreeMap<String, Vec<(Id, Term)>>;

/// One graph registered on a store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Registration {
    /// Logical graph IRI.
    pub graph: Id,
    /// Namespaces owned through this graph.
    pub namespaces: Vec<String>,
    /// Tie-breaker when two namespaces are prefixes of one IRI.
    pub priority: i32,
    /// Disabled registrations stay in the file but are excluded at runtime.
    pub enabled: bool,
    /// When false, a missing directory degrades to empty instead of failing.
    pub required: bool,
    /// Refuse fused evaluation when the satellite holds blank nodes.
    pub strict_blank_nodes: bool,
}

/// One physical store described by the registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreSpec {
    /// Registry subject.
    pub id: GraphStoreId,
    /// `main` or `satellite`.
    pub role: StoreRole,
    /// Path as written in the document.
    pub declared_path: PathBuf,
    /// Registered access.
    pub access: StoreAccess,
    /// Logical layout.
    pub layout: StoreLayout,
    /// Host-election tiebreaker.
    pub mount_order: i64,
    /// Hosted graphs.
    pub registrations: Vec<Registration>,
}

/// Parsed registry document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryModel {
    /// Schema version.
    pub version: u32,
    /// The main store.
    pub main: GraphStoreId,
    /// Every store, including main.
    pub stores: Vec<StoreSpec>,
}

fn onto(local: &str) -> Id {
    NamespaceRegistry::onto(local)
}

fn node_key(term: &Term) -> Option<String> {
    match term {
        Term::Iri(id) => Some(id.as_str().to_owned()),
        Term::BlankNode(id) => Some(format!("_:{id}")),
        Term::Literal(_) => None,
    }
}

fn first<'a>(props: &'a [(Id, Term)], predicate: &Id) -> Option<&'a Term> {
    props
        .iter()
        .find(|(candidate, _)| candidate == predicate)
        .map(|(_, object)| object)
}

fn literal(props: &[(Id, Term)], predicate: &Id) -> Option<String> {
    match first(props, predicate)? {
        Term::Literal(literal) => Some(literal.value.clone()),
        Term::Iri(id) => Some(id.to_string()),
        Term::BlankNode(_) => None,
    }
}

fn literal_or(props: &[(Id, Term)], predicate: &Id, default: &str) -> String {
    literal(props, predicate).unwrap_or_else(|| default.to_owned())
}

fn iri(props: &[(Id, Term)], predicate: &Id) -> Option<Id> {
    first(props, predicate)?.as_iri().cloned()
}

fn iris(props: &[(Id, Term)], predicate: &Id) -> Vec<Id> {
    props
        .iter()
        .filter(|(candidate, _)| candidate == predicate)
        .filter_map(|(_, object)| object.as_iri().cloned())
        .collect()
}

fn literals(props: &[(Id, Term)], predicate: &Id) -> Vec<String> {
    props
        .iter()
        .filter(|(candidate, _)| candidate == predicate)
        .filter_map(|(_, object)| match object {
            Term::Literal(value) => Some(value.value.clone()),
            Term::Iri(value) => Some(value.to_string()),
            Term::BlankNode(_) => None,
        })
        .collect()
}

fn integer(props: &[(Id, Term)], predicate: &Id) -> Option<i64> {
    literal(props, predicate).and_then(|value| value.trim().parse().ok())
}

fn boolean(props: &[(Id, Term)], predicate: &Id, default: bool) -> bool {
    match literal(props, predicate) {
        Some(value) => matches!(value.trim().to_ascii_lowercase().as_str(), "true" | "1"),
        None => default,
    }
}

/// Parses an RDF/XML registry document.
///
/// `base` must be the document's `file://` IRI so relative `rdf:resource`
/// references resolve deterministically.
///
/// # Errors
///
/// Returns [`Error::GraphRegistry`] for a document that is not a single,
/// completely described registry, and propagates RDF parse failures.
pub fn parse_registry(data: &[u8], base: &str) -> Result<RegistryModel> {
    let store = OxigraphStore::in_memory()?;
    store.load_from_slice(RdfFormat::RdfXml, data, Some(base), None)?;
    extract(store.get(QuadPattern::new())?)
}

/// Extracts a registry model from already-materialised quads.
///
/// # Errors
///
/// Returns [`Error::GraphRegistry`] for a document that is not a single,
/// completely described registry.
pub fn extract(quads: Vec<Quad>) -> Result<RegistryModel> {
    let mut by_subject: Document = BTreeMap::new();
    for quad in quads {
        if let Some(key) = node_key(&quad.subject) {
            by_subject
                .entry(key)
                .or_default()
                .push((quad.predicate, quad.object));
        }
    }

    let registry_class = onto("GraphRegistry");
    let rdf_type = Id::new(RDF_TYPE)?;
    let registry_subjects: Vec<String> = by_subject
        .iter()
        .filter(|(_, props)| {
            props.iter().any(|(predicate, object)| {
                *predicate == rdf_type && object.as_iri() == Some(&registry_class)
            })
        })
        .map(|(subject, _)| subject.clone())
        .collect();
    if registry_subjects.len() != 1 {
        return Err(Error::GraphRegistry {
            detail: format!(
                "the document must describe exactly one onto:GraphRegistry, found {}",
                registry_subjects.len()
            ),
        });
    }
    let registry_subject = &registry_subjects[0];
    let registry_props = by_subject
        .get(registry_subject)
        .expect("registry subject came from the index");

    let version = u32::try_from(integer(registry_props, &onto("registryVersion")).unwrap_or(1))
        .map_err(|_| Error::GraphRegistry {
            detail: "onto:registryVersion must be a non-negative integer".to_owned(),
        })?;
    let main = iri(registry_props, &onto("mainStore")).ok_or_else(|| Error::GraphRegistry {
        detail: "the registry must declare exactly one onto:mainStore".to_owned(),
    })?;
    let main = GraphStoreId::from_validated(main.into_string());

    let entries = iris(registry_props, &onto("entry"));
    if entries.is_empty() {
        return Err(Error::GraphRegistry {
            detail: "the registry must list at least one onto:entry".to_owned(),
        });
    }

    let mut stores = Vec::with_capacity(entries.len());
    for entry in entries {
        let props = by_subject
            .get(entry.as_str())
            .ok_or_else(|| Error::GraphRegistry {
                detail: format!("store {entry} is listed but not described"),
            })?;
        stores.push(parse_store(entry, props, &by_subject)?);
    }

    Ok(RegistryModel {
        version,
        main,
        stores,
    })
}

fn parse_store(id: Id, props: &[(Id, Term)], document: &Document) -> Result<StoreSpec> {
    let role_value = literal(props, &onto("role")).ok_or_else(|| Error::GraphRegistry {
        detail: format!("store {id} has no onto:role"),
    })?;
    let role = StoreRole::parse(&role_value).ok_or_else(|| Error::GraphRegistry {
        detail: format!("store {id} has invalid onto:role {role_value:?}"),
    })?;

    let declared_path = literal(props, &onto("storePath")).ok_or_else(|| Error::GraphRegistry {
        detail: format!("store {id} has no onto:storePath"),
    })?;

    let default_access = if role == StoreRole::Main { "rw" } else { "ro" };
    let access_value = literal_or(props, &onto("access"), default_access);
    let access = StoreAccess::parse(&access_value).ok_or_else(|| Error::GraphRegistry {
        detail: format!("store {id} has invalid onto:access {access_value:?}"),
    })?;

    let layout_value = literal_or(props, &onto("layout"), "named");
    let layout = StoreLayout::parse(&layout_value).ok_or_else(|| Error::GraphRegistry {
        detail: format!("store {id} has invalid onto:layout {layout_value:?}"),
    })?;

    let mount_order = integer(props, &onto("mountOrder")).unwrap_or(0);

    let store_namespaces = literals(props, &onto("namespace"));
    let mut registrations = Vec::new();
    for node in props
        .iter()
        .filter(|(predicate, _)| *predicate == onto("registration"))
        .filter_map(|(_, object)| node_key(object))
    {
        let registration_props = document.get(&node).ok_or_else(|| Error::GraphRegistry {
            detail: format!("store {id} describes a registration without statements"),
        })?;
        registrations.push(parse_registration(&id, &node, registration_props)?);
    }

    match store_namespaces.len() {
        0 => {}
        _ if registrations.len() == 1 => {
            registrations[0].namespaces.extend(store_namespaces);
        }
        _ => {
            return Err(Error::GraphRegistry {
                detail: format!(
                    "store {id} declares onto:namespace directly, which requires exactly one registration"
                ),
            });
        }
    }

    Ok(StoreSpec {
        id: GraphStoreId::from_validated(id.into_string()),
        role,
        declared_path: PathBuf::from(declared_path),
        access,
        layout,
        mount_order,
        registrations,
    })
}

fn parse_registration(store: &Id, node: &str, props: &[(Id, Term)]) -> Result<Registration> {
    let graph = iri(props, &onto("graphIri")).ok_or_else(|| Error::GraphRegistry {
        detail: format!("registration {node} on store {store} has no onto:graphIri"),
    })?;
    Ok(Registration {
        graph,
        namespaces: literals(props, &onto("namespace")),
        priority: i32::try_from(integer(props, &onto("priority")).unwrap_or(0)).unwrap_or(0),
        enabled: boolean(props, &onto("enabled"), true),
        required: boolean(props, &onto("required"), true),
        strict_blank_nodes: boolean(props, &onto("strictBlankNodes"), false),
    })
}

impl RegistryModel {
    /// Validates every registry invariant, reporting all failures at once.
    ///
    /// # Errors
    ///
    /// Returns [`Error::GraphRegistry`] listing every conflict.
    pub fn validate(&self, root: &Path, storage_path: &Path) -> Result<()> {
        let mut errors = Vec::new();
        if self.version != REGISTRY_VERSION {
            errors.push(format!(
                "onto:registryVersion {} is not supported (expected {REGISTRY_VERSION})",
                self.version
            ));
        }
        if self.stores.is_empty() {
            errors.push("the registry contains no stores".to_owned());
        }

        let mut main_stores: Vec<&StoreSpec> = self
            .stores
            .iter()
            .filter(|store| store.role == StoreRole::Main)
            .collect();
        if main_stores.len() != 1 {
            errors.push(format!(
                "exactly one store must have onto:role \"main\", found {}",
                main_stores.len()
            ));
        }
        main_stores.retain(|store| store.id == self.main);
        let Some(main) = main_stores.first() else {
            errors.push(format!(
                "onto:mainStore {} does not name a store with onto:role \"main\"",
                self.main
            ));
            return Err(Error::GraphRegistry {
                detail: errors.join("; "),
            });
        };

        if main.access != StoreAccess::ReadWrite {
            errors.push(format!("main store {} must be writable", main.id));
        }
        let expected_main = pool::absolute_path(storage_path);
        let declared_main = pool::absolute_path(&main.declared_path);
        if expected_main != declared_main {
            errors.push(format!(
                "registry main path {} does not match storage.path {}",
                declared_main.display(),
                expected_main.display()
            ));
        }

        let mut graphs: BTreeMap<Id, &GraphStoreId> = BTreeMap::new();
        let mut namespace_owners: BTreeMap<String, Id> = BTreeMap::new();
        let mut mount_orders: BTreeMap<i64, GraphStoreId> = BTreeMap::new();
        let mut seen_stores = BTreeSet::new();

        for store in &self.stores {
            if !seen_stores.insert(store.id.clone()) {
                errors.push(format!("duplicate store {}", store.id));
            }
            if let Some(existing) = mount_orders.insert(store.mount_order, store.id.clone()) {
                errors.push(format!(
                    "stores {existing} and {} share onto:mountOrder {}",
                    store.id, store.mount_order
                ));
            }
            if store.role == StoreRole::Satellite {
                if let Err(error) = pool::resolve_store_path(root, &store.declared_path) {
                    errors.push(format!("store {}: {error}", store.id));
                }
                if store.layout == StoreLayout::Single && store.registrations.len() != 1 {
                    errors.push(format!(
                        "store {} uses layout \"single\" and must register exactly one graph, found {}",
                        store.id,
                        store.registrations.len()
                    ));
                }
            }
            for registration in &store.registrations {
                if let Some(existing) = graphs.insert(registration.graph.clone(), &store.id) {
                    errors.push(format!(
                        "graph {} is claimed by both {} and {}",
                        registration.graph, existing, store.id
                    ));
                }
                let reserved = GraphKind::from_iri(&registration.graph).is_some();
                if reserved && store.role != StoreRole::Main {
                    errors.push(format!(
                        "reserved graph {} may only be owned by the main store",
                        registration.graph
                    ));
                }
                if registration.graph.as_str().starts_with(SYSTEM) && store.role != StoreRole::Main
                {
                    errors.push(format!(
                        "system graph {} may only be owned by the main store",
                        registration.graph
                    ));
                }
                for namespace in &registration.namespaces {
                    if Id::new(namespace.as_str()).is_err() {
                        errors.push(format!(
                            "graph {} declares a namespace {namespace:?} that is not an absolute IRI",
                            registration.graph
                        ));
                        continue;
                    }
                    match namespace_owners.get(namespace) {
                        Some(owner) if owner != &registration.graph => errors.push(format!(
                            "namespace {namespace} is claimed by both {owner} and {}",
                            registration.graph
                        )),
                        Some(_) => {}
                        None => {
                            namespace_owners.insert(namespace.clone(), registration.graph.clone());
                        }
                    }
                }
            }
            if store.registrations.is_empty() {
                errors.push(format!("store {} registers no graphs", store.id));
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(Error::GraphRegistry {
                detail: errors.join("; "),
            })
        }
    }

    /// Returns the store spec for `id`.
    #[must_use]
    pub fn store(&self, id: &GraphStoreId) -> Option<&StoreSpec> {
        self.stores.iter().find(|store| &store.id == id)
    }

    /// Every enabled graph across stores.
    #[must_use]
    pub fn enabled_graphs(&self) -> Vec<Id> {
        self.stores
            .iter()
            .flat_map(|store| &store.registrations)
            .filter(|registration| registration.enabled)
            .map(|registration| registration.graph.clone())
            .collect()
    }
}

/// Difference between two registry generations.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RegistryDiff {
    /// Store identifiers that appeared.
    pub added: Vec<String>,
    /// Store identifiers whose definition changed.
    pub changed: Vec<String>,
    /// Store identifiers that disappeared.
    pub removed: Vec<String>,
    /// Stores that are unchanged.
    pub unchanged: usize,
}

/// Compares two models by store.
#[must_use]
pub fn diff(previous: Option<&RegistryModel>, next: &RegistryModel) -> RegistryDiff {
    let mut diff = RegistryDiff::default();
    let Some(previous) = previous else {
        diff.added = next
            .stores
            .iter()
            .map(|store| store.id.to_string())
            .collect();
        return diff;
    };
    let old: BTreeMap<&GraphStoreId, &StoreSpec> = previous
        .stores
        .iter()
        .map(|store| (&store.id, store))
        .collect();
    let new: BTreeMap<&GraphStoreId, &StoreSpec> =
        next.stores.iter().map(|store| (&store.id, store)).collect();
    for (id, spec) in &new {
        match old.get(id) {
            None => diff.added.push(id.to_string()),
            Some(previous_spec) if previous_spec == spec => diff.unchanged += 1,
            Some(_) => diff.changed.push(id.to_string()),
        }
    }
    for id in old.keys() {
        if !new.contains_key(id) {
            diff.removed.push(id.to_string());
        }
    }
    diff
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../tests/fixtures/registry.rdf");

    fn model() -> RegistryModel {
        parse_registry(FIXTURE.as_bytes(), "file:///srv/onto/config/graphs.rdf")
            .expect("fixture parses")
    }

    #[test]
    fn extracts_stores_and_registrations() {
        let model = model();
        assert_eq!(model.version, 1);
        assert_eq!(model.main.as_str(), "urn:onto:store:main");
        assert_eq!(model.stores.len(), 3);

        let main = model
            .store(&GraphStoreId::new("urn:onto:store:main").expect("id"))
            .expect("main");
        assert_eq!(main.role, StoreRole::Main);
        assert_eq!(main.registrations.len(), 4);

        let products = model
            .store(&GraphStoreId::new("urn:onto:store:products").expect("id"))
            .expect("products");
        assert_eq!(products.layout, StoreLayout::Named);
        assert_eq!(products.registrations.len(), 1);
        assert_eq!(
            products.registrations[0].graph.as_str(),
            "urn:onto:data:products"
        );
        assert_eq!(
            products.registrations[0].namespaces,
            vec!["https://products.example/".to_owned()]
        );
        assert_eq!(products.registrations[0].priority, 5);

        let dbpedia = model
            .store(&GraphStoreId::new("urn:onto:store:dbpedia").expect("id"))
            .expect("dbpedia");
        assert_eq!(dbpedia.layout, StoreLayout::Single);
        assert_eq!(
            dbpedia.registrations[0].graph.as_str(),
            "urn:onto:external:dbpedia"
        );
    }

    #[test]
    fn validates_the_shipped_configuration() {
        let shipped = include_str!("../../../config/graphs.rdf");
        let model = parse_registry(shipped.as_bytes(), "file:///srv/onto/config/graphs.rdf")
            .expect("shipped configuration parses");
        model
            .validate(Path::new("./data/graphs"), Path::new("./data/ontology"))
            .expect("shipped configuration is valid");
    }

    #[test]
    fn validates_the_reference_document() {
        model()
            .validate(Path::new("./data/graphs"), Path::new("./data/ontology"))
            .expect("fixture is valid");
    }

    #[test]
    fn reports_conflicting_namespaces() {
        let conflicting = FIXTURE.replace("https://products.example/", "https://acme.example/");
        let model = parse_registry(conflicting.as_bytes(), "file:///srv/onto/config/graphs.rdf")
            .expect("parses");
        let error = model
            .validate(Path::new("./data/graphs"), Path::new("./data/ontology"))
            .expect_err("namespace conflict");
        assert!(error.to_string().contains("claimed by both"), "{error}");
    }

    #[test]
    fn rejects_reserved_graphs_on_satellites() {
        let conflicting = FIXTURE.replace("urn:onto:data:products", "urn:onto:system:schema");
        let model = parse_registry(conflicting.as_bytes(), "file:///srv/onto/config/graphs.rdf")
            .expect("parses");
        let error = model
            .validate(Path::new("./data/graphs"), Path::new("./data/ontology"))
            .expect_err("reserved graph on a satellite");
        assert!(
            error.to_string().contains("reserved graph")
                || error.to_string().contains("claimed by both"),
            "{error}"
        );
    }

    #[test]
    fn diffs_by_store() {
        let before = model();
        let mut after = model();
        after
            .stores
            .retain(|store| store.id.as_str() != "urn:onto:store:products");
        let diff = diff(Some(&before), &after);
        assert_eq!(diff.removed, vec!["urn:onto:store:products".to_owned()]);
        assert_eq!(diff.unchanged, 2);
    }
}
