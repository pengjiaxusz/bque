mod bench;
mod cli;
mod commands;
mod governor;
mod job;
mod lock;
mod runner;
mod state;
pub mod ui;
mod update;

use clap::Parser;
use cli::{Cli, Commands};

fn main() -> anyhow::Result<()> {
    ui::enable_ansi_support();
    update::clean_old_binaries();
    update::maybe_spawn_background_check();

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
        Commands::Update(args) => {
            update::handle_update(args)?;
            0
        }
    };

    if exit_code != 0 {
        std::process::exit(exit_code);
    }

    Ok(())
}
