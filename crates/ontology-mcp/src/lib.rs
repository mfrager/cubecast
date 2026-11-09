//! Compact Model Context Protocol interface over the shared application
//! service.
//!
//! The server exposes six tools (`rdf`, `ontology`, `semantic`, `jobs`,
//! `registry`, and `project`). Every action is translated into an
//! [`ontology_core::Command`], so MCP adds no domain logic and behaves
//! identically to HTTP and the CLI.

pub mod backend;
pub mod dispatch;
pub mod params;
pub mod policy;
pub mod server;

pub use backend::HttpService;
pub use dispatch::Resolver;
pub use params::{
    JobsArgs, OntologyArgs, ProjectAction, ProjectArgs, RdfArgs, RegistryAction, RegistryArgs,
    SemanticArgs,
};
pub use policy::{AccessMode, Policy};
pub use server::McpServer;
