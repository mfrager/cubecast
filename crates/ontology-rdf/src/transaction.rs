//! Atomic transactions over the Oxigraph store.

use ontology_core::error::Result;
use ontology_core::model::Quad;
use ontology_core::rdf::RdfTransaction;
use oxigraph::store::{Store, Transaction};

use crate::convert::quad_to_ox;
use crate::store::storage_error;

/// An Oxigraph transaction implementing the engine-independent trait.
///
/// Uncommitted changes are held in memory by the engine and are discarded when
/// the value is dropped, so failing part-way through a batch cannot leave a
/// half-applied mutation.
pub struct OxigraphTransaction<'a> {
    inner: Transaction<'a>,
}

impl<'a> OxigraphTransaction<'a> {
    /// Starts a transaction on `store`.
    ///
    /// # Errors
    ///
    /// Returns [`ontology_core::Error::Storage`] when the engine cannot start
    /// one (for example on a read-only store).
    pub fn new(store: &'a Store) -> Result<Self> {
        Ok(Self {
            inner: store.start_transaction().map_err(storage_error)?,
        })
    }
}

impl RdfTransaction for OxigraphTransaction<'_> {
    fn insert(&mut self, quads: &[Quad]) -> Result<()> {
        for quad in quads {
            self.inner.insert(quad_to_ox(quad)?);
        }
        Ok(())
    }

    fn delete(&mut self, quads: &[Quad]) -> Result<()> {
        for quad in quads {
            self.inner.remove(&quad_to_ox(quad)?);
        }
        Ok(())
    }

    fn commit(self: Box<Self>) -> Result<()> {
        self.inner.commit().map_err(storage_error)
    }
}
