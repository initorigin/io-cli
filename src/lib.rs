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
//! Since 0.8.0 a session that configures `[app.io-cli.containment]` runs its
//! turns through io-harness's contained entry point, which is the only one that
//! reaches its spawn loop — see [`fleet`] for what that stream looks like and
//! what the interface makes of it, and [`settings::contained_notice`] for what
//! such a turn gives up.
//!
//! [io-harness]: https://docs.rs/io-harness

pub mod app;
pub mod approval;
pub mod attach;
pub mod bridge;
pub mod cli;
pub mod clipboard;
pub mod commands;
pub mod complete;
pub mod composer;
pub mod diff;
pub mod events;
pub mod exec;
pub mod fleet;
pub mod fuzzy;
pub mod glyphs;
pub mod keys;
pub mod picker;
pub mod picture;
pub mod provider;
pub mod rewind;
pub mod sessions;
pub mod settings;
pub mod shell;
pub mod splash;
pub mod status;
pub mod term;
pub mod theme;
pub mod transcript;
pub mod verify;
pub mod wizard;
