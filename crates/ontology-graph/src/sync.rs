//! Synchronising the RDF/XML registry into the main graph.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use chrono::Utc;
use ontology_core::error::{Error, Result};
use ontology_core::id::Id;
use ontology_core::model::{GraphName, Literal, Quad, QuadPattern, Term};
use ontology_core::namespace::{DCTERMS, PROV};
use ontology_core::rdf::RdfStore;
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::catalog::{
    CatalogSnapshot, Generation, NamespaceBinding, StoreHandle, StoreHealth, StoreRole,
};
use crate::pool;
use crate::registry::RegistryModel;
use crate::REGISTRY_GRAPH_IRI;

/// Outcome of one registry sync.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphSyncReport {
    /// Published generation.
    pub generation: u64,
    /// Stores that appeared.
    pub added: Vec<String>,
    /// Stores whose definition changed.
    pub changed: Vec<String>,
    /// Stores that disappeared.
    pub removed: Vec<String>,
    /// Number of unchanged stores.
    pub unchanged: usize,
    /// Stores that are registered but unavailable.
    pub degraded: Vec<String>,
}

/// A registry document that parsed successfully.
#[derive(Debug, Clone)]
pub struct LoadedRegistry {
    /// Parsed model.
    pub model: RegistryModel,
    /// `sha256` of the source bytes, lowercase hex.
    pub digest: String,
}

/// Hashes bytes with SHA-256.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// Reads and parses the registry file.
///
/// # Errors
///
/// Returns [`Error::GraphRegistry`] when the file cannot be read and propagates
/// parse failures.
pub fn load_registry_file(path: &Path) -> Result<LoadedRegistry> {
    let bytes = std::fs::read(path).map_err(|error| Error::GraphRegistry {
        detail: format!("could not read {}: {error}", path.display()),
    })?;
    let absolute = pool::absolute_path(path);
    let base = format!("file://{}", absolute.display());
    let model = crate::registry::parse_registry(&bytes, &base)?;
    Ok(LoadedRegistry {
        model,
        digest: sha256_hex(&bytes),
    })
}

/// Reads the registry materialised in `urn:onto:system:graphs`.
///
/// # Errors
///
/// Propagates storage and extraction failures.
pub fn load_persisted_registry(main: &StoreHandle) -> Result<Option<RegistryModel>> {
    let quads =
        main.with_store(|store| store.get(QuadPattern::new().with_graph(registry_graph())))?;
    if quads.is_empty() {
        Ok(None)
    } else {
        crate::registry::extract(quads).map(Some)
    }
}

/// Builds the catalog snapshot for `model`, opening every satellite read-only.
///
/// Returns the snapshot and the identifiers of degraded stores.
///
/// # Errors
///
/// Returns [`Error::GraphOpen`] for a missing required store or a marker
/// mismatch, and propagates path validation failures.
pub fn build_snapshot(
    main: Arc<StoreHandle>,
    model: &RegistryModel,
    root: &Path,
    generation: Generation,
) -> Result<(CatalogSnapshot, Vec<String>)> {
    let mut stores = BTreeMap::new();
    let mut graph_index = BTreeMap::new();
    let mut graph_namespaces = BTreeMap::new();
    let mut bindings = Vec::new();
    let mut degraded = Vec::new();

    for spec in &model.stores {
        let handle = if spec.id == main.id {
            let handle = Arc::clone(&main);
            stores.insert(spec.id.clone(), Arc::clone(&handle));
            handle
        } else {
            let path = pool::resolve_store_path(root, &spec.declared_path)?;
            let graphs: Vec<Id> = spec
                .registrations
                .iter()
                .map(|registration| registration.graph.clone())
                .collect();
            let namespaces: Vec<String> = spec
                .registrations
                .iter()
                .flat_map(|registration| registration.namespaces.clone())
                .collect();
            let required = spec
                .registrations
                .iter()
                .any(|registration| registration.required && registration.enabled);
            let handle = if !path.exists() {
                if required {
                    return Err(Error::GraphOpen {
                        path,
                        detail: format!("store {} directory does not exist", spec.id),
                    });
                }
                degraded.push(spec.id.to_string());
                Arc::new(StoreHandle::new(
                    spec.id.clone(),
                    path.clone(),
                    spec.declared_path.clone(),
                    StoreRole::Satellite,
                    spec.access,
                    spec.layout,
                    spec.mount_order,
                    graphs,
                    namespaces,
                    StoreHealth::Degraded {
                        reason: "directory missing".to_owned(),
                    },
                    None,
                ))
            } else {
                let store = pool::open_satellite(&path, &spec.id, spec.layout, &graphs)?;
                Arc::new(StoreHandle::new(
                    spec.id.clone(),
                    path.clone(),
                    spec.declared_path.clone(),
                    StoreRole::Satellite,
                    spec.access,
                    spec.layout,
                    spec.mount_order,
                    graphs,
                    namespaces,
                    StoreHealth::Ready,
                    Some(Arc::new(store)),
                ))
            };
            stores.insert(spec.id.clone(), Arc::clone(&handle));
            handle
        };

        for registration in spec
            .registrations
            .iter()
            .filter(|registration| registration.enabled)
        {
            graph_index.insert(registration.graph.clone(), Arc::clone(&handle));
            graph_namespaces.insert(registration.graph.clone(), registration.namespaces.clone());
            for namespace in &registration.namespaces {
                bindings.push(NamespaceBinding {
                    namespace: namespace.clone(),
                    graph: registration.graph.clone(),
                    store: spec.id.clone(),
                    priority: registration.priority,
                });
            }
        }
    }

    let snapshot = CatalogSnapshot::assemble(
        generation,
        main,
        stores,
        graph_index,
        graph_namespaces,
        bindings,
        model.clone(),
    );
    Ok((snapshot, degraded))
}

/// Replaces `urn:onto:system:graphs` with the materialised registry in one
/// transaction.
///
/// # Errors
///
/// Propagates store and transaction failures; a failed commit leaves the
/// previous generation untouched.
pub fn persist_registry(
    main: &StoreHandle,
    model: &RegistryModel,
    generation: Generation,
    digest: &str,
) -> Result<()> {
    let graph = registry_graph();
    let quads = registry_quads(model, generation, digest)?;
    main.with_store(|store| {
        let existing = store.get(QuadPattern::new().with_graph(graph.clone()))?;
        let mut transaction = store.transaction()?;
        if !existing.is_empty() {
            transaction.delete(&existing)?;
        }
        transaction.insert(&quads)?;
        transaction.commit()
    })
}

/// The registry graph name.
///
/// # Panics
///
/// Panics only if the constant IRI is invalid, which the crate tests cover.
#[must_use]
pub fn registry_graph() -> GraphName {
    GraphName::Iri(Id::new(REGISTRY_GRAPH_IRI).expect("the registry graph IRI is a valid IRI"))
}

/// Materialises the model as quads for `urn:onto:system:graphs`.
///
/// # Errors
///
/// Returns [`Error::InvalidIri`] when a vocabulary IRI is malformed.
pub fn registry_quads(
    model: &RegistryModel,
    generation: Generation,
    digest: &str,
) -> Result<Vec<Quad>> {
    let graph = registry_graph();
    let mut quads: Vec<Quad> = Vec::new();
    let registry = REGISTRY_GRAPH_IRI;

    fn add(quads: &mut Vec<Quad>, subject: &str, predicate: Id, object: Term, graph: &GraphName) {
        let subject = match subject.strip_prefix("_:") {
            Some(label) => Term::BlankNode(label.to_owned()),
            None => Term::Iri(Id::new(subject).expect("subjects are validated IRIs")),
        };
        quads.push(Quad::in_graph(subject, predicate, object, graph.clone()));
    }

    add(
        &mut quads,
        registry,
        Id::new(ontology_core::namespace::RDF_TYPE)?,
        Term::Iri(onto("GraphRegistry")),
        &graph,
    );
    add(
        &mut quads,
        registry,
        onto("registryVersion"),
        Term::Literal(Literal::integer(i64::from(model.version))),
        &graph,
    );
    add(
        &mut quads,
        registry,
        onto("generation"),
        Term::Literal(Literal::integer(
            i64::try_from(generation.0).unwrap_or(i64::MAX),
        )),
        &graph,
    );
    add(
        &mut quads,
        registry,
        Id::new(format!("{DCTERMS}modified"))?,
        Term::Literal(Literal::datetime(&Utc::now())),
        &graph,
    );
    add(
        &mut quads,
        registry,
        onto("sourceDigest"),
        Term::string(digest),
        &graph,
    );
    add(
        &mut quads,
        registry,
        Id::new(format!("{PROV}wasGeneratedBy"))?,
        Term::string("graph-registry-sync"),
        &graph,
    );
    add(
        &mut quads,
        registry,
        onto("mainStore"),
        Term::Iri(Id::new(model.main.as_str())?),
        &graph,
    );

    for (store_index, store) in model.stores.iter().enumerate() {
        let subject = store.id.as_str();
        add(
            &mut quads,
            subject,
            Id::new(ontology_core::namespace::RDF_TYPE)?,
            Term::Iri(onto("Store")),
            &graph,
        );
        add(
            &mut quads,
            registry,
            onto("entry"),
            Term::Iri(Id::new(subject)?),
            &graph,
        );
        add(
            &mut quads,
            subject,
            onto("role"),
            Term::string(store.role.as_str()),
            &graph,
        );
        add(
            &mut quads,
            subject,
            onto("storePath"),
            Term::string(store.declared_path.display().to_string()),
            &graph,
        );
        add(
            &mut quads,
            subject,
            onto("access"),
            Term::string(store.access.as_str()),
            &graph,
        );
        add(
            &mut quads,
            subject,
            onto("layout"),
            Term::string(store.layout.as_str()),
            &graph,
        );
        add(
            &mut quads,
            subject,
            onto("mountOrder"),
            Term::Literal(Literal::integer(store.mount_order)),
            &graph,
        );

        for (registration_index, registration) in store.registrations.iter().enumerate() {
            let node = format!("reg-{store_index}-{registration_index}");
            let blank = format!("_:{node}");
            add(
                &mut quads,
                subject,
                onto("registration"),
                Term::BlankNode(node.clone()),
                &graph,
            );
            add(
                &mut quads,
                &blank,
                Id::new(ontology_core::namespace::RDF_TYPE)?,
                Term::Iri(onto("GraphRegistration")),
                &graph,
            );
            add(
                &mut quads,
                &blank,
                onto("graphIri"),
                Term::Iri(registration.graph.clone()),
                &graph,
            );
            for namespace in &registration.namespaces {
                add(
                    &mut quads,
                    &blank,
                    onto("namespace"),
                    Term::string(namespace),
                    &graph,
                );
            }
            add(
                &mut quads,
                &blank,
                onto("priority"),
                Term::Literal(Literal::integer(i64::from(registration.priority))),
                &graph,
            );
            add(
                &mut quads,
                &blank,
                onto("enabled"),
                Term::Literal(Literal::boolean(registration.enabled)),
                &graph,
            );
            add(
                &mut quads,
                &blank,
                onto("required"),
                Term::Literal(Literal::boolean(registration.required)),
                &graph,
            );
            add(
                &mut quads,
                &blank,
                onto("strictBlankNodes"),
                Term::Literal(Literal::boolean(registration.strict_blank_nodes)),
                &graph,
            );
        }
    }

    Ok(quads)
}

fn onto(local: &str) -> Id {
    ontology_core::namespace::NamespaceRegistry::onto(local)
}
