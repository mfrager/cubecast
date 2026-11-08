//! Multi-graph backend: a main graph directory plus registered satellites.
//!
//! The crate owns three pieces:
//!
//! - the **registry** (`registry`, `sync`) — an RDF/XML root configuration that
//!   is validated and materialised into `urn:onto:system:graphs`;
//! - the **catalog** (`catalog`) — immutable snapshots that map graphs and
//!   namespaces to stores;
//! - the **router** (`router`, `rewrite`) — a drop-in [`ontology_core::rdf::RdfStore`]
//!   that evaluates each query on one host and reaches satellites through
//!   in-process `SERVICE <oxigraph://…>`.
//!
//! [`GraphBackend`] is the only type an interface needs: it builds the right
//! store for a configuration and exposes the catalog and administration APIs.

pub mod admin;
pub mod catalog;
pub mod pool;
pub mod registry;
pub mod rewrite;
pub mod router;
pub mod sync;

#[cfg(test)]
mod test_support;

use std::path::PathBuf;
use std::sync::Arc;

use ontology_core::config::{Config, GraphsSync, StorageConfig, UpdatePolicy};
use ontology_core::error::{Error, Result};
use ontology_core::rdf::RdfStore;
use ontology_rdf::OxigraphStore;

pub use admin::{GraphAdmin, GraphAdminImpl, GraphBackupReport, StoreValidation};
pub use catalog::{
    CatalogSnapshot, Generation, GraphCatalog, GraphCatalogImpl, GraphRoute, GraphStoreId,
    StoreAccess, StoreHandle, StoreHealth, StoreLayout, StoreRole, StoreStatus,
};
pub use pool::{StoreMarker, MARKER_FILE};
pub use registry::{Registration, RegistryModel, StoreSpec};
pub use rewrite::{FallbackReason, QueryPlan};
pub use router::MultiStore;
pub use sync::GraphSyncReport;

/// Graph that holds the materialised registry.
pub const REGISTRY_GRAPH_IRI: &str = "urn:onto:system:graphs";

/// Facade over the catalog, router, and administrator.
pub struct GraphBackend {
    catalog: Arc<GraphCatalogImpl>,
    admin: Arc<GraphAdminImpl>,
    store: Arc<MultiStore>,
    plain: Option<Arc<OxigraphStore>>,
}

impl GraphBackend {
    /// Single-store compatibility mode: one directory, no registry.
    ///
    /// # Errors
    ///
    /// Propagates storage and schema bootstrap failures.
    pub fn single(config: &StorageConfig) -> Result<Self> {
        let store = Arc::new(OxigraphStore::open_with(config)?);
        store.bootstrap_schema()?;
        let path = config
            .database_path()?
            .map(pool::absolute_path)
            .unwrap_or_else(|| PathBuf::from("<memory>"));
        let id = GraphStoreId::new(pool::MAIN_STORE_ID)?;
        let handle = Arc::new(StoreHandle::new(
            id,
            path.clone(),
            path,
            StoreRole::Main,
            StoreAccess::ReadWrite,
            StoreLayout::Named,
            0,
            Vec::new(),
            Vec::new(),
            StoreHealth::Ready,
            Some(Arc::clone(&store)),
        ));
        let catalog = Arc::new(GraphCatalogImpl::new(Arc::new(CatalogSnapshot::single(
            handle,
            Generation(0),
        ))));
        let mut backend = Self::assemble(
            catalog,
            UpdatePolicy::Strict,
            PathBuf::new(),
            PathBuf::new(),
        )?;
        backend.plain = Some(store);
        Ok(backend)
    }

    /// Registry mode: opens the main store, synchronises the RDF/XML registry,
    /// and opens every satellite.
    ///
    /// # Errors
    ///
    /// Propagates configuration, parse, validation, open, and storage failures.
    pub fn from_config(config: &Config) -> Result<Self> {
        config.validate()?;
        if !config.graphs.enabled {
            return Self::single(&config.storage);
        }
        let storage_path = config
            .storage
            .database_path()?
            .ok_or_else(|| Error::Config {
                detail: "graphs.enabled requires a persistent storage.path".to_owned(),
            })?;
        let registry_path = config
            .graphs
            .registry
            .clone()
            .ok_or_else(|| Error::Config {
                detail: "graphs.enabled requires graphs.registry".to_owned(),
            })?;

        let loaded = sync::load_registry_file(&registry_path)?;
        loaded.model.validate(&config.graphs.root, storage_path)?;
        let main = pool::open_main(
            storage_path,
            loaded.model.main.clone(),
            config.storage.max_open_files,
        )?;
        main.with_store(OxigraphStore::bootstrap_schema)?;

        let catalog = Arc::new(GraphCatalogImpl::new(Arc::new(CatalogSnapshot::single(
            Arc::clone(&main),
            Generation(0),
        ))));
        let backend = Self::assemble(
            Arc::clone(&catalog),
            config.graphs.update_policy,
            registry_path,
            config.graphs.root.clone(),
        )?;

        match config.graphs.sync {
            GraphsSync::OnStartup => {
                backend.admin.sync_registry()?;
            }
            GraphsSync::Manual => match sync::load_persisted_registry(&main)? {
                Some(model) => {
                    model.validate(&config.graphs.root, storage_path)?;
                    let (snapshot, _) =
                        sync::build_snapshot(main, &model, &config.graphs.root, Generation(0))?;
                    catalog.publish(Arc::new(snapshot));
                }
                None => {
                    return Err(Error::GraphRegistry {
                        detail: "graphs.sync = \"manual\" requires a previously synced registry"
                            .to_owned(),
                    })
                }
            },
        }
        Ok(backend)
    }

    fn assemble(
        catalog: Arc<GraphCatalogImpl>,
        policy: UpdatePolicy,
        registry_path: PathBuf,
        root: PathBuf,
    ) -> Result<Self> {
        let main = Arc::clone(&catalog.current().main);
        let admin = Arc::new(GraphAdminImpl::new(
            Arc::clone(&catalog),
            main,
            registry_path,
            root,
            policy,
        ));
        let store = Arc::new(MultiStore::new(Arc::clone(&catalog), policy));
        Ok(Self {
            catalog,
            admin,
            store,
            plain: None,
        })
    }

    /// The drop-in store for the application service constructor.
    #[must_use]
    pub fn rdf_store(&self) -> Arc<dyn RdfStore> {
        match &self.plain {
            Some(store) => Arc::clone(store) as Arc<dyn RdfStore>,
            None => self.store.clone(),
        }
    }

    /// Administrative access.
    #[must_use]
    pub fn admin(&self) -> Arc<dyn GraphAdmin> {
        Arc::clone(&self.admin) as Arc<dyn GraphAdmin>
    }

    /// The concrete administrator, for calls that need its generic helpers.
    #[must_use]
    pub fn administrator(&self) -> &Arc<GraphAdminImpl> {
        &self.admin
    }

    /// Read-side discovery.
    #[must_use]
    pub fn catalog(&self) -> Arc<dyn GraphCatalog> {
        Arc::clone(&self.catalog) as Arc<dyn GraphCatalog>
    }

    /// The router itself.
    #[must_use]
    pub fn store(&self) -> &Arc<MultiStore> {
        &self.store
    }

    /// Returns the main store handle.
    #[must_use]
    pub fn main(&self) -> Arc<StoreHandle> {
        Arc::clone(&self.catalog.current().main)
    }
}
