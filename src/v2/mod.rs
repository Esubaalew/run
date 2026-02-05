//! Run 2.0

pub mod bridge;
pub mod build;
pub mod cli;
pub mod component;
pub mod config;
pub mod deploy;
pub mod dev;
pub mod error;
pub mod orchestrator;
pub mod plugins;
pub mod registry;
pub mod runtime;
pub mod test;
pub mod toolchain;
pub mod wit;

pub use component::Component;
pub use config::RunConfig;
pub use error::{Error, Result};
pub use orchestrator::Orchestrator;
pub use runtime::{RuntimeConfig, RuntimeEngine};

pub const VERSION: &str = "2.0.0-alpha";

pub const WASI_VERSION: &str = "0.2";

pub fn is_available() -> bool {
    true
}

pub fn version_info() -> String {
    format!("Run {} (WASI {})", VERSION, WASI_VERSION)
}
