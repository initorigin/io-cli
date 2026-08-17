//! io-cli's own settings, and writing io-harness's configuration file.
//!
//! **There is no configuration parser in this repository.** io-harness owns
//! discovery, layering and validation; this module hands it a `ProviderSpec` and
//! a `Defaults` — types the harness declares and already derives `Serialize` for —
//! and serializes them. Reading comes back through `Config::discover` and
//! `Config::app`, which is the section the harness deliberately does not validate
//! because it belongs to whoever is building on top of it.

use std::io;
use std::path::{Path, PathBuf};

use io_harness::{Defaults, Effect, ProviderSpec};
use serde::{Deserialize, Serialize};

/// The key io-cli's own section sits under: `[app.io-cli]`.
pub const APP_KEY: &str = "io-cli";

/// Everything io-cli itself remembers. One field in this release.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CliSettings {
    /// The theme by name. Absent means "detect from the terminal background".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    /// How much of a change to show: `unified` or `minimal`.
    ///
    /// Absent means `unified`, which is what every configuration file written
    /// before 0.3.0 means — so this key needs no migration and an older binary
    /// reading a file that has it ignores it, because `[app.io-cli]` is the one
    /// section io-harness deliberately does not validate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff: Option<String>,
}

/// How much of a change a diff shows.
///
/// Two, not a number of context lines. The counter-pressure this answers is
/// approval fatigue: someone reviewing by file rather than by hunk wants the
/// changed lines and nothing else, and a dial from 0 to 3 is a dial nobody sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiffStyle {
    /// The hunk as the harness stored it, context and all.
    #[default]
    Unified,
    /// Changed lines only, with the `@@` header kept so the change still says
    /// where in the file it is.
    Minimal,
}

impl DiffStyle {
    /// What a configured value means. An unrecognised one is `Unified` rather
    /// than an error: `[app.io-cli]` is unvalidated by design, and refusing to
    /// start a session over a typo in a cosmetic key would be the wrong trade.
    pub fn from_setting(value: Option<&str>) -> Self {
        match value {
            Some("minimal") => Self::Minimal,
            _ => Self::Unified,
        }
    }
}

/// A default permission posture, in the words the wizard offers it in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Posture {
    /// Read, write and run inside the workspace; no outbound network.
    Workspace,
    /// Read freely; writes and commands ask first.
    AskWrites,
    /// Read only.
    ReadOnly,
}

impl Posture {
    pub const ALL: &'static [Posture] =
        &[Posture::Workspace, Posture::AskWrites, Posture::ReadOnly];

    pub fn label(self) -> &'static str {
        match self {
            Self::Workspace => "Sandboxed workspace",
            Self::AskWrites => "Ask before writes",
            Self::ReadOnly => "Read only",
        }
    }

    /// The short name the status line uses. Hyphenated rather than spaced, so the
    /// field is one token a reader's eye can skip over or stop on.
    pub fn short(self) -> &'static str {
        match self {
            Self::Workspace => "workspace",
            Self::AskWrites => "ask-writes",
            Self::ReadOnly => "read-only",
        }
    }

    /// The next posture in the cycle. It wraps, because one key that only ever
    /// moves one way is a key you press three times to undo.
    pub fn next(self) -> Self {
        match self {
            Self::Workspace => Self::AskWrites,
            Self::AskWrites => Self::ReadOnly,
            Self::ReadOnly => Self::Workspace,
        }
    }

    /// Which posture a set of defaults *is*, if it is one of them.
    ///
    /// `None` for a configuration file holding a policy nobody offered, which is
    /// allowed — io-harness's own file can express far more than three postures.
    /// Reporting such a policy as one of the three would put a true-looking word
    /// beside a boundary it does not describe.
    pub fn of(defaults: &Defaults) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|posture| &posture.defaults() == defaults)
    }

    pub fn detail(self) -> &'static str {
        match self {
            Self::Workspace => "read, write and run inside this repository; no outbound network",
            // True as of 0.2.0. Through 0.1.0 and 0.1.1 this line had to say that
            // a write was *declined* rather than asked about, because the approver
            // handed to the harness was `DenyAll` — a posture whose behaviour is
            // not what its name suggests has to say so at the moment it is chosen.
            Self::AskWrites => "read freely; a write or a command stops and asks you first",
            Self::ReadOnly => "read only; nothing is written and nothing is run",
        }
    }

    /// The policy defaults this posture is.
    ///
    /// A posture is an `io_harness::Policy`, not a flag of io-cli's own. That is
    /// what makes the status line able to name the layer in force, and what will
    /// make a refusal able to name the rule that produced it.
    pub fn defaults(self) -> Defaults {
        match self {
            Self::Workspace => Defaults {
                read: Effect::Allow,
                write: Effect::Allow,
                exec: Effect::Allow,
                net: Effect::Deny,
            },
            Self::AskWrites => Defaults {
                read: Effect::Allow,
                write: Effect::Ask,
                exec: Effect::Ask,
                net: Effect::Deny,
            },
            Self::ReadOnly => Defaults {
                read: Effect::Allow,
                write: Effect::Deny,
                exec: Effect::Deny,
                net: Effect::Deny,
            },
        }
    }
}

/// The whole file, as it will be written.
#[derive(Debug, Serialize)]
struct File<'a> {
    provider: Vec<&'a ProviderSpec>,
    policy: PolicySection,
    app: AppSection,
}

#[derive(Debug, Serialize)]
struct PolicySection {
    defaults: Defaults,
}

#[derive(Debug, Serialize)]
struct AppSection {
    #[serde(rename = "io-cli")]
    io_cli: CliSettings,
}

/// Render the configuration file's text.
///
/// Separate from writing it so the confirmation screen can show exactly what is
/// about to land, and so a test can read it without a filesystem.
pub fn render(
    spec: &ProviderSpec,
    posture: Posture,
    theme: &str,
) -> Result<String, toml::ser::Error> {
    let file = File {
        provider: vec![spec],
        policy: PolicySection {
            defaults: posture.defaults(),
        },
        app: AppSection {
            io_cli: CliSettings {
                theme: Some(theme.to_string()),
                // Left out of the file the wizard writes. Its absence is
                // `unified`, and a key written with its own default is a key a
                // reader has to wonder about — and one that would have to be
                // rewritten if the default ever changed.
                diff: None,
            },
        },
    };
    toml::to_string_pretty(&file)
}

/// Where io-harness looks for the user-scope file.
///
/// The harness's own function, not a copy of its rules: `$IO_CONFIG`, else
/// `$IO_CONFIG_HOME/io.toml`, else the platform's own place. Duplicating that
/// here would be a second answer to a question the harness already answers, and
/// the two would drift.
pub fn user_path() -> Option<PathBuf> {
    io_harness::config::user_path()
}

/// Write the file, creating its directory, with mode `0600` on unix.
///
/// The mode is set on the file that is created rather than afterwards, so there
/// is no window in which a key sits on disk world-readable. This is what `gh`,
/// `aws` and `npm` do; an OS keychain is a later question and not obviously a
/// better answer, since one that fails silently on a headless Linux box is worse
/// than a file that never does.
pub fn write(path: &Path, contents: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    #[cfg(unix)]
    {
        use std::io::Write as _;
        use std::os::unix::fs::OpenOptionsExt;

        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        file.write_all(contents.as_bytes())?;
        file.sync_all()
    }
    #[cfg(not(unix))]
    {
        // Windows has no mode bits. The file lands in the user's own roaming
        // profile, which is already per-user, and pretending otherwise by
        // reporting a mode we did not set would be worse than saying nothing.
        std::fs::write(path, contents)
    }
}
