//! Capability traits for RDF storage and shared application execution.

use crate::error::{Error, Result};
use crate::format::RdfFormat;
use crate::id::Id;
use crate::model::{GraphName, Instance, Quad, QuadPattern, QueryResult, Statement, Term};

/// Loads an instance from one graph, defaulting to `data:asserted`.
///
/// # Errors
///
/// Propagates storage failures.
pub fn load_instance(store: &dyn RdfStore, id: &Id, graph: Option<&GraphName>) -> Result<Instance> {
    let graph = graph
        .cloned()
        .unwrap_or_else(|| GraphName::Iri(crate::graph::GraphKind::Asserted.iri()));
    let quads = store.get(
        QuadPattern::new()
            .with_subject(Term::Iri(id.clone()))
            .with_graph(graph.clone()),
    )?;
    let rdf_type = Id::new(format!("{}type", crate::namespace::RDF))?;
    let types = quads
        .iter()
        .filter(|quad| quad.predicate == rdf_type)
        .filter_map(|quad| quad.object.as_iri().cloned())
        .collect();
    let statements = quads.into_iter().map(Statement::new).collect();
    Ok(Instance {
        id: id.clone(),
        graph,
        types,
        statements,
    })
}

/// Read/write access to an RDF dataset.
///
/// The trait is synchronous because the underlying engine (Oxigraph over
/// RocksDB) is synchronous. Async interfaces will call it inside blocking
/// workers rather than putting storage work on the async executor.
pub trait RdfStore: Send + Sync {
    /// Returns every quad matching `pattern`.
    fn get(&self, pattern: QuadPattern) -> Result<Vec<Quad>>;

    /// Inserts quads. Existing quads are kept (insertion is idempotent).
    fn insert(&self, quads: &[Quad]) -> Result<()>;

    /// Deletes quads. Missing quads are ignored.
    fn delete(&self, quads: &[Quad]) -> Result<()>;

    /// Evaluates a SPARQL query.
    ///
    /// Queries see the store as an RDF dataset: the default graph is the
    /// unnamed graph, and managed data in `data:asserted` (or any other named
    /// graph) is queried through `GRAPH <...> { ... }`.
    fn query(&self, sparql: &str) -> Result<QueryResult>;

    /// Evaluates a SPARQL update.
    fn update(&self, sparql: &str) -> Result<()>;

    /// Lists named graphs that hold at least one quad.
    fn graphs(&self) -> Result<Vec<GraphName>>;

    /// Parses RDF data and inserts it, optionally forcing every quad into
    /// `graph`. Returns the number of parsed quads.
    ///
    /// The default implementation reports the capability as unsupported so a
    /// read-only or custom backend does not need a parser.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Unsupported`] by default.
    fn load_from_slice(
        &self,
        _format: RdfFormat,
        _data: &[u8],
        _base: Option<&str>,
        _graph: Option<&GraphName>,
    ) -> Result<u64> {
        Err(Error::Unsupported {
            operation: "load_from_slice".to_owned(),
        })
    }

    /// Serializes the dataset or one graph into an RDF document.
    ///
    /// The default implementation reports the capability as unsupported so a
    /// read-only or custom backend is not required to implement serialization.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Unsupported`] by default and propagates serializer
    /// failures otherwise.
    fn serialize(&self, _format: RdfFormat, _graph: Option<&GraphName>) -> Result<String> {
        Err(Error::Unsupported {
            operation: "RDF serialization".to_owned(),
        })
    }

    /// Starts an atomic transaction when the backend supports one.
    ///
    /// The default implementation reports the capability as unsupported so a
    /// new backend can opt in without implementing it.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Unsupported`] by default.
    fn transaction(&self) -> Result<Box<dyn RdfTransaction + '_>> {
        Err(Error::Unsupported {
            operation: "transaction".to_owned(),
        })
    }

    /// Returns whether the store contains `quad`.
    ///
    /// # Errors
    ///
    /// Propagates storage failures from [`RdfStore::get`].
    fn contains(&self, quad: &Quad) -> Result<bool> {
        Ok(!self.get(QuadPattern::new().with_quad(quad))?.is_empty())
    }

    /// Returns the total number of quads.
    ///
    /// # Errors
    ///
    /// Propagates storage failures from [`RdfStore::get`].
    fn len(&self) -> Result<usize> {
        Ok(self.get(QuadPattern::new())?.len())
    }

    /// Returns whether the store holds no quads.
    ///
    /// # Errors
    ///
    /// Propagates storage failures from [`RdfStore::len`].
    fn is_empty(&self) -> Result<bool> {
        Ok(self.len()? == 0)
    }
}

/// An atomic batch of storage changes.
///
/// Changes stay invisible until [`RdfTransaction::commit`]; dropping the
/// transaction rolls them back.
pub trait RdfTransaction {
    /// Adds quads to the transaction.
    fn insert(&mut self, quads: &[Quad]) -> Result<()>;

    /// Removes quads in the transaction.
    fn delete(&mut self, quads: &[Quad]) -> Result<()>;

    /// Commits the transaction atomically.
    ///
    /// Takes `self: Box<Self>` so transactions remain usable through
    /// `dyn RdfTransaction`, which is how the store hands them out.
    fn commit(self: Box<Self>) -> Result<()>;
}

/// The single execution boundary shared by HTTP, CLI, and MCP.
#[async_trait::async_trait]
pub trait OntologyService: Send + Sync {
    /// Executes one application command.
    ///
    /// # Errors
    ///
    /// Returns a domain [`Error`] when the command is invalid or fails.
    async fn execute(
        &self,
        command: crate::command::Command,
    ) -> Result<crate::command::CommandResult>;
}
