//! Asynchronous HTTP interface over the shared application service.
//!
//! Every endpoint translates a request into an `ontology_core::Command` and
//! renders the resulting `CommandResult`, so the HTTP layer holds no domain
//! logic and matches the CLI and future MCP interface.

pub mod error;
pub mod openapi;
pub mod router;
pub mod server;
pub mod state;

pub use error::ApiError;
pub use router::router;
pub use server::{serve, ServeOptions};
pub use state::AppState;
