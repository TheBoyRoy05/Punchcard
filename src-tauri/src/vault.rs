use std::fmt;
use std::fs;
use std::io;
use std::iter;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::process::Command;

use chrono::Local;

use crate::note::{self, DAYS_KEY, LIST_ITEM, Note, SPAN_FORMAT, SPANS_KEY};

const MACHINE_ENV: &str = ".config/machine.env";

#[derive(Debug)]
pub struct TimerError(String);

#[derive(Debug, PartialEq)]
pub enum Toggle {
    Started,
    Stopped,
}

impl fmt::Display for TimerError {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for TimerError {}

// Tauri commands send errors to the picker window as plain messages.
impl serde::Serialize for TimerError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl From<io::Error> for TimerError {
    fn from(error: io::Error) -> Self {
        Self(error.to_string())
    }
}

impl From<String> for TimerError {
    fn from(message: String) -> Self {
        Self(message)
    }
}

impl fmt::Display for Toggle {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str(match self {
            Toggle::Started => "Timer Started",
            Toggle::Stopped => "Timer Stopped",
        })
    }
}

pub fn vault_dir() -> Result<PathBuf, TimerError> {
    if let Some(vault) = std::env::var_os("VAULT") {
        return Ok(PathBuf::from(vault));
    }

    let home = std::env::home_dir().ok_or_else(|| TimerError("no home directory".to_string()))?;
    let machine_env = home.join(MACHINE_ENV);
    if !machine_env.is_file() {
        let message = format!("{} missing, run Config/setup.sh", machine_env.display());
        return Err(TimerError(message));
    }

    let output = Command::new("sh")
        .args(["-c", r#". "$1" && printf %s "$WORK""#, "sh"])
        .arg(&machine_env)
        .output()?;
    if !output.status.success() {
        let message = format!("sourcing {} failed", machine_env.display());
        return Err(TimerError(message));
    }

    Ok(PathBuf::from(String::from_utf8_lossy(&output.stdout).as_ref()).join("Vault"))
}

pub fn notes(vault: &Path) -> Result<Vec<Note>, TimerError> {
    let mut notes = Vec::new();
    collect_notes(vault, &mut notes)?;
    Ok(notes)
}

pub fn toggle(vault: &Path, name: &str) -> Result<(Toggle, Note), TimerError> {
    let notes = notes(vault)?;
    let matches: Vec<&Note> = notes.iter().filter(|note| note.name() == name).collect();
    let [note] = matches.as_slice() else {
        return Err(TimerError(format!("{} notes named {name}", matches.len())));
    };

    let now = Local::now().format(SPAN_FORMAT).to_string();
    let mut lines = read_lines(&note.path)?;
    let toggle = toggle_span(&mut lines, &now, name)?;

    let fields = note::parse_frontmatter(&lines).ok_or_else(|| unclosed(name))?;
    let toggled = Note {
        path: note.path.clone(),
        fields,
    };
    set_days(&mut lines, &toggled.days(), name)?;
    write_lines(&note.path, &lines)?;
    Ok((toggle, toggled))
}

pub fn sync(vault: &Path) -> Result<Vec<String>, TimerError> {
    let mut synced = Vec::new();
    for note in notes(vault)? {
        let days = note.days();
        if note.list(DAYS_KEY) == days.as_slice() {
            continue;
        }

        let mut lines = read_lines(&note.path)?;
        set_days(&mut lines, &days, note.name())?;
        write_lines(&note.path, &lines)?;
        synced.push(note.name().to_string());
    }
    Ok(synced)
}

fn collect_notes(dir: &Path, notes: &mut Vec<Note>) -> Result<(), TimerError> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if entry.file_name().to_string_lossy().starts_with(['.', '_']) {
            continue;
        }

        let path = entry.path();
        if entry.file_type()?.is_dir() {
            collect_notes(&path, notes)?;
        } else if path.extension().is_some_and(|extension| extension == "md")
            && let Some(fields) = note::parse_frontmatter(&read_lines(&path)?)
        {
            notes.push(Note { path, fields });
        }
    }
    Ok(())
}

fn toggle_span(lines: &mut Vec<String>, now: &str, name: &str) -> Result<Toggle, TimerError> {
    let end = frontmatter_end(lines, name)?;
    let started = format!("{LIST_ITEM}{now} -");

    let Some(key_line) = key_line(&lines[..end], SPANS_KEY) else {
        lines.splice(end..end, [format!("{SPANS_KEY}:"), started]);
        return Ok(Toggle::Started);
    };

    if lines[key_line].trim_end() != format!("{SPANS_KEY}:") {
        let message = format!("{name} has {SPANS_KEY} on one line, expected a list");
        return Err(TimerError(message));
    }

    let spans = list_items(lines, key_line, end);
    if !spans.is_empty() && lines[spans.end - 1].trim_end().ends_with(" -") {
        let running = spans.end - 1;
        lines[running] = format!("{} {now}", lines[running].trim_end());
        return Ok(Toggle::Stopped);
    }

    lines.insert(spans.end, started);
    Ok(Toggle::Started)
}

fn set_days(lines: &mut Vec<String>, days: &[String], name: &str) -> Result<(), TimerError> {
    let end = frontmatter_end(lines, name)?;
    let block: Vec<String> = if days.is_empty() {
        Vec::new()
    } else {
        let items = days.iter().map(|day| format!("{LIST_ITEM}\"{day}\""));
        iter::once(format!("{DAYS_KEY}:")).chain(items).collect()
    };

    let replaced = match key_line(&lines[..end], DAYS_KEY) {
        Some(key_line) => key_line..list_items(lines, key_line, end).end,
        None => end..end,
    };
    lines.splice(replaced, block);
    Ok(())
}

fn frontmatter_end(lines: &[String], name: &str) -> Result<usize, TimerError> {
    let closing = lines.iter().skip(1).position(|line| line == "---");
    closing.map(|index| index + 1).ok_or_else(|| unclosed(name))
}

fn key_line(frontmatter: &[String], key: &str) -> Option<usize> {
    frontmatter
        .iter()
        .position(|line| line.split(':').next() == Some(key))
}

fn list_items(lines: &[String], key_line: usize, end: usize) -> Range<usize> {
    let first = key_line + 1;
    let count = lines[first..end]
        .iter()
        .take_while(|line| line.starts_with(LIST_ITEM))
        .count();
    first..first + count
}

fn unclosed(name: &str) -> TimerError {
    TimerError(format!("{name} has no closing --- for its frontmatter"))
}

fn read_lines(path: &Path) -> io::Result<Vec<String>> {
    Ok(fs::read_to_string(path)?
        .split('\n')
        .map(String::from)
        .collect())
}

fn write_lines(path: &Path, lines: &[String]) -> io::Result<()> {
    fs::write(path, lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: &str = "26/11/01 11:40";

    fn lines(text: &str) -> Vec<String> {
        text.split('\n').map(String::from).collect()
    }

    fn toggled(text: &str) -> (Toggle, String) {
        let mut lines = lines(text);
        let toggle = toggle_span(&mut lines, NOW, "Port").unwrap();
        (toggle, lines.join("\n"))
    }

    fn with_days(text: &str, days: &[&str]) -> String {
        let mut lines = lines(text);
        let days: Vec<String> = days.iter().map(|day| day.to_string()).collect();
        set_days(&mut lines, &days, "Port").unwrap();
        lines.join("\n")
    }

    #[test]
    fn toggle_adds_spans_key_when_missing() {
        let (toggle, text) = toggled("---\nstatus: Done\n---\nbody");

        assert_eq!(toggle, Toggle::Started);
        assert_eq!(
            text,
            "---\nstatus: Done\nworked_on:\n  - 26/11/01 11:40 -\n---\nbody"
        );
    }

    #[test]
    fn toggle_appends_span_after_finished_ones() {
        let (toggle, text) =
            toggled("---\nworked_on:\n  - 26/10/31 09:00 - 26/10/31 10:00\ntags:\n---");

        assert_eq!(toggle, Toggle::Started);
        assert_eq!(
            text,
            "---\nworked_on:\n  - 26/10/31 09:00 - 26/10/31 10:00\n  - 26/11/01 11:40 -\ntags:\n---"
        );
    }

    #[test]
    fn toggle_stops_running_span() {
        let (toggle, text) = toggled("---\nworked_on:\n  - 26/11/01 09:00 -  \n---");

        assert_eq!(toggle, Toggle::Stopped);
        assert_eq!(
            text,
            "---\nworked_on:\n  - 26/11/01 09:00 - 26/11/01 11:40\n---"
        );
    }

    #[test]
    fn toggle_starts_span_under_empty_key() {
        let (toggle, text) = toggled("---\nworked_on:\n---");

        assert_eq!(toggle, Toggle::Started);
        assert_eq!(text, "---\nworked_on:\n  - 26/11/01 11:40 -\n---");
    }

    #[test]
    fn toggle_refuses_spans_on_one_line() {
        let mut lines = lines("---\nworked_on: 26/11/01 09:00 -\n---");

        let error = toggle_span(&mut lines, NOW, "Port").unwrap_err();

        assert_eq!(
            error.to_string(),
            "Port has worked_on on one line, expected a list"
        );
    }

    #[test]
    fn set_days_replaces_existing_list() {
        let text = with_days(
            "---\nworked_days:\n  - \"[[26-10-30]]\"\ntags:\n---",
            &["[[26-10-31]]"],
        );

        assert_eq!(text, "---\nworked_days:\n  - \"[[26-10-31]]\"\ntags:\n---");
    }

    #[test]
    fn set_days_appends_key_when_missing() {
        let text = with_days("---\ntags:\n---", &["[[26-10-31]]"]);

        assert_eq!(text, "---\ntags:\nworked_days:\n  - \"[[26-10-31]]\"\n---");
    }

    #[test]
    fn set_days_removes_key_when_no_days() {
        let text = with_days("---\nworked_days:\n  - \"[[26-10-30]]\"\n---", &[]);

        assert_eq!(text, "---\n---");
    }
}
