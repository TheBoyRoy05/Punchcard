// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::process::ExitCode;

use clap::{Parser, Subcommand};
use stint_lib::vault::{self, TimerError};
use stint_lib::{notify, picker, toggle_and_notify};

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Open the ranked task picker; Enter toggles the selected task
    Pick,
    /// Start or stop the running block of the note with this name
    Toggle { note: String },
    /// Rebuild worked_days and worked_hours from every note's log, reporting bad headings, extra running blocks and buried logs
    Sync,
}

fn main() -> ExitCode {
    match run(Cli::parse().command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("stint: {error}");
            notify::show("Timer not changed", &error.to_string(), None);
            ExitCode::FAILURE
        }
    }
}

fn run(command: Command) -> Result<(), TimerError> {
    let vault = vault::vault_dir()?;
    match command {
        Command::Pick => picker::open(vault)?,
        Command::Toggle { note } => toggle_and_notify(&vault, &note)?,
        Command::Sync => {
            for synced in vault::sync(&vault)? {
                println!("stint: {synced}");
            }
        }
    }
    Ok(())
}
