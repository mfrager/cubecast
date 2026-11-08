//! Administrative operations over the graph registry and its stores.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::Utc;
use ontology_core::config::UpdatePolicy;
use ontology_core::error::{Error, Result};
use ontology_core::id::Id;
use ontology_rdf::OxigraphStore;
use serde::Serialize;

use crate::catalog::{CatalogSnapshot, GraphCatalogImpl, GraphStoreId, StoreAccess, StoreStatus};
use crate::registry;
use crate::sync::{self, GraphSyncReport};

/// Result of validating one store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreValidation {
    /// Store identifier.
    pub store: String,
    /// Whether the store passed.
    pub ok: bool,
    /// Failure detail.
    pub detail: Option<String>,
}

/// Result of backing up every store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphBackupReport {
    /// Generation the backup was taken at.
    pub generation: u64,
    /// Backup root.
    pub target: String,
    /// Per-store directories.
    pub stores: Vec<String>,
    /// Manifest path.
    pub manifest: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BackupManifest {
    generation: u64,
    created_at: String,
    engine: String,
    stores: Vec<BackupStore>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BackupStore {
    id: String,
    directory: String,
    quads: u64,
}

/// Write and maintenance operations.
pub trait GraphAdmin: Send + Sync {
    /// Parses the RDF/XML registry, diffs it, persists it into the main store,
    /// and publishes a new snapshot.
    ///
    /// # Errors
    ///
    /// Propagates parse, validation, open, and storage failures.
    fn sync_registry(&self) -> Result<GraphSyncReport>;

    /// Re-opens every store at the published generation.
    ///
    /// # Errors
    ///
    /// Propagates the same failures as [`GraphAdmin::sync_registry`].
    fn reload(&self) -> Result<GraphSyncReport>;

    /// Operator-facing status for every store.
    fn status(&self) -> Vec<StoreStatus>;

    /// Validates registry invariants and every store's indexes.
    ///
    /// # Errors
    ///
    /// Returns [`Error::GraphRegistry`] when the registry is invalid.
    fn validate(&self) -> Result<Vec<StoreValidation>>;

    /// Copies every store into `target` and writes a manifest.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Conflict`] when `target` exists and propagates backup
    /// failures.
    fn backup(&self, target: &Path) -> Result<GraphBackupReport>;

    /// Runs `operation` with a satellite reopened read-write.
    ///
    /// # Errors
    ///
    /// Returns [`Error::GraphReadOnly`] for a read-only store and propagates
    /// open, operation, and flush failures.
    fn with_writable_store(
        &self,
        id: &GraphStoreId,
        operation: &mut dyn FnMut(&OxigraphStore) -> Result<()>,
    ) -> Result<()>;
}

/// Default implementation over a shared catalog.
pub struct GraphAdminImpl {
    catalog: Arc<GraphCatalogImpl>,
    main: Arc<crate::catalog::StoreHandle>,
    registry_path: PathBuf,
    root: PathBuf,
    policy: UpdatePolicy,
}

impl GraphAdminImpl {
    /// Creates the administrator for one registry file.
    #[must_use]
    pub fn new(
        catalog: Arc<GraphCatalogImpl>,
        main: Arc<crate::catalog::StoreHandle>,
        registry_path: PathBuf,
        root: PathBuf,
        policy: UpdatePolicy,
    ) -> Self {
        Self {
            catalog,
            main,
            registry_path,
            root,
            policy,
        }
    }

    /// Current snapshot.
    #[must_use]
    pub fn snapshot(&self) -> Arc<CatalogSnapshot> {
        self.catalog.current()
    }

    /// The configured cross-store policy.
    #[must_use]
    pub fn policy(&self) -> UpdatePolicy {
        self.policy
    }

    fn publish_loaded(&self, persist: bool) -> Result<GraphSyncReport> {
        let loaded = sync::load_registry_file(&self.registry_path)?;
        loaded.model.validate(&self.root, &self.main.path)?;
        let previous = self.catalog.current();
        let generation = previous.generation.next();
        let (snapshot, degraded) = sync::build_snapshot(
            Arc::clone(&self.main),
            &loaded.model,
            &self.root,
            generation,
        )?;
        if persist {
            sync::persist_registry(&self.main, &loaded.model, generation, &loaded.digest)?;
        }
        let diff = registry::diff(previous.model.as_ref(), &loaded.model);
        let report = GraphSyncReport {
            generation: generation.0,
            added: diff.added,
            changed: diff.changed,
            removed: diff.removed,
            unchanged: diff.unchanged,
            degraded,
        };
        self.catalog.publish(Arc::new(snapshot));
        tracing::info!(
            generation = report.generation,
            added = report.added.len(),
            changed = report.changed.len(),
            removed = report.removed.len(),
            degraded = report.degraded.len(),
            "graph registry synchronised"
        );
        Ok(report)
    }

    /// Loads a registry snapshot without rewriting the main graph, which is how
    /// `sync = "manual"` starts from the persisted registry.
    ///
    /// # Errors
    ///
    /// Propagates open and validation failures.
    pub fn load_only(&self) -> Result<GraphSyncReport> {
        if self.registry_path.as_os_str().is_empty() {
            return Err(Error::GraphRegistry {
                detail: "no registry file is configured".to_owned(),
            });
        }
        self.publish_loaded(false)
    }

    /// Runs `operation` on a satellite reopened read-write.
    ///
    /// # Errors
    ///
    /// Returns [`Error::GraphReadOnly`] for a read-only store and propagates
    /// open, operation, and flush failures.
    pub fn write<T>(
        &self,
        id: &GraphStoreId,
        operation: impl FnOnce(&OxigraphStore) -> Result<T>,
    ) -> Result<T> {
        let snapshot = self.catalog.current();
        let handle = snapshot.store(id).ok_or_else(|| Error::GraphRegistry {
            detail: format!("store {id} is not registered"),
        })?;
        if handle.access != StoreAccess::ReadWrite {
            return Err(Error::GraphReadOnly {
                graph: handle.id.to_string(),
                store: handle.id.to_string(),
            });
        }
        handle.with_writable(operation)
    }
}

impl GraphAdmin for GraphAdminImpl {
    fn sync_registry(&self) -> Result<GraphSyncReport> {
        self.publish_loaded(true)
    }

    fn reload(&self) -> Result<GraphSyncReport> {
        self.publish_loaded(true)
    }

    fn status(&self) -> Vec<StoreStatus> {
        self.catalog.current().status()
    }

    fn validate(&self) -> Result<Vec<StoreValidation>> {
        let snapshot = self.catalog.current();
        if let Some(model) = &snapshot.model {
            model.validate(&self.root, &self.main.path)?;
        }
        let mut validations = Vec::new();
        for handle in snapshot.stores.values() {
            let outcome = handle.with_store(|store| store.validate());
            validations.push(StoreValidation {
                store: handle.id.to_string(),
                ok: outcome.is_ok(),
                detail: outcome.err().map(|error| error.to_string()),
            });
        }
        Ok(validations)
    }

    fn backup(&self, target: &Path) -> Result<GraphBackupReport> {
        if target.exists() {
            return Err(Error::Conflict {
                kind: "backup directory",
                id: target.display().to_string(),
            });
        }
        std::fs::create_dir_all(target)?;
        let snapshot = self.catalog.current();
        let mut stores = Vec::new();
        let mut manifest_stores = Vec::new();
        for (index, handle) in snapshot.stores.values().enumerate() {
            let local = Id::new(handle.id.as_str())
                .map(|id| id.local_name().to_owned())
                .unwrap_or_else(|_| format!("store-{index}"));
            let directory = format!("{index:02}-{local}");
            let path = target.join(&directory);
            handle.with_store(|store| store.backup(&path))?;
            stores.push(directory.clone());
            manifest_stores.push(BackupStore {
                id: handle.id.to_string(),
                directory,
                quads: handle.quad_count(),
            });
        }
        let manifest = BackupManifest {
            generation: snapshot.generation.0,
            created_at: Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            engine: crate::pool::ENGINE_REVISION.to_owned(),
            stores: manifest_stores,
        };
        let manifest_path = target.join("manifest.json");
        std::fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&manifest).map_err(Error::serialization)?,
        )?;
        Ok(GraphBackupReport {
            generation: snapshot.generation.0,
            target: target.display().to_string(),
            stores,
            manifest: manifest_path.display().to_string(),
        })
    }

    fn with_writable_store(
        &self,
        id: &GraphStoreId,
        operation: &mut dyn FnMut(&OxigraphStore) -> Result<()>,
    ) -> Result<()> {
        self.write(id, |store| operation(store))
    }
}
