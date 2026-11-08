//! The Oxigraph-backed store.

use std::io::{Read, Write};
use std::path::Path;

use ontology_core::config::StorageConfig;
use ontology_core::error::{Error, Result};
use ontology_core::model::{GraphName, Quad, QuadPattern, QueryResult};
use ontology_core::rdf::{RdfStore, RdfTransaction};
use oxigraph::io::{RdfParser, RdfSerializer};
use oxigraph::sparql::{QueryResults, SparqlEvaluator};
use oxigraph::store::{Store, StoreOptions};

use crate::convert::{
    graph_label_from_ox, graph_to_ox, pattern_to_ox, quad_from_ox, quad_to_ox, term_from_ox,
};
use crate::format::{to_oxigraph, RdfFormat};
use crate::transaction::OxigraphTransaction;

/// Number of quads buffered before a load flush.
const LOAD_BATCH_SIZE: usize = 10_000;

/// Maps an engine error into the domain error type.
pub(crate) fn storage_error(error: impl std::fmt::Display) -> Error {
    Error::Storage {
        detail: error.to_string(),
    }
}

/// An RDF store backed by the internal Oxigraph engine (RocksDB or memory).
///
/// The type owns the engine handle; the rest of the system only sees it through
/// [`RdfStore`] and the extra capabilities implemented here (loading,
/// serialization, bulk loading, backup, and maintenance).
pub struct OxigraphStore {
    store: Store,
}

impl OxigraphStore {
    /// Creates an in-memory store.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Storage`] if the engine cannot initialise.
    pub fn in_memory() -> Result<Self> {
        Ok(Self {
            store: Store::new().map_err(storage_error)?,
        })
    }

    /// Opens (creating if necessary) a persistent store at `path`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::StoreOpen`] when the directory cannot be created or
    /// opened (for example because another writer holds the lock).
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        ensure_parent(path)?;
        Self::wrap_open(path, Store::open(path))
    }

    /// Opens a persistent store with an explicit RocksDB `max_open_files`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::StoreOpen`] when the directory cannot be opened.
    pub fn open_with_max_open_files(path: impl AsRef<Path>, max_open_files: u32) -> Result<Self> {
        let path = path.as_ref();
        ensure_parent(path)?;
        let options = StoreOptions::default().with_max_open_files(max_open_files);
        Self::wrap_open(path, Store::open_with_options(path, options))
    }

    /// Opens an existing store without write access.
    ///
    /// # Errors
    ///
    /// Returns [`Error::StoreOpen`] when the directory is missing or unreadable.
    pub fn open_read_only(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        Self::wrap_open(path, Store::open_read_only(path))
    }

    /// Opens a store according to a [`StorageConfig`].
    ///
    /// `read_only` is ignored for in-memory stores, which have no backing files.
    ///
    /// # Errors
    ///
    /// Propagates open failures from the engine.
    pub fn open_with(config: &StorageConfig) -> Result<Self> {
        let Some(path) = config.database_path()? else {
            return Self::in_memory();
        };
        if config.read_only {
            return Self::open_read_only(path);
        }
        match config.max_open_files {
            Some(max_open_files) => Self::open_with_max_open_files(path, max_open_files),
            None => Self::open(path),
        }
    }

    fn wrap_open(
        path: &Path,
        result: std::result::Result<Store, oxigraph::store::StorageError>,
    ) -> Result<Self> {
        result
            .map(|store| Self { store })
            .map_err(|error| Error::StoreOpen {
                path: path.to_path_buf(),
                detail: error.to_string(),
            })
    }

    /// Exposes the engine handle for maintenance and benchmarks.
    #[must_use]
    pub fn inner(&self) -> &Store {
        &self.store
    }

    /// Number of stored quads.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Storage`] on engine failure.
    pub fn quad_count(&self) -> Result<usize> {
        self.store.len().map_err(storage_error)
    }

    /// Flushes pending writes to disk.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Storage`] on engine failure.
    pub fn flush(&self) -> Result<()> {
        self.store.flush().map_err(storage_error)
    }

    /// Runs the engine's storage compaction/optimisation pass.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Storage`] on engine failure.
    pub fn optimize(&self) -> Result<()> {
        self.store.optimize().map_err(storage_error)
    }

    /// Checks the storage indexes for corruption.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Storage`] when the engine reports a problem.
    pub fn validate(&self) -> Result<()> {
        self.store.validate().map_err(storage_error)
    }

    /// Creates a consistent backup at `target`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] when the directory cannot be created and
    /// [`Error::Storage`] when the engine fails.
    pub fn backup(&self, target: impl AsRef<Path>) -> Result<()> {
        let target = target.as_ref();
        ensure_parent(target)?;
        self.store.backup(target).map_err(storage_error)
    }

    /// Parses RDF from `reader` and inserts it, optionally forcing every quad
    /// into `graph`.
    ///
    /// Returns the number of parsed quads.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Parse`] for invalid input and [`Error::Storage`] when
    /// insertion fails.
    pub fn load_from_reader<R: Read>(
        &self,
        format: RdfFormat,
        reader: R,
        base: Option<&str>,
        graph: Option<&GraphName>,
    ) -> Result<u64> {
        let mut parser = RdfParser::from_format(to_oxigraph(format));
        if let Some(base) = base {
            parser = parser.with_base_iri(base).map_err(Error::parse)?;
        }
        let mut count = 0_u64;
        let mut batch: Vec<oxigraph::model::Quad> = Vec::with_capacity(LOAD_BATCH_SIZE);
        for item in parser.for_reader(reader) {
            let mut quad = quad_from_ox(&item.map_err(Error::parse)?)?;
            if let Some(graph) = graph {
                quad = quad.with_graph(graph.clone());
            }
            batch.push(quad_to_ox(&quad)?);
            count += 1;
            if batch.len() >= LOAD_BATCH_SIZE {
                self.store.extend(batch.drain(..)).map_err(storage_error)?;
            }
        }
        if !batch.is_empty() {
            self.store.extend(batch).map_err(storage_error)?;
        }
        Ok(count)
    }

    /// Parses RDF from `data` and inserts it, optionally forcing every quad into
    /// `graph`.
    ///
    /// Returns the number of parsed quads.
    ///
    /// # Errors
    ///
    /// Propagates the same errors as [`OxigraphStore::load_from_reader`].
    pub fn load_from_slice(
        &self,
        format: RdfFormat,
        data: &[u8],
        base: Option<&str>,
        graph: Option<&GraphName>,
    ) -> Result<u64> {
        self.load_from_reader(format, std::io::Cursor::new(data), base, graph)
    }

    /// Fast bulk load for an empty store.
    ///
    /// The engine's bulk loader bypasses the ordinary write path, so this
    /// requires a store that holds no quads; use [`OxigraphStore::load_from_slice`]
    /// for incremental ingestion.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Parse`] for invalid input and [`Error::Storage`] when
    /// the loader refuses the store state or fails.
    pub fn bulk_load_from_slice(&self, format: RdfFormat, data: &[u8]) -> Result<u64> {
        let mut loader = self.store.bulk_loader();
        loader
            .load_from_slice(RdfParser::from_format(to_oxigraph(format)), data)
            .map_err(Error::parse)?;
        loader.commit().map_err(storage_error)?;
        self.quad_count().map(|count| count as u64)
    }

    /// Serializes the whole dataset into `writer`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Serialization`] for writer failures and
    /// [`Error::Storage`] when the store cannot be read.
    pub fn dump_to_writer<W: Write>(&self, format: RdfFormat, writer: W) -> Result<()> {
        let mut serializer = RdfSerializer::from_format(to_oxigraph(format)).for_writer(writer);
        for quad in self.store.iter() {
            serializer
                .serialize_quad(&quad.map_err(storage_error)?)
                .map_err(Error::serialization)?;
        }
        let _ = serializer.finish().map_err(Error::serialization)?;
        Ok(())
    }

    /// Serializes the whole dataset into a string.
    ///
    /// # Errors
    ///
    /// Propagates serialization failures and non-UTF-8 output.
    pub fn dump_to_string(&self, format: RdfFormat) -> Result<String> {
        let mut buffer = Vec::new();
        self.dump_to_writer(format, &mut buffer)?;
        String::from_utf8(buffer).map_err(Error::serialization)
    }

    /// Serializes one graph into `writer`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Serialization`] for writer failures and
    /// [`Error::Storage`] when the store cannot be read.
    pub fn dump_graph_to_writer<W: Write>(
        &self,
        graph: &GraphName,
        format: RdfFormat,
        writer: W,
    ) -> Result<()> {
        let _ = self
            .store
            .dump_graph_to_writer(&graph_to_ox(graph)?, to_oxigraph(format), writer)
            .map_err(storage_error)?;
        Ok(())
    }

    /// Serializes one graph into a string.
    ///
    /// # Errors
    ///
    /// Propagates serialization failures and non-UTF-8 output.
    pub fn dump_graph_to_string(&self, graph: &GraphName, format: RdfFormat) -> Result<String> {
        let mut buffer = Vec::new();
        self.dump_graph_to_writer(graph, format, &mut buffer)?;
        String::from_utf8(buffer).map_err(Error::serialization)
    }

    /// Seeds the internal OWL schema into `system:schema`.
    ///
    /// # Errors
    ///
    /// Propagates loading failures.
    pub fn bootstrap_schema(&self) -> Result<u64> {
        crate::schema::bootstrap(self)
    }
}

impl RdfStore for OxigraphStore {
    fn get(&self, pattern: QuadPattern) -> Result<Vec<Quad>> {
        let (subject, predicate, object, graph) = pattern_to_ox(&pattern)?;
        self.store
            .quads_for_pattern(
                subject.as_ref(),
                predicate.as_ref(),
                object.as_ref(),
                graph.as_ref(),
            )
            .map(|item| {
                item.map_err(storage_error)
                    .and_then(|quad| quad_from_ox(&quad))
            })
            .collect()
    }

    fn insert(&self, quads: &[Quad]) -> Result<()> {
        let translated: Vec<oxigraph::model::Quad> =
            quads.iter().map(quad_to_ox).collect::<Result<_>>()?;
        self.store.extend(translated).map_err(storage_error)
    }

    fn delete(&self, quads: &[Quad]) -> Result<()> {
        for quad in quads {
            self.store
                .remove(&quad_to_ox(quad)?)
                .map_err(storage_error)?;
        }
        Ok(())
    }

    fn query(&self, sparql: &str) -> Result<QueryResult> {
        let results = SparqlEvaluator::new()
            .parse_query(sparql)
            .map_err(Error::query)?
            .on_store(&self.store)
            .execute()
            .map_err(Error::query)?;
        convert_query_results(results)
    }

    fn update(&self, sparql: &str) -> Result<()> {
        SparqlEvaluator::new()
            .parse_update(sparql)
            .map_err(Error::update)?
            .on_store(&self.store)
            .execute()
            .map_err(Error::update)
    }

    fn graphs(&self) -> Result<Vec<GraphName>> {
        self.store
            .named_graphs()
            .map(|item| {
                item.map_err(storage_error)
                    .and_then(|graph| graph_label_from_ox(&graph))
            })
            .collect()
    }

    fn transaction(&self) -> Result<Box<dyn RdfTransaction + '_>> {
        Ok(Box::new(OxigraphTransaction::new(&self.store)?))
    }

    fn load_from_slice(
        &self,
        format: RdfFormat,
        data: &[u8],
        base: Option<&str>,
        graph: Option<&GraphName>,
    ) -> Result<u64> {
        OxigraphStore::load_from_slice(self, format, data, base, graph)
    }

    fn serialize(&self, format: RdfFormat, graph: Option<&GraphName>) -> Result<String> {
        match graph {
            Some(graph) => self.dump_graph_to_string(graph, format),
            None => self.dump_to_string(format),
        }
    }

    fn len(&self) -> Result<usize> {
        self.quad_count()
    }
}

fn convert_query_results(results: QueryResults<'_>) -> Result<QueryResult> {
    match results {
        QueryResults::Solutions(solutions) => {
            let variables: Vec<String> = solutions
                .variables()
                .iter()
                .map(|variable| variable.as_str().to_owned())
                .collect();
            let mut rows = Vec::new();
            for solution in solutions {
                let solution = solution.map_err(Error::query)?;
                let mut row = Vec::with_capacity(variables.len());
                for variable in &variables {
                    row.push(
                        solution
                            .get(variable.as_str())
                            .map(term_from_ox)
                            .transpose()?,
                    );
                }
                rows.push(row);
            }
            Ok(QueryResult::Solutions { variables, rows })
        }
        QueryResults::Boolean(value) => Ok(QueryResult::Boolean(value)),
        QueryResults::Graph(triples) => {
            let mut quads = Vec::new();
            for triple in triples {
                let triple = triple.map_err(Error::query)?;
                quads.push(quad_from_ox(&oxigraph::model::Quad::new(
                    triple.subject,
                    triple.predicate,
                    triple.object,
                    oxigraph::model::GraphName::DefaultGraph,
                ))?);
            }
            Ok(QueryResult::Graph(quads))
        }
        #[allow(unreachable_patterns)]
        _ => Err(Error::query("unsupported SPARQL query result kind")),
    }
}

fn ensure_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    Ok(())
}

/// Convenience alias for a shared store handle.
pub type DynStore = std::sync::Arc<dyn RdfStore>;
