//! Application services: instance management, statement CRUD, provenance, and
//! the shared execution boundary used by HTTP, CLI, and MCP.
//!
//! The crate depends only on `ontology-core` and therefore never on a transport
//! or on Oxigraph itself.

pub mod instance;
pub mod jobs;
pub mod service;

pub use instance::InstanceService;
pub use jobs::{JobContext, JobManager};
pub use service::ApplicationService;
