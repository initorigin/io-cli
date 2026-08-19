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

/// Everything io-cli itself remembers.
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
    /// Which glyph set to draw with: `unicode` or `ascii`.
    ///
    /// Absent means "ask the locale", which is what every file written before
    /// 0.6.0 means. It is a separate key from the theme and from `plain` on
    /// purpose: a terminal that cannot draw `›` may still be perfectly happy
    /// with colour, and a reader who wants the animation stilled may be reading
    /// on a terminal that draws every glyph in the set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub glyphs: Option<String>,
    /// Whether to run in plain mode without being asked each time.
    ///
    /// The same switch as `--plain`, and the flag wins when both are present —
    /// a flag is this run and a file is every run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plain: Option<bool>,
    /// The session's keys, by action name: `[app.io-cli.keys]`.
    ///
    /// A map rather than a struct of named fields on purpose. A struct would
    /// make an action nobody has heard of a *deserialization* failure, which
    /// would take the whole section down — theme, diff style, glyphs and plain
    /// mode with it — over a misspelt keybinding. A map lets
    /// [`crate::keys::Keys::resolve`] answer for each line on its own and say
    /// which names it does know, which is the difference between a typo that
    /// costs one key and a typo that costs every setting in the file.
    ///
    /// `BTreeMap` rather than `HashMap` so the notices a bad file produces come
    /// out in the same order every time; a diagnostic that shuffles is one
    /// nobody can compare against the last run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keys: Option<std::collections::BTreeMap<String, String>>,
    /// The caps a fan-out runs under: `[app.io-cli.containment]`.
    ///
    /// **This key is what turns the fleet on, and it is not a preference.**
    /// `Session::turn_contained_observed` is the only session entry point that
    /// passes a containment into the driver, and therefore the only one that
    /// reaches the loop owning the spawn tool — so a session with no caps
    /// configured cannot decompose anything, and one with them runs a materially
    /// different turn. Absent means every turn is the steered turn 0.7.0 shipped.
    ///
    /// io-harness's own type rather than four fields of io-cli's own, because it
    /// is `Serialize`/`Deserialize` for exactly this purpose and because a
    /// second spelling of the caps would be a second thing to keep true. It
    /// carries the crate's own `#[serde(alias = "max_concurrent")]`, so a file
    /// written against the pre-0.32.0 name still reads.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub containment: Option<io_harness::Containment>,
    /// MCP servers for the turn: `[[app.io-cli.mcp]]`.
    ///
    /// io-harness's own `McpServer`, which is `Deserialize` for exactly this
    /// purpose. **It reaches a turn only where a contract does**, which today is
    /// the contained turn — see [`crate::contract`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp: Option<Vec<io_harness::McpServer>>,
    /// Language servers for this workspace: `[[app.io-cli.lsp]]`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lsp: Option<Vec<io_harness::LspServer>>,
    /// A browser the agent may drive: `[app.io-cli.browser]`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub browser: Option<io_harness::BrowserConfig>,
    /// The directory io-harness discovers skills in: `skills = "..."`.
    ///
    /// A path and not a list, because discovery is the harness's and io-cli
    /// parses no skill file of its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skills: Option<std::path::PathBuf>,
}

/// The caps this session runs its turns under, if any.
///
/// A function rather than a field read at the call site so that the decision has
/// somewhere a test can reach: `src/main.rs` cannot be linked by anything under
/// `tests/`, which is the same reason [`plain`] lives here.
pub fn containment(stored: Option<&CliSettings>) -> Option<&io_harness::Containment> {
    stored.and_then(|settings| settings.containment.as_ref())
}

/// What a contained turn gives up, in the words the session says it in.
///
/// **Disclosure rather than decoration.** A contained turn takes no `SteerInbox`,
/// so text typed mid-turn cannot redirect it, and it is built from a contract
/// io-cli writes rather than from io-harness's `[run]` and `[sandbox]` sections —
/// so budgets and an agent roster set there still do not reach it. None of that
/// is visible from the screen, and a mode that silently drops a step cap somebody
/// set is the worst kind of quiet.
///
/// Since 0.10.0 the sentence also says what the mode *gains*, because that is now
/// the more surprising half: this is the only turn that can be given a responder,
/// a plan gate, MCP servers, language servers, a browser or skills, and an
/// operator who turned containment on for the fan-out has just turned all of them
/// on too.
pub fn contained_notice(caps: &io_harness::Containment, dash: &str) -> String {
    format!(
        "contained {dash} up to {} agents, {} at once per tier, {} deep, {} tokens for the \
         tree. This is the turn that carries a contract: questions are answered here, a plan \
         is decided here, and [app.io-cli] skills, mcp, lsp and browser apply here. It cannot \
         be steered mid-flight, and takes no agent roster, no [run] budget and no [sandbox]; \
         Ctrl+C still ends it.",
        caps.max_total_agents, caps.max_concurrent_agents, caps.max_depth, caps.max_total_tokens,
    )
}

/// io-cli's own section, and what was wrong with it.
///
/// **This is F10, and it exists because `.unwrap_or_default()` on the `Result`
/// was the whole of the old behaviour.** io-harness answers `Config::app` with
/// three distinct outcomes — the section is there and parsed, the section is not
/// there at all, or the section is there and could not be read — and collapsing
/// the third into the second meant that one mistyped value silently reverted the
/// theme, the diff style, the glyph set, plain mode and every keybinding at
/// once, with nothing said about any of it. A setting that quietly goes back to
/// its default is worse than one that fails loudly: the operator sees a session
/// that looks almost right and has no thread to pull.
///
/// The notice carries **the harness's own message**, which already names the
/// section and the key that broke — rewording it here would drop the only part
/// that says where to look.
///
/// It lives in the library rather than at the two call sites in `src/main.rs`
/// because nothing under `tests/` can link the binary: a decision written there
/// is one no test drives and no sabotage can make fail.
pub fn stored(config: &io_harness::Config) -> (Option<CliSettings>, Option<String>) {
    match config.app(APP_KEY) {
        Ok(stored) => (stored, None),
        Err(error) => (
            None,
            Some(format!(
                "{error}; this session is running on the default settings until that is fixed"
            )),
        ),
    }
}

/// Whether this session runs in plain mode: `--plain`, or `[app.io-cli] plain`.
///
/// **A pure function, and it lives here rather than in `src/main.rs` on purpose.**
/// The binary has no automated coverage by construction — an integration test
/// cannot link it — so a decision written inline there is one no test can drive
/// and no sabotage can be made to fail. Two earlier releases had to move a
/// decision out of `main.rs` for exactly that reason, and this is the third.
///
/// **The flag wins over the file**, because a flag is this run and a file is
/// every run. That has teeth in one direction only, and saying so is more honest
/// than implying a precedence there is no way to exercise: there is no
/// `--no-plain`, so a file that says `plain = true` cannot be turned off from the
/// command line for one session. The asymmetry is the right way round —
/// accessibility is a thing somebody switched on deliberately, and a mode that
/// can be lost to a stray flag is not one you can rely on.
///
/// An absent key is `false`, which is what every configuration file written
/// before 0.6.0 means. `Some(false)` and `None` therefore answer the same, and
/// the distinction is kept in the type only so that a file can state the default
/// without the wizard ever writing it — plain mode is asked for, never inferred.
pub fn plain(flag: bool, stored: Option<&CliSettings>) -> bool {
    flag || stored.is_some_and(|settings| settings.plain.unwrap_or(false))
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
                // Left out for the same reason, and with more force. The glyph
                // set the wizard ran under was chosen from the locale of the
                // machine it ran on; writing it down would freeze that answer
                // into a file that may later be read on another terminal, and
                // turn a detected default into a stated preference nobody
                // stated. Plain mode likewise: it is asked for, never inferred.
                glyphs: None,
                plain: None,
                // Left out for the strongest reason of the four: writing the
                // defaults down would make every later change to a default a
                // change that only reaches new installations, and would put a
                // table of five bindings in a file the wizard's user never
                // asked to edit. The keys are documented; they are not written.
                keys: None,
                // Left out with the most force of all: this key changes what a
                // turn *is*, not how it looks. The wizard asks nothing about
                // fan-out, and a file that arrived with caps already in it would
                // have turned steering off for somebody who never chose to.
                containment: None,
                // The four capability keys are left out for the same reason as
                // the caps above and with the same force: each reaches a turn
                // only through a contract, which only a contained turn takes, so
                // writing any of them would turn fan-out on for somebody who
                // never chose it. The wizard asks about none of them.
                mcp: None,
                lsp: None,
                browser: None,
                skills: None,
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

/// The run store, beside the configuration file.
///
/// That is the directory this product already owns, and asking for a second one
/// buys nothing. It lives here rather than in the binary because both entry
/// points need it: an interactive session and a headless `io exec` write to the
/// same store, which is what lets `/resume` list a run that CI started.
pub fn store_path() -> Option<PathBuf> {
    Some(user_path()?.parent()?.join("runs.db"))
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
