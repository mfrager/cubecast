//! Store directory handling: path containment, marker files, and opening.

use std::path::{Component, Path, PathBuf};

use ontology_core::error::{Error, Result};
use ontology_core::id::Id;
use ontology_rdf::OxigraphStore;
use serde::{Deserialize, Serialize};

use crate::catalog::{GraphStoreId, StoreAccess, StoreLayout, StoreRole};

/// Marker written into every store directory.
pub const MARKER_FILE: &str = "graph.meta.json";

/// Engine identity recorded in markers.
pub const ENGINE_REVISION: &str = "oxigraph-0.6.0-dev+ae2a2ae";

/// Default main store identifier.
pub const MAIN_STORE_ID: &str = "urn:onto:store:main";

/// Contents of `graph.meta.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreMarker {
    /// Marker schema version.
    pub format_version: u32,
    /// Registry subject this directory belongs to.
    pub store_id: String,
    /// `main` or `satellite`.
    pub role: String,
    /// Logical graphs hosted here.
    pub graphs: Vec<String>,
    /// `single` or `named`.
    pub layout: String,
    /// Creation timestamp.
    pub created_at: String,
    /// Engine revision.
    pub engine: String,
}

impl StoreMarker {
    /// Creates a marker for `id`.
    #[must_use]
    pub fn new(id: &GraphStoreId, role: StoreRole, layout: StoreLayout, graphs: &[Id]) -> Self {
        Self {
            format_version: 1,
            store_id: id.to_string(),
            role: role.as_str().to_owned(),
            graphs: graphs.iter().map(ToString::to_string).collect(),
            layout: layout.as_str().to_owned(),
            created_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            engine: ENGINE_REVISION.to_owned(),
        }
    }
}

/// Resolves the absolute form of a path against the current directory.
#[must_use]
pub fn absolute_path(path: &Path) -> PathBuf {
    let base = std::env::current_dir().unwrap_or_default();
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    };
    normalise(&joined)
}

fn normalise(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Resolves a satellite path and checks that it stays inside `root`.
///
/// # Errors
///
/// Returns [`Error::GraphRegistry`] when the path escapes the root or cannot be
/// represented as an `oxigraph://` IRI.
pub fn resolve_store_path(root: &Path, declared: &Path) -> Result<PathBuf> {
    let root = absolute_path(root);
    let joined = if declared.is_absolute() {
        normalise(declared)
    } else {
        normalise(&root.join(declared))
    };
    if !joined.starts_with(&root) {
        return Err(Error::GraphRegistry {
            detail: format!(
                "store path {} escapes graphs.root {}",
                joined.display(),
                root.display()
            ),
        });
    }
    ensure_iri_safe(&joined)?;
    Ok(joined)
}

fn ensure_iri_safe(path: &Path) -> Result<()> {
    let Some(text) = path.to_str() else {
        return Err(Error::GraphRegistry {
            detail: format!(
                "store path {} is not valid UTF-8 and cannot be used as an oxigraph:// IRI",
                path.display()
            ),
        });
    };
    if text.is_empty()
        || text.chars().any(|value| {
            value.is_whitespace() || !value.is_ascii() || matches!(value, '#' | '?' | '%')
        })
    {
        return Err(Error::GraphRegistry {
            detail: format!(
                "store path {text:?} contains characters that cannot appear in an oxigraph:// IRI"
            ),
        });
    }
    Ok(())
}

/// Reads the marker of `directory`.
///
/// # Errors
///
/// Returns [`Error::GraphOpen`] when the marker is missing or invalid.
pub fn read_marker(directory: &Path) -> Result<StoreMarker> {
    let path = directory.join(MARKER_FILE);
    let data = std::fs::read(&path).map_err(|error| Error::GraphOpen {
        path: directory.to_path_buf(),
        detail: format!("missing {MARKER_FILE}: {error}"),
    })?;
    serde_json::from_slice(&data).map_err(|error| Error::GraphOpen {
        path: directory.to_path_buf(),
        detail: format!("invalid {MARKER_FILE}: {error}"),
    })
}

/// Writes a marker, replacing any previous one.
///
/// # Errors
///
/// Returns [`Error::Io`] or [`Error::Serialization`] on failure.
pub fn write_marker(directory: &Path, marker: &StoreMarker) -> Result<()> {
    let path = directory.join(MARKER_FILE);
    let data = serde_json::to_vec_pretty(marker).map_err(Error::serialization)?;
    std::fs::write(&path, data)?;
    Ok(())
}

/// Verifies that a marker matches the registry entry.
///
/// # Errors
///
/// Returns [`Error::GraphOpen`] when the directory belongs to another store or
/// layout, which protects against a restored or moved directory.
pub fn verify_marker(
    directory: &Path,
    id: &GraphStoreId,
    layout: StoreLayout,
    graphs: &[Id],
) -> Result<()> {
    let marker = read_marker(directory)?;
    if marker.store_id != id.as_str() {
        return Err(Error::GraphOpen {
            path: directory.to_path_buf(),
            detail: format!(
                "directory belongs to {} but the registry expects {}",
                marker.store_id, id
            ),
        });
    }
    if marker.layout != layout.as_str() {
        return Err(Error::GraphOpen {
            path: directory.to_path_buf(),
            detail: format!(
                "directory layout is {} but the registry expects {}",
                marker.layout,
                layout.as_str()
            ),
        });
    }
    if !graphs.is_empty() {
        let mut expected: Vec<&str> = graphs.iter().map(Id::as_str).collect();
        expected.sort_unstable();
        let mut recorded: Vec<&str> = marker.graphs.iter().map(String::as_str).collect();
        recorded.sort_unstable();
        if expected != recorded {
            return Err(Error::GraphOpen {
                path: directory.to_path_buf(),
                detail: format!(
                    "directory holds {:?} but the registry expects {:?}",
                    marker.graphs, marker.graphs
                ),
            });
        }
    }
    Ok(())
}

/// Opens (creating if needed) the main store directory.
///
/// # Errors
///
/// Propagates directory and engine failures.
pub fn open_main(
    path: &Path,
    id: GraphStoreId,
    max_open_files: Option<u32>,
) -> Result<std::sync::Arc<crate::catalog::StoreHandle>> {
    use std::sync::Arc;

    use crate::catalog::{StoreHandle, StoreHealth};

    let path = absolute_path(path);
    std::fs::create_dir_all(&path)?;
    if !path.join(MARKER_FILE).exists() {
        write_marker(
            &path,
            &StoreMarker::new(&id, StoreRole::Main, StoreLayout::Named, &[]),
        )?;
    }
    let store = match max_open_files {
        Some(limit) => OxigraphStore::open_with_max_open_files(&path, limit)?,
        None => OxigraphStore::open(&path)?,
    };
    Ok(Arc::new(StoreHandle::new(
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
        Some(Arc::new(store)),
    )))
}

/// Opens a satellite read-only after verifying its marker.
///
/// # Errors
///
/// Returns [`Error::GraphOpen`] for a missing directory or marker mismatch, and
/// propagates engine failures.
pub fn open_satellite(
    path: &Path,
    id: &GraphStoreId,
    layout: StoreLayout,
    graphs: &[Id],
) -> Result<OxigraphStore> {
    if !path.exists() {
        return Err(Error::GraphOpen {
            path: path.to_path_buf(),
            detail: format!("store {id} directory does not exist"),
        });
    }
    verify_marker(path, id, layout, graphs)?;
    OxigraphStore::open_read_only(path)
}

/// Creates an empty satellite store with its marker.
///
/// # Errors
///
/// Propagates directory and engine failures.
pub fn create_satellite(
    path: &Path,
    id: &GraphStoreId,
    layout: StoreLayout,
    graphs: &[Id],
) -> Result<()> {
    std::fs::create_dir_all(path)?;
    let store = OxigraphStore::open(path)?;
    store.flush()?;
    drop(store);
    write_marker(
        path,
        &StoreMarker::new(id, StoreRole::Satellite, layout, graphs),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_markers_and_rejects_mismatches() {
        let directory = tempfile::tempdir().expect("temp dir");
        let id = GraphStoreId::new("urn:onto:store:products").expect("id");
        let graph = Id::new("urn:onto:data:products").expect("graph");
        create_satellite(
            directory.path(),
            &id,
            StoreLayout::Named,
            std::slice::from_ref(&graph),
        )
        .expect("create");

        let marker = read_marker(directory.path()).expect("marker");
        assert_eq!(marker.store_id, id.as_str());
        assert_eq!(marker.layout, "named");

        verify_marker(directory.path(), &id, StoreLayout::Named, &[graph]).expect("matches");

        let other = GraphStoreId::new("urn:onto:store:other").expect("id");
        assert!(matches!(
            verify_marker(directory.path(), &other, StoreLayout::Named, &[]),
            Err(Error::GraphOpen { .. })
        ));
    }

    #[test]
    fn rejects_paths_outside_the_root() {
        let root = Path::new("./data/graphs");
        assert!(resolve_store_path(root, Path::new("../escape")).is_err());
        assert!(resolve_store_path(root, Path::new("products")).is_ok());
    }
}
