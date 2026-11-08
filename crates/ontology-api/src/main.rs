//! `ontology-api` server binary.

use std::path::PathBuf;
use std::process::ExitCode;

use ontology_api::{serve, ServeOptions};
use ontology_core::config::Config;
use ontology_core::tracing::init_tracing;

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("onto-api: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let config_path = std::env::var("ONTO_CONFIG").ok().map(PathBuf::from);
    let config = Config::load(config_path.as_deref())?;
    init_tracing(&config.logging);
    serve(ServeOptions::from_env(config)).await?;
    Ok(())
}
