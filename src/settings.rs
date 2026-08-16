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

    pub fn detail(self) -> &'static str {
        match self {
            Self::Workspace => "read, write and run inside this repository; no outbound network",
            // Said plainly rather than implied. The overlay that asks a human is
            // 0.2.0's; until it exists, "ask" is answered by declining, and a
            // posture whose behaviour is not what its name suggests has to say so
            // at the moment it is chosen.
            Self::AskWrites => {
                "read freely; a write or a command is declined until the approval surface lands"
            }
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
