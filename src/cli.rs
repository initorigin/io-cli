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
}
