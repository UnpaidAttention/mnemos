//! Mnemos core: storage, types, file I/O, retrieval.
//!
//! This crate is transport-agnostic. CLI, daemon, and UI all sit on top of it.

#![deny(rust_2018_idioms)]
#![warn(clippy::all)]

pub mod correction;
pub mod doctor;
pub mod embedder_rebuild;
pub mod error;
pub mod file_io;
pub mod frontmatter;
pub mod graph;
pub mod id;
pub mod paths;
pub mod pipeline;
pub mod providers;
pub mod rebuild;
pub mod retrieval;
pub mod storage;
pub mod sync;
pub mod tier;
pub mod types;
pub mod vault;
pub mod watcher;

// re-exports populated in later tasks
pub use error::{MnemosError, Result};
pub use storage::migrations::LATEST_SCHEMA_VERSION;
pub use storage::Storage; // re-enabled in Task 10
pub use tier::Tier;
pub use types::{Memory, MemoryType}; // re-enabled in Task 7
pub use vault::{BackfillStats, RememberOpts, Vault};
