// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::process::ExitCode;

use clap::{Parser, Subcommand};
use stint_lib::note::DAYS_KEY;
use stint_lib::vault::{self, TimerError};

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Start or stop the running span of the note with this name
    Toggle { note: String },
    /// Rebuild worked_days from worked_on in every note
    Sync,
}

fn main() -> ExitCode {
    match run(Cli::parse().command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("stint: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(command: Command) -> Result<(), TimerError> {
    let vault = vault::vault_dir()?;
    match command {
        Command::Toggle { note } => {
            let toggle = vault::toggle(&vault, &note)?;
            println!("stint: {toggle} {note}");
        }
        Command::Sync => {
            for name in vault::sync(&vault)? {
                println!("stint: synced {DAYS_KEY} in {name}");
            }
        }
    }
    Ok(())
}
