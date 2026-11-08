//! Remote administration CLI for the ontology management system.

pub mod cli;
pub mod client;

/// CLI result type.
pub type CliResult<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub use cli::{Cli, CommandArgs, Connection};
pub use client::{Invocation, OutputFormat};
