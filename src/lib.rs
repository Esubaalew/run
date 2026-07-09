pub mod aliases;
pub mod app;
pub mod cache;
pub mod cli;
pub mod config;
pub mod detect;
pub mod engine;
pub mod highlight;
pub mod language;
pub mod output;
pub mod repl;
pub mod runtime;
pub mod templates;
pub mod version;

#[cfg(feature = "v2")]
pub mod v2;
