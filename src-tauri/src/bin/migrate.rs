// One-time move of frontmatter worked_on spans into each note's ### Log.
use std::path::PathBuf;
use std::process::ExitCode;

use stint_lib::vault::migrate;

// The vault is a required argument, never a default, so a test run cannot rewrite the real vault by accident.
fn main() -> ExitCode {
    let Some(vault) = std::env::args_os().nth(1).map(PathBuf::from) else {
        eprintln!("usage: migrate <vault>");
        return ExitCode::FAILURE;
    };

    match migrate::migrate(&vault) {
        Ok(migrated) => {
            for outcome in migrated {
                println!("migrate: {outcome}");
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("migrate: {error}");
            ExitCode::FAILURE
        }
    }
}
