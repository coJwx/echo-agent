//! Shared application core for Echo desktop and future HTTP/WebSocket adapters.
//!
//! This crate intentionally does not depend on Tauri. Shells provide their own
//! transport-specific adapters and reuse these modules for session, provider,
//! and stream payload behavior.

pub mod chat;
pub mod error;
pub mod events;
pub mod provider;
pub mod state;
