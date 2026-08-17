//! The command line. Five flags and one subcommand, because everything else is
//! a slash command inside the session.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// `io` — a terminal agent over io-harness.
#[derive(Debug, Parser)]
#[command(name = "io", version, about, long_about = None)]
pub struct Cli {
    /// The workspace to work in. Defaults to the current directory.
    #[arg(short = 'C', long, value_name = "DIR")]
    pub dir: Option<PathBuf>,

    /// The model to use for this session, overriding the configured one.
    #[arg(short, long, value_name = "MODEL")]
    pub model: Option<String>,

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
