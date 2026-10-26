#![forbid(unsafe_code)]
//! `tunnel-core` — the portable tunnel domain, host-free.
//!
//! Pure data + transitions + argv planning. Process supervision, TCP probing,
//! and file persistence arrive as injected traits from the composition root
//! (Tauri shell, CLI, or tests), so this crate compiles in seconds and never
//! names a WebView, tokio, or HTTP server. It mirrors `packages/ssh-core` +
//! `packages/ssh-contract`; the JSON wire shapes are identical, and
//! `fixtures/` adjudicates both implementations.

pub mod argv;
pub mod contract;
pub mod error;
pub mod host;
pub mod manager;
pub mod ports;
pub mod state_machine;
pub mod validate;
