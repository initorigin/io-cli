use clap::Parser;

use io_cli::cli::{Cli, Command};

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Setup) => todo!("the wizard arrives with US-IO-CLI-0.1.0-T11"),
        None => todo!("the session arrives with US-IO-CLI-0.1.0-T10"),
    }
}
