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
}
