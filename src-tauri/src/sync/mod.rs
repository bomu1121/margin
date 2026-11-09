//! Cloud sync: server-authoritative op log with local SQLite materialisation.
//!
//! Design and limitations: `docs/SYNC_DESIGN.md`.

pub mod engine;
pub mod hlc;
pub mod registry;
pub mod transport;

#[cfg(test)]
mod tests;

pub use engine::{SyncEngine, SyncReport, SyncState};
pub use transport::HttpTransport;
