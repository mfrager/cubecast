//! Configuration for storage, logging, namespaces, and limits.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
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
    /// Operational limits.
    #[serde(default)]
    pub limits: LimitsConfig,
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
