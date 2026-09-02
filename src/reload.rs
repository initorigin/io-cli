//! The configuration a turn is built from, re-read once per turn.
//!
//! **io-harness takes the instruction files as a snapshot and never takes
//! another.** `read_instructions` runs inside `Config::discover`
//! (`io-harness-0.74.0/src/config.rs:2680`), composes each named file into a
//! constraint, and stores the result in a private `Config.instructions` field.
//! There is no `Config::reload`, and `Config::with_profile` clones the field
//! rather than going back to disk — so a `Config` is exactly as old as the call
//! that made it.
//!
//! io-cli made that call twice, both before the first prompt: `src/main.rs:81`
//! for every arm, and `src/main.rs:188` after the wizard has written a file. A
//! repository whose `AGENTS.md` changed during a session therefore reached no
//! turn at all, and the only way to pick the change up was to leave the session
//! and start another one. 0.18.0 adds a command that writes those very files,
//! which turns a papered-over annoyance into a surface that would lie about its
//! own effect: the operator writes an instruction, io-cli says it wrote it, and
//! the next turn is composed from the text that was on disk when the session
//! started.
//!
//! # Why the last good configuration is held rather than the error propagated
//!
//! Re-reading once per turn means the session now depends, every turn, on a file
//! an operator may be halfway through editing. An editor that writes in two steps
//! — or a `/config` write that has cut a table header and not yet the body — puts
//! a file on disk that `Config::discover` refuses, and a refusal that propagated
//! would end the session over a state that exists for a fraction of a second and
//! that the operator is already fixing.
//!
//! So [`Configuration::refresh`] never surrenders what it holds. A discovery
//! that fails leaves the previous pair in place, the turn runs on the last
//! configuration that discovered cleanly, and the operator is handed
//! io-harness's own sentence
//! — which names the file and says what it objected to, and is more than anything
//! written here could say. When the file parses again the new configuration is
//! adopted with nothing further asked of anybody.
//!
//! # Why the error is reported once and not every turn
//!
//! The refresh happens at the top of every turn, and a turn is one thing an
//! operator typed. A file that stays broken for six prompts would otherwise
//! produce the same sentence six times, which is how a product teaches people to
//! stop reading its notices — the same reason
//! [`crate::settings::deprecated_max_steps`] speaks only to a file that actually
//! carries the key.
//!
//! "Once" is decided by comparing the error text against the last text this type
//! reported and nothing else. No clock is read and no turn is counted: N1 forbids
//! both outside the driver, and neither would be a better answer anyway. A
//! *different* refusal is a different thing to say, so it is said; and a success
//! clears the memory, so a file that breaks, is fixed, and breaks again reports
//! twice, because the second break is news.
//!
//! # Both halves, or neither
//!
//! The re-read goes through [`crate::configure::reload`], which returns the
//! `Config` and io-cli's own `CliSettings` as a pair for the reason its own doc
//! comment gives: refreshing one and forgetting the other leaves the theme, the
//! glyph set and every other `[app.io-cli]` answer as it was at session start,
//! reporting a value no turn is using. This type stores them as one field pair
//! and replaces them in one assignment, so there is no arrangement of calls that
//! updates half of it.

use std::path::PathBuf;

use io_harness::Config;

use crate::settings::CliSettings;

/// The configuration in force, and the last refusal the operator was told about.
///
/// Constructed from what the driver already discovered at startup rather than
/// discovering again — `src/main.rs` applies `--profile` to its `Config` before
/// anything reads it, and a second discovery here would silently drop that
/// overlay.
pub struct Configuration {
    /// The directory discovery resolves the project and local scopes against.
    root: PathBuf,
    /// The last configuration that discovered cleanly. Never replaced by a
    /// failure.
    config: Config,
    /// io-cli's own section of that same configuration, replaced with it.
    settings: Option<CliSettings>,
    /// The text of the last refusal handed to the caller, or `None` when the last
    /// refresh succeeded. Compared against, never displayed from.
    reported: Option<String>,
}

impl Configuration {
    /// Hold what the driver has already read.
    ///
    /// `settings` is the `Option<CliSettings>` half of
    /// [`crate::settings::stored`], which is `None` both when no file wrote the
    /// section and when it could not be read — the notice for the second case is
    /// the driver's to disclose, and is deliberately not re-derived here.
    pub fn new(root: PathBuf, config: Config, settings: Option<CliSettings>) -> Self {
        Self {
            root,
            config,
            settings,
            reported: None,
        }
    }

    /// The configuration this turn is built from.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// io-cli's own settings, from the same read as [`Configuration::config`].
    pub fn settings(&self) -> Option<&CliSettings> {
        self.settings.as_ref()
    }

    /// Re-read before a turn is built. Returns the sentence the operator has not
    /// been told yet, where there is one.
    ///
    /// `Some(text)` means the configuration on disk is unreadable, this turn will
    /// run on the last one that was readable, and the caller should show `text`.
    /// `None` means either that the read succeeded — in which case
    /// [`Configuration::config`] now answers from the file as it is now — or
    /// that the same refusal has already been shown and saying it again would
    /// teach the operator to stop looking.
    pub fn refresh(&mut self) -> Option<String> {
        match crate::configure::reload(&self.root) {
            Ok((config, settings)) => {
                self.config = config;
                self.settings = settings;
                // Cleared, not left: the next failure after a clean read is a new
                // fact even when its text matches one reported before the fix.
                self.reported = None;
                None
            }
            Err(refusal) => {
                if self.reported.as_deref() == Some(refusal.as_str()) {
                    return None;
                }
                self.reported = Some(refusal.clone());
                Some(refusal)
            }
        }
    }
}
