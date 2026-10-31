mod bench;
mod cli;
mod commands;
mod governor;
mod job;
mod lock;
mod runner;
mod state;
pub mod ui;

use clap::Parser;
use cli::{Cli, Commands};

fn main() -> anyhow::Result<()> {
    ui::enable_ansi_support();
    let cli = Cli::parse();

    let exit_code = match cli.command {
        Commands::Run(args) => commands::handle_run(args)?,
        Commands::Bench(args) => bench::run_benchmark(args)?,
        Commands::Status(args) => {
            commands::handle_status(args)?;
            0
        }
        Commands::Pause(args) => {
            commands::handle_pause(args)?;
            0
        }
        Commands::Resume => {
            commands::handle_resume()?;
            0
        }
        Commands::Clean => {
            commands::handle_clean()?;
            0
        }
    };

    if exit_code != 0 {
        std::process::exit(exit_code);
    }

    Ok(())
}
