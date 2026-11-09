//! Configuration for storage, logging, namespaces, and limits.

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::document::{ExtractionMode, TextFormat};
use crate::error::{Error, Result};
use crate::id::Id;
use crate::namespace::NamespaceRegistry;

/// Where the RDF store lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageBackend {
    /// An in-memory store, discarded when the process exits.
    #[default]
    Memory,
    /// A persistent RocksDB database directory named by `path`.
    Persistent,
}

/// Storage settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Backing store.
    #[serde(default)]
    pub backend: StorageBackend,
    /// Database directory, required for [`StorageBackend::Persistent`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    /// Open the database without write access.
    #[serde(default)]
    pub read_only: bool,
    /// RocksDB `max_open_files`; `None` keeps the engine default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_open_files: Option<u32>,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            backend: StorageBackend::Memory,
            path: None,
            read_only: false,
            max_open_files: None,
        }
    }
}

impl StorageConfig {
    /// Returns the database directory, or an error when a persistent store has
    /// no path configured.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Config`] for an incomplete persistent configuration.
    pub fn database_path(&self) -> Result<Option<&Path>> {
        match self.backend {
            StorageBackend::Memory => Ok(None),
            StorageBackend::Persistent => {
                self.path.as_deref().map(Some).ok_or_else(|| Error::Config {
                    detail: "storage.backend = \"persistent\" requires storage.path".to_owned(),
                })
            }
        }
    }
}

/// Logging settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// `tracing` environment filter (e.g. `info,ontology_rdf=debug`).
    #[serde(default = "default_log_filter")]
    pub filter: String,
    /// Emit one JSON object per event.
    #[serde(default)]
    pub json: bool,
}

/// When the RDF/XML graph registry is synchronised into the main store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphsSync {
    /// Parse and persist the registry while the process starts.
    #[default]
    OnStartup,
    /// Use whatever the main store already holds; sync only on demand.
    Manual,
}

/// How an update that touches more than one store is handled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdatePolicy {
    /// Reject the update before writing anything.
    #[default]
    Strict,
    /// Execute one group per store, without cross-store rollback.
    Split,
}

fn default_graphs_root() -> PathBuf {
    PathBuf::from("./data/graphs")
}

/// Multi-graph backend settings.
///
/// When the section is absent or `enabled = false`, the process runs in
/// single-store mode and behaves exactly as before.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphsConfig {
    /// Turns the registry and satellite stores on.
    #[serde(default)]
    pub enabled: bool,
    /// RDF/XML root configuration; required when `enabled` is true.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registry: Option<PathBuf>,
    /// Parent directory for satellite stores.
    #[serde(default = "default_graphs_root")]
    pub root: PathBuf,
    /// Startup synchronisation behaviour.
    #[serde(default)]
    pub sync: GraphsSync,
    /// Cross-store update policy.
    #[serde(default)]
    pub update_policy: UpdatePolicy,
}

impl Default for GraphsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            registry: None,
            root: default_graphs_root(),
            sync: GraphsSync::OnStartup,
            update_policy: UpdatePolicy::Strict,
        }
    }
}

impl GraphsConfig {
    /// Returns whether the registry section is active.
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}

fn default_registry_artifact_dir() -> PathBuf {
    PathBuf::from("./data/registry")
}

fn default_max_source_bytes() -> u64 {
    50 * 1024 * 1024
}

fn default_max_sync_bytes() -> u64 {
    4 * 1024 * 1024
}

fn default_public_base_url() -> String {
    "http://127.0.0.1:8080".to_owned()
}

fn default_project_base_iri() -> String {
    "urn:onto:project:".to_owned()
}

/// Semantic registry settings.
///
/// The section is optional; defaults keep local ingestion available. Remote
/// access is always opt-in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryConfig {
    /// Directory for content-addressed source and distribution artifacts.
    #[serde(default = "default_registry_artifact_dir")]
    pub artifact_dir: PathBuf,
    /// Maximum local or remote source size.
    #[serde(default = "default_max_source_bytes")]
    pub max_source_bytes: u64,
    /// Maximum multipart upload size.
    #[serde(default = "default_max_source_bytes")]
    pub max_upload_bytes: u64,
    /// Sources above this size use the background job path.
    #[serde(default = "default_max_sync_bytes")]
    pub max_sync_bytes: u64,
    /// Allow HTTP(S) source retrieval.
    #[serde(default)]
    pub allow_remote: bool,
    /// Exact host names allowed for remote retrieval; `*` opts into all hosts.
    #[serde(default)]
    pub remote_hosts: Vec<String>,
    /// Require every import to resolve before publication.
    #[serde(default)]
    pub require_resolved_imports: bool,
    /// Base URL used to render human-facing download links.
    #[serde(default = "default_public_base_url")]
    pub public_base_url: String,
}

impl Default for RegistryConfig {
    fn default() -> Self {
        Self {
            artifact_dir: default_registry_artifact_dir(),
            max_source_bytes: default_max_source_bytes(),
            max_upload_bytes: default_max_source_bytes(),
            max_sync_bytes: default_max_sync_bytes(),
            allow_remote: false,
            remote_hosts: Vec::new(),
            require_resolved_imports: false,
            public_base_url: default_public_base_url(),
        }
    }
}

/// Project-management settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectConfig {
    /// Absolute IRI prefix for generated project and work identifiers.
    #[serde(default = "default_project_base_iri")]
    pub base_iri: String,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            base_iri: default_project_base_iri(),
        }
    }
}

fn default_extraction_dir() -> PathBuf {
    PathBuf::from("./data/extractions")
}

fn default_shim_path() -> PathBuf {
    PathBuf::from("./python")
}

fn default_extraction_backend() -> String {
    "pipeline".to_owned()
}

fn default_extraction_tier() -> String {
    "default".to_owned()
}

fn default_max_input_bytes() -> u64 {
    200 * 1024 * 1024
}

fn default_extraction_concurrency() -> usize {
    1
}

fn default_extraction_timeout() -> u64 {
    900
}

fn default_max_text_bytes() -> usize {
    5 * 1024 * 1024
}

/// Document extraction settings.
///
/// When the section is absent or `enabled = false`, the process carries no
/// extractor and document commands report the capability as unsupported.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtractionConfig {
    /// Turns the MinerU extractor on.
    #[serde(default)]
    pub enabled: bool,
    /// Extraction mode used when a request does not override it.
    #[serde(default)]
    pub mode: ExtractionMode,
    /// Text format used when a request does not override it.
    #[serde(default)]
    pub text_format: TextFormat,
    /// Root directory for content-addressed runs.
    #[serde(default = "default_extraction_dir")]
    pub output_dir: PathBuf,
    /// Directory containing the `onto_mineru` Python shim.
    #[serde(default = "default_shim_path")]
    pub shim_path: PathBuf,
    /// Virtual environment holding MinerU.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub venv: Option<PathBuf>,
    /// MinerU backend (`pipeline`, `vlm`, …).
    #[serde(default = "default_extraction_backend")]
    pub backend: String,
    /// MinerU model tier.
    #[serde(default = "default_extraction_tier")]
    pub tier: String,
    /// Local MinerU model directory; downloading is opt-in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_source: Option<PathBuf>,
    /// Largest accepted source file.
    #[serde(default = "default_max_input_bytes")]
    pub max_input_bytes: u64,
    /// Maximum concurrent MinerU parses.
    #[serde(default = "default_extraction_concurrency")]
    pub max_concurrency: usize,
    /// Soft timeout communicated to the shim.
    #[serde(default = "default_extraction_timeout")]
    pub timeout_seconds: u64,
    /// Keep raw MinerU intermediates after a successful run.
    #[serde(default)]
    pub keep_intermediates: bool,
    /// Cap on text returned inline by interfaces.
    #[serde(default = "default_max_text_bytes")]
    pub max_text_bytes: usize,
}

impl Default for ExtractionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: ExtractionMode::Both,
            text_format: TextFormat::Markdown,
            output_dir: default_extraction_dir(),
            shim_path: default_shim_path(),
            venv: None,
            backend: default_extraction_backend(),
            tier: default_extraction_tier(),
            model_source: None,
            max_input_bytes: default_max_input_bytes(),
            max_concurrency: default_extraction_concurrency(),
            timeout_seconds: default_extraction_timeout(),
            keep_intermediates: false,
            max_text_bytes: default_max_text_bytes(),
        }
    }
}

fn default_log_filter() -> String {
    "info".to_owned()
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            filter: default_log_filter(),
            json: false,
        }
    }
}

/// Operational limits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LimitsConfig {
    /// Upper bound on concurrent blocking storage tasks (used from Phase 4).
    #[serde(default = "default_blocking_tasks")]
    pub max_blocking_tasks: usize,
    /// Upper bound on a single page of results.
    #[serde(default = "default_page_size")]
    pub max_page_size: usize,
}

fn default_blocking_tasks() -> usize {
    8
}

fn default_page_size() -> usize {
    1_000
}

impl Default for LimitsConfig {
    fn default() -> Self {
        Self {
            max_blocking_tasks: default_blocking_tasks(),
            max_page_size: default_page_size(),
        }
    }
}

/// Complete application configuration.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    /// Storage settings.
    #[serde(default)]
    pub storage: StorageConfig,
    /// Logging settings.
    #[serde(default)]
    pub logging: LoggingConfig,
    /// Additional or overriding namespace prefixes.
    #[serde(default)]
    pub namespaces: BTreeMap<String, String>,
    /// Multi-graph registry settings.
    #[serde(default)]
    pub graphs: GraphsConfig,
    /// Semantic registry settings.
    #[serde(default)]
    pub registry: RegistryConfig,
    /// Project-management settings.
    #[serde(default)]
    pub project: ProjectConfig,
    /// Operational limits.
    #[serde(default)]
    pub limits: LimitsConfig,
    /// Document extraction settings.
    #[serde(default)]
    pub extraction: ExtractionConfig,
}

impl Config {
    /// Parses configuration from a TOML document.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Config`] when the document is not valid TOML for this
    /// schema.
    pub fn from_toml_str(document: &str) -> Result<Self> {
        toml::from_str(document).map_err(|error| Error::Config {
            detail: error.to_string(),
        })
    }

    /// Reads configuration from a TOML file.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Config`] when the file cannot be read or parsed.
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let document = std::fs::read_to_string(path).map_err(|error| Error::Config {
            detail: format!("could not read {}: {error}", path.display()),
        })?;
        Self::from_toml_str(&document)
    }

    /// Loads configuration from `path` when it exists, otherwise defaults, and
    /// then applies environment overrides.
    ///
    /// Environment overrides:
    ///
    /// - `ONTO_STORAGE_PATH`: use a persistent store at this directory.
    /// - `ONTO_LOG`: `tracing` filter.
    ///
    /// # Errors
    ///
    /// Propagates read/parse failures from [`Config::from_file`].
    pub fn load(path: Option<&Path>) -> Result<Self> {
        let mut config = match path {
            Some(path) if path.exists() => Self::from_file(path)?,
            _ => Self::default(),
        };
        config.apply_env();
        Ok(config)
    }

    /// Validates invariants that would otherwise fail later at runtime.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Config`] when a limit is zero or a persistent store has
    /// no path, and propagates namespace validation failures.
    pub fn validate(&self) -> Result<()> {
        self.storage.database_path()?;
        if self.limits.max_blocking_tasks == 0 {
            return Err(Error::Config {
                detail: "limits.max_blocking_tasks must be greater than zero".to_owned(),
            });
        }
        if self.limits.max_page_size == 0 {
            return Err(Error::Config {
                detail: "limits.max_page_size must be greater than zero".to_owned(),
            });
        }
        self.namespace_registry()?;
        self.graphs_config()?;
        self.registry_config()?;
        self.project_config()?;
        self.extraction_config()?;
        Ok(())
    }

    /// Validates registry paths, limits, and remote-source policy.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Config`] when a path escapes its root, a size is zero,
    /// or remote access is enabled without an allowlist.
    pub fn registry_config(&self) -> Result<()> {
        let registry = &self.registry;
        if registry.artifact_dir.as_os_str().is_empty()
            || registry
                .artifact_dir
                .components()
                .any(|component| component == Component::ParentDir)
        {
            return Err(Error::Config {
                detail: format!(
                    "registry.artifact_dir {} must be non-empty and must not contain `..`",
                    registry.artifact_dir.display()
                ),
            });
        }
        for (name, value) in [
            ("max_source_bytes", registry.max_source_bytes),
            ("max_upload_bytes", registry.max_upload_bytes),
            ("max_sync_bytes", registry.max_sync_bytes),
        ] {
            if value == 0 {
                return Err(Error::Config {
                    detail: format!("registry.{name} must be greater than zero"),
                });
            }
        }
        if registry.max_sync_bytes > registry.max_source_bytes {
            return Err(Error::Config {
                detail: "registry.max_sync_bytes must not exceed max_source_bytes".to_owned(),
            });
        }
        if registry.allow_remote && registry.remote_hosts.is_empty() {
            return Err(Error::Config {
                detail: "registry.allow_remote = true requires registry.remote_hosts".to_owned(),
            });
        }
        for host in &registry.remote_hosts {
            let host = host.trim();
            if host.is_empty() || (host != "*" && host.contains('*')) {
                return Err(Error::Config {
                    detail: format!("invalid registry.remote_hosts entry {host:?}"),
                });
            }
        }
        if !(registry.public_base_url.starts_with("http://")
            || registry.public_base_url.starts_with("https://"))
        {
            return Err(Error::Config {
                detail: "registry.public_base_url must be an HTTP(S) URL".to_owned(),
            });
        }
        Ok(())
    }

    /// Validates the project identifier prefix.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Config`] when the prefix is not an absolute IRI.
    pub fn project_config(&self) -> Result<()> {
        if Id::new(self.project.base_iri.as_str()).is_err() {
            return Err(Error::Config {
                detail: format!(
                    "project.base_iri {:?} must be an absolute IRI",
                    self.project.base_iri
                ),
            });
        }
        Ok(())
    }

    /// Validates the document extraction settings.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Config`] when extraction is enabled with an unusable
    /// shim, venv, or model directory, or with a zero limit.
    pub fn extraction_config(&self) -> Result<()> {
        let extraction = &self.extraction;
        if !extraction.enabled {
            return Ok(());
        }
        if !extraction.shim_path.exists() {
            return Err(Error::Config {
                detail: format!(
                    "extraction.shim_path {} does not exist",
                    extraction.shim_path.display()
                ),
            });
        }
        if let Some(venv) = &extraction.venv {
            if !venv.exists() {
                return Err(Error::Config {
                    detail: format!("extraction.venv {} does not exist", venv.display()),
                });
            }
        }
        if let Some(models) = &extraction.model_source {
            if !models.exists() {
                return Err(Error::Config {
                    detail: format!(
                        "extraction.model_source {} does not exist",
                        models.display()
                    ),
                });
            }
        }
        for (name, value) in [
            ("max_input_bytes", extraction.max_input_bytes),
            ("timeout_seconds", extraction.timeout_seconds),
            ("max_text_bytes", extraction.max_text_bytes as u64),
        ] {
            if value == 0 {
                return Err(Error::Config {
                    detail: format!("extraction.{name} must be greater than zero"),
                });
            }
        }
        if extraction.max_concurrency == 0 {
            return Err(Error::Config {
                detail: "extraction.max_concurrency must be greater than zero".to_owned(),
            });
        }
        Ok(())
    }

    /// Validates the multi-graph settings, which also depend on storage.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Config`] when the registry is enabled without a file,
    /// without persistent storage, or with a root directory that escapes the
    /// working tree.
    pub fn graphs_config(&self) -> Result<()> {
        if !self.graphs.enabled {
            return Ok(());
        }
        if self.graphs.registry.is_none() {
            return Err(Error::Config {
                detail: "graphs.enabled = true requires graphs.registry".to_owned(),
            });
        }
        if self.storage.backend != StorageBackend::Persistent {
            return Err(Error::Config {
                detail: "graphs.enabled = true requires storage.backend = \"persistent\""
                    .to_owned(),
            });
        }
        if self
            .graphs
            .root
            .components()
            .any(|component| component == Component::ParentDir)
        {
            return Err(Error::Config {
                detail: format!(
                    "graphs.root {} must not contain `..` components",
                    self.graphs.root.display()
                ),
            });
        }
        Ok(())
    }

    /// Applies `ONTO_*` environment overrides in place.
    pub fn apply_env(&mut self) {
        if let Ok(path) = std::env::var("ONTO_STORAGE_PATH") {
            if !path.trim().is_empty() {
                self.storage.backend = StorageBackend::Persistent;
                self.storage.path = Some(PathBuf::from(path));
            }
        }
        if let Ok(filter) = std::env::var("ONTO_LOG") {
            if !filter.trim().is_empty() {
                self.logging.filter = filter;
            }
        }
    }

    /// Builds the namespace registry from the built-ins plus configured prefixes.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Validation`] when a configured prefix or namespace is
    /// invalid.
    pub fn namespace_registry(&self) -> Result<NamespaceRegistry> {
        let mut registry = NamespaceRegistry::with_builtins();
        for (prefix, namespace) in &self.namespaces {
            registry.insert(prefix.clone(), namespace.clone())?;
        }
        Ok(registry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_full_document() {
        let config = Config::from_toml_str(
            r#"
[storage]
backend = "persistent"
path = "./data/ontology"
max_open_files = 512

[logging]
filter = "debug"
json = true

[namespaces]
acme = "https://acme.example/"

[limits]
max_blocking_tasks = 2
max_page_size = 50
"#,
        )
        .unwrap();

        assert_eq!(config.storage.backend, StorageBackend::Persistent);
        assert_eq!(
            config.storage.database_path().unwrap(),
            Some(Path::new("./data/ontology"))
        );
        assert_eq!(config.storage.max_open_files, Some(512));
        assert!(config.logging.json);
        assert_eq!(config.limits.max_page_size, 50);
        assert!(config.namespace_registry().unwrap().get("acme").is_some());
    }

    #[test]
    fn empty_document_uses_defaults() {
        let config = Config::from_toml_str("").unwrap();
        assert_eq!(config, Config::default());
        assert_eq!(config.storage.backend, StorageBackend::Memory);
        assert_eq!(config.logging.filter, "info");
    }

    #[test]
    fn missing_file_yields_defaults() {
        let config = Config::load(Some(Path::new("does-not-exist.toml"))).unwrap();
        assert_eq!(config.storage.backend, StorageBackend::Memory);
    }

    #[test]
    fn persistent_storage_requires_a_path() {
        let config = Config::from_toml_str("[storage]\nbackend = \"persistent\"\n").unwrap();
        assert!(matches!(
            config.storage.database_path(),
            Err(Error::Config { .. })
        ));
    }

    #[test]
    fn extraction_requires_an_installed_shim_when_enabled() {
        let config = Config::from_toml_str(
            "[extraction]\nenabled = true\nshim_path = \"/definitely/not/a/shim\"\n",
        )
        .unwrap();
        assert!(matches!(config.validate(), Err(Error::Config { .. })));
        assert!(Config::default().validate().is_ok());

        let config = Config::from_toml_str("[extraction]\ntimeout_seconds = 0\n").unwrap();
        assert!(
            config.validate().is_ok(),
            "disabled extraction is not validated"
        );
    }

    #[test]
    fn validate_rejects_zero_limits_and_bad_prefixes() {
        let config = Config::from_toml_str("[limits]\nmax_page_size = 0\n").unwrap();
        assert!(matches!(config.validate(), Err(Error::Config { .. })));

        let config =
            Config::from_toml_str("[namespaces]\n\"bad!prefix\" = \"https://example.org/\"\n")
                .unwrap();
        assert!(config.validate().is_err());
        assert!(Config::default().validate().is_ok());
    }
}
