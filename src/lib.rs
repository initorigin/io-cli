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
//! such a turn decides.
//!
//! **That is now the only thing it decides.** Through 0.11.0 the contained entry
//! point was also the only one that took a caller's [`contract`], so a responder,
//! a plan gate, MCP servers, language servers, a browser and skills all arrived
//! with the fan-out or not at all. 0.11.0 moved the flat turn onto
//! `Session::turn_bounded_observed`, which takes a contract too, and 0.12.0
//! finished the separation: every turn can answer a question, and a plan is
//! proposed only where the operator typed `/plan on`. 0.17.0 took the last thing
//! off that switch — both arms are now `Session::turn_bounded_steered` and
//! `Session::turn_contained_bounded_steered`, so a contained turn can be steered
//! mid-flight exactly as an uncontained one can, and containment decides fan-out
//! and nothing else. See [`queue`] for the surface an operator says it through.
//!
//! [io-harness]: https://docs.rs/io-harness

pub mod app;
pub mod approval;
pub mod attach;
pub mod bridge;
pub mod cli;
pub mod clipboard;
pub mod commands;
pub mod compact;
pub mod complete;
pub mod composer;
pub mod configure;
pub mod context;
pub mod contract;
pub mod cost;
pub mod diff;
pub mod edit;
pub mod editor;
pub mod events;
pub mod exec;
pub mod failure;
pub mod fanout;
pub mod fleet;
pub mod fuzzy;
pub mod gates;
pub mod glyphs;
pub mod home;
pub mod import;
pub mod intent;
pub mod keys;
pub mod lock;
pub mod markdown;
pub mod memory;
pub mod page;
pub mod picker;
pub mod picture;
pub mod plan;
pub mod pluginview;
pub mod prices;
pub mod provider;
pub mod providers;
pub mod queue;
pub mod recall;
pub mod reload;
pub mod resume;
pub mod reviewer;
pub mod rewind;
pub mod servers;
pub mod sessions;
pub mod settings;
pub mod shell;
pub mod skills;
pub mod skillview;
pub mod splash;
pub mod stats;
pub mod status;
pub mod stdin;
pub mod term;
pub mod theme;
pub mod transcript;
pub mod triage;
pub mod verify;
pub mod wizard;
