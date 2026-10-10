//! Verbatim — the engine.
//!
//! A research agent whose every claim must be backed by a verbatim quote from a
//! source it actually fetched, verified by a deterministic checker that is not
//! the model that wrote the claim.
//!
//! Rule: if we didn't fetch it, we didn't say it.

pub mod agent;
pub mod config;
pub mod llm;
pub mod model;
pub mod retrieval;
pub mod store;
pub mod text;
pub mod trace;
pub mod verify;

pub use config::Config;
pub use llm::Llm;

/// Crate version, surfaced by the CLI and the health endpoint.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
