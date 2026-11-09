// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::fmt::Display;
use std::io::{self, Write};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use stint_lib::timeline::{self, Range};
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
    /// Open the timeline of today or this week, to drag, add and edit blocks
    Timeline {
        #[arg(value_enum, default_value = "day")]
        range: Range,
    },
    /// Start or stop the running block of the note with this name
    Toggle { note: String },
    /// Rebuild punchcard__worked_days and punchcard__worked_hours from every note's log, reporting bad headings, extra running blocks and buried logs
    Sync,
    /// Print Not Started and In Progress Tasks notes as JSON lines, with estimates and unfinished blockers; notes that do not parse are named on stderr
    Tasks {
        /// Keep tasks with this tag or a child of it, e.g. KnowledgeAnchor
        #[arg(long)]
        tag: Option<String>,
    },
}

fn main() -> ExitCode {
    let command = Cli::parse().command;
    let changes_timer = matches!(command, Command::Pick | Command::Toggle { .. });

    match run(command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("stint: {error}");
            if changes_timer {
                notify::show("Timer not changed", &error.to_string(), None);
            }
            ExitCode::FAILURE
        }
    }
}

fn run(command: Command) -> Result<(), TimerError> {
    let vault = vault::vault_dir()?;
    match command {
        Command::Pick => picker::open(vault)?,
        Command::Timeline { range } => timeline::open(vault, range)?,
        Command::Toggle { note } => toggle_and_notify(&vault, &note)?,
        Command::Sync => {
            let synced = vault::sync(&vault)?;
            print_lines(synced.iter().map(|synced| format!("stint: {synced}")))?;
        }
        Command::Tasks { tag } => {
            let listing = vault::tasks::tasks(&vault, tag.as_deref())?;
            print_lines(listing.lines)?;
            for skipped in listing.skipped {
                eprintln!("stint: {skipped}");
            }
        }
    }
    Ok(())
}

fn print_lines(lines: impl IntoIterator<Item = impl Display>) -> io::Result<()> {
    write_lines(io::stdout().lock(), lines)
}

fn write_lines(
    mut out: impl Write,
    lines: impl IntoIterator<Item = impl Display>,
) -> io::Result<()> {
    for line in lines {
        match writeln!(out, "{line}") {
            // A reader like `head` closes the pipe once it has enough, which is not a failure.
            Err(error) if error.kind() == io::ErrorKind::BrokenPipe => return Ok(()),
            result => result?,
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ClosedPipe;

    impl Write for ClosedPipe {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::ErrorKind::BrokenPipe.into())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn a_closed_pipe_ends_output_without_error() {
        assert!(write_lines(ClosedPipe, ["a", "b"]).is_ok());
    }

    #[test]
    fn lines_are_written_one_per_line() {
        let mut out = Vec::new();

        write_lines(&mut out, ["a", "b"]).unwrap();

        assert_eq!(out, b"a\nb\n");
    }
}
