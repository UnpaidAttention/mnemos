//! Mnemos daemon: long-running HTTP + WebSocket + MCP server over the
//! `mnemos_core::Vault`. Re-exported here for integration tests.

#![deny(rust_2018_idioms)]
#![warn(clippy::all)]

pub mod auth;
pub mod config;
