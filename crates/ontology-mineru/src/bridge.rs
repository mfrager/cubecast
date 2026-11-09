//! The PyO3 boundary: interpreter initialisation, `sys.path` setup, and one
//! shim call.

use std::path::{Path, PathBuf};

use ontology_core::error::{Error, Result};
use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::manifest::Manifest;

/// Options translated into the shim's `options` dictionary.
pub(crate) struct BridgeOptions {
    pub output_dir: PathBuf,
    pub shim_path: PathBuf,
    pub venv: Option<PathBuf>,
    pub backend: String,
    pub tier: String,
    pub model_source: Option<PathBuf>,
    pub timeout_seconds: u64,
}

/// Runs the shim once and returns its manifest.
pub(crate) fn run(source: &Path, options: &BridgeOptions) -> Result<Manifest> {
    Python::initialize();
    Python::attach(|py| {
        let sys = py.import("sys").map_err(python_error)?;
        let path = sys.getattr("path").map_err(python_error)?;
        path.call_method1(
            "insert",
            (0, options.shim_path.to_string_lossy().to_string()),
        )
        .map_err(python_error)?;
        if let Some(venv) = &options.venv {
            add_venv(py, venv)?;
        }

        let module = py.import("onto_mineru.bridge").map_err(|error| {
            Error::python(format!(
                "could not import onto_mineru.bridge from {}: {error}",
                options.shim_path.display()
            ))
        })?;
        let dict = PyDict::new(py);
        dict.set_item(
            "output_dir",
            options.output_dir.to_string_lossy().to_string(),
        )
        .map_err(python_error)?;
        dict.set_item("backend", &options.backend)
            .map_err(python_error)?;
        dict.set_item("tier", &options.tier).map_err(python_error)?;
        dict.set_item("timeout_seconds", options.timeout_seconds)
            .map_err(python_error)?;
        if let Some(source) = &options.model_source {
            dict.set_item("model_source", source.to_string_lossy().to_string())
                .map_err(python_error)?;
        }

        let result = module
            .getattr("extract")
            .and_then(|function| function.call1((source.to_string_lossy().to_string(), dict)))
            .map_err(|error| Error::extraction("mineru", truncate(&format!("{error}"))))?;
        let json: String = py
            .import("json")
            .and_then(|json| json.call_method1("dumps", (result,)))
            .and_then(|encoded| encoded.extract())
            .map_err(python_error)?;
        serde_json::from_str(&json).map_err(|error| {
            Error::extraction("manifest", format!("invalid shim manifest: {error}"))
        })
    })
}

fn add_venv(py: Python<'_>, venv: &Path) -> Result<()> {
    let site = py.import("site").map_err(python_error)?;
    let mut directories = Vec::new();
    for root in [venv.join("lib"), venv.join("Lib")] {
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        for entry in entries.filter_map(|entry| entry.ok()) {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let site_packages = path.join("site-packages");
            if site_packages.is_dir() {
                directories.push(site_packages);
            }
        }
    }
    if directories.is_empty() {
        return Err(Error::python(format!(
            "no site-packages directory found under {}",
            venv.display()
        )));
    }
    for directory in directories {
        site.call_method1("addsitedir", (directory.to_string_lossy().to_string(),))
            .map_err(python_error)?;
    }
    Ok(())
}

fn python_error(error: PyErr) -> Error {
    Error::python(truncate(&format!("{error}")))
}

fn truncate(message: &str) -> String {
    const LIMIT: usize = 4_000;
    if message.len() <= LIMIT {
        return message.to_owned();
    }
    let mut end = LIMIT;
    while !message.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &message[..end])
}
