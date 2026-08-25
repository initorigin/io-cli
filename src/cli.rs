//! The command line. A handful of flags and two subcommands, because everything
//! else is a slash command inside the session.
//!
//! The count is deliberately not stated. It said "five" from 0.2.0 until 0.6.0
//! and was wrong for most of that time, which is the argument against writing a
//! number into prose that nothing checks.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// `io` — a terminal agent over io-harness.
#[derive(Debug, Parser)]
#[command(name = "io", version, about, long_about = None)]
pub struct Cli {
    /// The workspace to work in. Defaults to the current directory.
    ///
    /// `global` so it is accepted on either side of a subcommand: `io -C dir
    /// exec "…"` and `io exec -C dir "…"` are the same command. Without it clap
    /// takes a flag declared here as belonging strictly before the subcommand,
    /// which is not how anyone types it and not how the README documents it.
    #[arg(short = 'C', long, value_name = "DIR", global = true)]
    pub dir: Option<PathBuf>,

    /// A named profile from the configuration file, for this run only.
    ///
    /// `[profile.<name>]` is io-harness's own — a profile body is the file
    /// format again, applied over the merged scopes through the same merge they
    /// use. It has been in the harness since its 0.27.0 and no io-cli release
    /// selected one until 0.16.0.
    ///
    /// `global` for the reason `-C` and `-m` are: a set of choices you want for
    /// one run is exactly the thing you type on either side of a subcommand, and
    /// `io exec --profile ci "…"` is the shape CI reaches for. Nothing is
    /// written — a profile chosen here lasts the run and no longer.
    #[arg(long, value_name = "NAME", global = true)]
    pub profile: Option<String>,

    /// The model to use for this run, overriding the configured one.
    #[arg(short, long, value_name = "MODEL", global = true)]
    pub model: Option<String>,

    /// Run without animation: nothing turns, nothing moves, and every state the
    /// session enters is written into the terminal's scrollback as one line of
    /// text. Forces the ASCII glyph set.
    ///
    /// For a screen reader, a braille display, a serial console and a captured
    /// log — surfaces on which a spinner is not a quiet decoration but a cell
    /// that changes ten times a second with no new information in it, and on
    /// which a status line that only ever repaints is a state nobody can read.
    ///
    /// `global` for the same reason `-C` and `-m` are: `io --plain exec "…"` and
    /// `io exec --plain "…"` are one command, and 0.5.0 shipped a defect where
    /// `-m` after the subcommand was rejected. A flag whose acceptance depends on
    /// which side of a word it is typed is a flag that works on the author's
    /// machine.
    ///
    /// It reaches an interactive session and stops there. `io exec` constructs no
    /// theme, draws nothing and animates nothing already — see `crate::exec` —
    /// so there is no second thing for the flag to switch off there, and wiring
    /// it in would be a knob with no wire behind it.
    #[arg(long, global = true)]
    pub plain: bool,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Run the first-run wizard again: provider, credential, model, theme and
    /// permission posture, written to io-harness's configuration file.
    Setup,
    /// Run one goal to completion without a terminal, and exit with a status
    /// that says how the run ended.
    Exec(Exec),
}

/// `io exec` — the headless entry point.
///
/// It takes no keyboard and draws nothing. Everything an interactive session
/// decides by keystroke is either a flag here or a line in the configuration
/// file, and the flags are deliberately few: a headless run that needs six of
/// them is a configuration file with worse ergonomics.
#[derive(Debug, clap::Args)]
pub struct Exec {
    /// What the agent should do.
    pub goal: String,

    /// Write the run's events to stdout as newline-delimited JSON, one object
    /// per line, instead of the agent's reply.
    ///
    /// The objects are `io_harness::RunEvent` serialized by io-harness's own
    /// derive — the same shape its `[[hook]]` writer appends and its store
    /// keeps. Nothing else is written to stdout, so the stream pipes straight
    /// into a JSON reader.
    #[arg(long)]
    pub json: bool,

    /// Where a command this run executes may write. Defaults to `[sandbox]`.
    ///
    /// This is not the same axis as `--policy`: `--sandbox` is where the
    /// sandbox lets a command write, `--policy` is what the agent is permitted
    /// to attempt at all. They share the word `read-only` and mean different
    /// things by it.
    #[arg(long, value_enum, value_name = "MODE")]
    pub sandbox: Option<Sandbox>,

    /// The permission posture for this run. Defaults to `[policy]`.
    ///
    /// `ask-writes` is refused here: nothing in an unattended run can answer an
    /// approval, so honouring it would turn *ask* into *deny* without saying so.
    #[arg(long, value_enum, value_name = "POSTURE")]
    pub policy: Option<PolicyFlag>,

    /// Take the provider from the environment instead of a configuration file.
    ///
    /// For CI, where nothing should be written to disk: the credential and the
    /// model come from the pair of variables io-harness's own `from_env`
    /// constructors read, and `-m` overrides the model half.
    #[arg(long, value_enum, value_name = "NAME")]
    pub provider: Option<FromEnv>,
}

/// `--provider`, for a run with no configuration file.
///
/// `compatible` is deliberately absent. io-harness gives it no `from_env` of its
/// own, for the reason its source states: a base URL or a preset has to come
/// from somewhere, and a base URL on a command line is a configuration file with
/// worse ergonomics. A `compatible` endpoint reaches `io exec` through
/// `io.toml`, which already works.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum FromEnv {
    /// Named as io-harness's own `ProviderSpec` tag spells it, not as clap
    /// would derive it from the variant — `openrouter`, never `open-router`.
    #[value(name = "openrouter")]
    OpenRouter,
    #[value(name = "anthropic")]
    Anthropic,
    #[value(name = "openai")]
    OpenAi,
}

impl FromEnv {
    /// The credential and model variables, which are io-harness's own names —
    /// so a shell that already works with the harness works here unchanged.
    pub fn vars(self) -> (&'static str, &'static str) {
        match self {
            Self::OpenRouter => ("OPENROUTER_API_KEY", "OPENROUTER_MODEL"),
            Self::Anthropic => ("ANTHROPIC_API_KEY", "ANTHROPIC_MODEL"),
            Self::OpenAi => ("OPENAI_API_KEY", "OPENAI_MODEL"),
        }
    }
}

/// `--sandbox`, in `io_harness::ExecMode`'s own kebab-case names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Sandbox {
    /// Commands may read the workspace and write nothing.
    #[value(name = "read-only")]
    ReadOnly,
    /// Commands may write inside the workspace.
    #[value(name = "workspace-write")]
    WorkspaceWrite,
    /// No confinement. A widening a checked-in configuration file is not
    /// allowed to express, which is why using it prints a line on stderr.
    #[value(name = "full-access")]
    FullAccess,
}

impl Sandbox {
    pub fn mode(self) -> io_harness::ExecMode {
        match self {
            Self::ReadOnly => io_harness::ExecMode::ReadOnly,
            Self::WorkspaceWrite => io_harness::ExecMode::WorkspaceWrite,
            Self::FullAccess => io_harness::ExecMode::FullAccess,
        }
    }
}

/// `--policy`, in the words the status line and the wizard already use.
///
/// The value names are `settings::Posture::short()`, reused rather than
/// re-spelled; `tests/exec.rs` asserts the two lists are the same, because a
/// flag that disagrees with the status line is a flag that teaches the wrong
/// vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum PolicyFlag {
    /// Read, write and run inside the workspace; no outbound network.
    #[value(name = "workspace")]
    Workspace,
    /// Read freely; writes and commands ask first. Refused for a headless run.
    #[value(name = "ask-writes")]
    AskWrites,
    /// Read only.
    #[value(name = "read-only")]
    ReadOnly,
}
