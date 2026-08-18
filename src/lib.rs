//! io-cli — a terminal interface over [io-harness].
//!
//! This crate renders run events, edits a prompt, and reads a keyboard. It
//! contains no agent loop, no provider client, no tool implementation, no
//! sandbox, no policy engine and no session store; all of that is the harness,
//! and `tests/dependencies.rs` asserts it rather than trusting it.
//!
//! The library half exists so the integration tests can reach the renderer, the
//! composer and the picker. `io` itself is the binary in `src/main.rs`.
//!
//! [io-harness]: https://docs.rs/io-harness

pub mod app;
pub mod approval;
pub mod bridge;
pub mod cli;
pub mod clipboard;
pub mod commands;
pub mod composer;
pub mod diff;
pub mod events;
pub mod exec;
pub mod fuzzy;
pub mod glyphs;
pub mod keys;
pub mod picker;
pub mod provider;
pub mod rewind;
pub mod sessions;
pub mod settings;
pub mod splash;
pub mod status;
pub mod term;
pub mod theme;
pub mod transcript;
pub mod verify;
pub mod wizard;
