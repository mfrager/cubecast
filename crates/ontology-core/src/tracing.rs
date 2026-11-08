//! Structured logging setup.

use tracing_subscriber::EnvFilter;

use crate::config::LoggingConfig;

/// Initialises the global `tracing` subscriber.
///
/// Calling this more than once is harmless: the first initialisation wins and
/// later calls are ignored, which keeps tests and embedded use simple.
pub fn init_tracing(config: &LoggingConfig) {
    let filter = EnvFilter::try_new(&config.filter).unwrap_or_else(|_| EnvFilter::new("info"));
    if config.json {
        let _ = tracing_subscriber::fmt()
            .json()
            .with_env_filter(filter)
            .try_init();
    } else {
        let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
    }
}
