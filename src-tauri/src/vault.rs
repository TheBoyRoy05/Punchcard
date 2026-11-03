use std::fmt;
use std::fs;
use std::io;
use std::iter;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::process::Command;

use chrono::{Local, NaiveDateTime, TimeDelta};
use punchcard_core::day::{DayStart, worked_days};
use punchcard_core::span::{Span, shorter_than, worked_hours};
use punchcard_core::time::{format_day, wall_clock};
use punchcard_core::{
    Block, LOG_HEADING, Log, entry_lines, has_buried_log, insert_block, log_blocks, log_heading,
};

use crate::note::{DAYS_KEY, HOURS_KEY, LIST_ITEM, Note};
use settings::Settings;

pub mod migrate;
pub mod settings;

const MACHINE_ENV: &str = ".config/machine.env";

#[derive(Debug)]
pub struct TimerError(String);

#[derive(Debug, PartialEq)]
pub enum Toggle {
    Started,
    Resumed,
    Stopped,
    Discarded,
}

#[derive(Debug, PartialEq)]
pub enum Synced {
    Rebuilt { note: String },
    ReportedBadHeading { note: String, heading: String },
    ReportedExtraRunning { note: String },
    ReportedBuriedLog { note: String },
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
            Toggle::Resumed => "Timer Resumed",
            Toggle::Stopped => "Timer Stopped",
            Toggle::Discarded => "Timer Stopped, Short Block Discarded",
        })
    }
}

impl fmt::Display for Synced {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Synced::Rebuilt { note } => {
                write!(formatter, "rebuilt {DAYS_KEY} and {HOURS_KEY} in {note}")
            }
            Synced::ReportedBadHeading { note, heading } => {
                write!(
                    formatter,
                    "{note} has a log heading that does not parse: {heading}"
                )
            }
            Synced::ReportedExtraRunning { note } => {
                formatter.write_str(&extra_running_message(note))
            }
            Synced::ReportedBuriedLog { note } => formatter.write_str(&buried_log_message(note)),
        }
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
    let settings = settings::read(vault)?;
    let notes = notes(vault)?;
    let matches: Vec<&Note> = notes.iter().filter(|note| note.name() == name).collect();
    let [note] = matches.as_slice() else {
        return Err(TimerError(format!("{} notes named {name}", matches.len())));
    };

    let mut lines = read_lines(&note.path)?;
    let now = wall_clock(Local::now().naive_local());
    let toggle = toggle_block(&mut lines, now, &settings, name)?;
    rebuild_derived(&mut lines, settings.day_start, name)?;
    write_lines(&note.path, &lines)?;

    let toggled = Note::from_lines(note.path.clone(), &lines).ok_or_else(|| unclosed(name))?;
    Ok((toggle, toggled))
}

pub fn sync(vault: &Path) -> Result<Vec<Synced>, TimerError> {
    let day_start = settings::read(vault)?.day_start;
    let mut synced = Vec::new();
    for note in notes(vault)? {
        let name = note.name();
        let lines = read_lines(&note.path)?;

        for heading in &note.log.bad_headings {
            synced.push(Synced::ReportedBadHeading {
                note: name.to_string(),
                heading: heading.clone(),
            });
        }

        if note.log.running_blocks().count() > 1 {
            synced.push(Synced::ReportedExtraRunning {
                note: name.to_string(),
            });
        }

        if has_buried_log(&lines) {
            synced.push(Synced::ReportedBuriedLog {
                note: name.to_string(),
            });
        }

        let mut rebuilt = lines.clone();
        rebuild_derived(&mut rebuilt, day_start, name)?;
        if rebuilt != lines {
            write_lines(&note.path, &rebuilt)?;
            synced.push(Synced::Rebuilt {
                note: name.to_string(),
            });
        }
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
        } else if path.extension().is_some_and(|extension| extension == "md") {
            let lines = read_lines(&path)?;
            notes.extend(Note::from_lines(path, &lines));
        }
    }
    Ok(())
}

fn toggle_block(
    lines: &mut Vec<String>,
    now: NaiveDateTime,
    settings: &Settings,
    name: &str,
) -> Result<Toggle, TimerError> {
    if has_buried_log(lines) {
        return Err(buried_log(name));
    }

    if Log::parse(lines).running_blocks().count() > 1 {
        return Err(extra_running(name));
    }

    let blocks = log_heading(lines)
        .into_iter()
        .flat_map(|log| log_blocks(lines, log));
    let (mut running, closed): (Vec<_>, Vec<_>) =
        blocks.partition(|(_, block)| block.span.end.is_none());

    if let Some((index, mut block)) = running.pop() {
        // In the repeated fall-back hour, wall-clock now can be before the start.
        if now < block.span.start {
            let message = format!(
                "{name} has a running block that starts after now: {}",
                block.heading()
            );
            return Err(TimerError(message));
        }

        let entry = entry_lines(lines, index);
        let short = shorter_than(block.span.start, now, settings.block_min_minutes);
        let blank_entry = lines[entry.clone()]
            .iter()
            .all(|line| line.trim().is_empty());
        if short && blank_entry {
            lines.drain(index..entry.end);
            return Ok(Toggle::Discarded);
        }

        if now == block.span.start {
            let message = format!(
                "{name} has a running block with an entry that started this minute, stop it from the next minute on: {}",
                block.heading()
            );
            return Err(TimerError(message));
        }

        block.span.end = Some(now);
        lines[index] = block.heading();
        return Ok(Toggle::Stopped);
    }

    let newest = closed.into_iter().max_by_key(|(_, block)| block.span.start);
    // In the repeated fall-back hour a block can end after wall-clock now, and reopening it could start after now.
    if let Some((index, mut block)) = newest
        && let Some(end) = block.span.end
        && end <= now
        && now - end <= TimeDelta::minutes(i64::from(settings.block_rejoin_minutes))
    {
        block.span.end = None;
        lines[index] = block.heading();
        return Ok(Toggle::Resumed);
    }

    let started = Block {
        span: Span {
            start: now,
            end: None,
        },
        entry: String::new(),
    };
    insert_block(lines, &started);
    Ok(Toggle::Started)
}

fn rebuild_derived(
    lines: &mut Vec<String>,
    day_start: DayStart,
    name: &str,
) -> Result<(), TimerError> {
    let log = Log::parse(lines);

    let days = worked_days(log.spans(), day_start).into_iter().map(|day| {
        format_day(day).ok_or_else(|| {
            let message = format!("{name} has a block on {day}, which has no two-digit year");
            TimerError(message)
        })
    });
    let days: Vec<String> = days.collect::<Result<_, _>>()?;
    let days_field = if days.is_empty() {
        Vec::new()
    } else {
        let items = days
            .into_iter()
            .map(|day| format!("{LIST_ITEM}\"[[{day}]]\""));
        iter::once(format!("{DAYS_KEY}:")).chain(items).collect()
    };
    set_field(lines, DAYS_KEY, days_field, name)?;

    let hours = worked_hours(log.spans());
    let hours_field = if hours == 0.0 {
        Vec::new()
    } else {
        vec![format!("{HOURS_KEY}: {hours}")]
    };
    set_field(lines, HOURS_KEY, hours_field, name)
}

fn set_field(
    lines: &mut Vec<String>,
    key: &str,
    field: Vec<String>,
    name: &str,
) -> Result<(), TimerError> {
    let end = frontmatter_end(lines, name)?;
    let replaced = match key_line(&lines[..end], key) {
        Some(key_line) => key_line..list_items(lines, key_line, end).end,
        None => end..end,
    };
    lines.splice(replaced, field);
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

fn buried_log(name: &str) -> TimerError {
    let message = buried_log_message(name);
    TimerError(format!("{message}, move it to the end by hand"))
}

fn extra_running(name: &str) -> TimerError {
    let message = extra_running_message(name);
    TimerError(format!("{message}, stop all but one by hand"))
}

fn buried_log_message(name: &str) -> String {
    format!("{name} has a {LOG_HEADING} that is not its last section")
}

fn extra_running_message(name: &str) -> String {
    format!("{name} has more than one running block")
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
    use chrono::NaiveDate;
    use punchcard_core::test_support::{FIXTURE_SETTINGS, ISO_MINUTE, for_each_fixture};
    use serde::Deserialize;

    use super::settings::{DAY_START_KEY, MIN_BLOCK_KEY, REJOIN_KEY, SETTINGS};
    use super::*;

    #[derive(Deserialize)]
    struct ExpectedToggle {
        now: String,
        toggle: String,
        after: String,
    }

    // Removes the vault even when an assert panics, so a failed test leaves nothing in the temp dir.
    pub(super) struct TempVault {
        pub(super) path: PathBuf,
    }

    impl TempVault {
        pub(super) fn new(test: &str) -> Self {
            let path = std::env::temp_dir().join(format!("stint-{test}-{}", std::process::id()));
            let vault = Self { path };
            fs::create_dir_all(vault.path.join("_Meta")).unwrap();

            let day_start = FIXTURE_SETTINGS.day_start_hour;
            let rejoin = FIXTURE_SETTINGS.block_rejoin_minutes;
            let min_block = FIXTURE_SETTINGS.block_min_minutes;
            let settings = format!(
                "---\n{DAY_START_KEY}: {day_start}\n{REJOIN_KEY}: {rejoin}\n{MIN_BLOCK_KEY}: {min_block}\n---\n"
            );
            fs::write(vault.path.join(SETTINGS), settings).unwrap();
            vault
        }
    }

    impl Drop for TempVault {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn test_settings() -> Settings {
        Settings {
            day_start: DayStart::new(FIXTURE_SETTINGS.day_start_hour).unwrap(),
            block_rejoin_minutes: FIXTURE_SETTINGS.block_rejoin_minutes,
            block_min_minutes: FIXTURE_SETTINGS.block_min_minutes,
        }
    }

    fn lines(text: &str) -> Vec<String> {
        text.split('\n').map(String::from).collect()
    }

    // Seconds past the minute, as Local::now() has, so every toggle test also covers wall_clock.
    fn now() -> NaiveDateTime {
        let date = NaiveDate::from_ymd_opt(2026, 11, 1).unwrap();
        wall_clock(date.and_hms_opt(11, 40, 30).unwrap())
    }

    fn toggled(text: &str) -> (Toggle, String) {
        let mut lines = lines(text);
        let toggle = toggle_block(&mut lines, now(), &test_settings(), "Port").unwrap();
        (toggle, lines.join("\n"))
    }

    fn rebuilt(text: &str) -> String {
        let mut lines = lines(text);
        rebuild_derived(&mut lines, test_settings().day_start, "Port").unwrap();
        lines.join("\n")
    }

    #[test]
    fn toggle_adds_log_at_end_of_body_when_missing() {
        let (toggle, text) = toggled("---\nstatus: Done\n---\nbody\n");

        assert_eq!(toggle, Toggle::Started);
        assert_eq!(
            text,
            "---\nstatus: Done\n---\nbody\n### Log\n#### 26/11/01 11:40 -\n"
        );
    }

    #[test]
    fn toggle_starts_block_at_top_of_log() {
        let (toggle, text) =
            toggled("---\n---\n### Log\n#### 26/10/31 09:00 - 26/10/31 10:00\nentry");

        assert_eq!(toggle, Toggle::Started);
        assert_eq!(
            text,
            "---\n---\n### Log\n#### 26/11/01 11:40 -\n#### 26/10/31 09:00 - 26/10/31 10:00\nentry"
        );
    }

    #[test]
    fn toggle_stops_running_block() {
        let (toggle, text) = toggled("---\n---\n### Log\n#### 26/11/01 09:00 -  \nentry");

        assert_eq!(toggle, Toggle::Stopped);
        assert_eq!(
            text,
            "---\n---\n### Log\n#### 26/11/01 09:00 - 26/11/01 11:40\nentry"
        );
    }

    #[test]
    fn toggle_starts_block_below_stray_log_text() {
        let (_, text) = toggled("---\n---\n### Log\nStray.\n#### 26/10/31 09:00 - 26/10/31 10:00");

        assert_eq!(
            text,
            "---\n---\n### Log\nStray.\n#### 26/11/01 11:40 -\n#### 26/10/31 09:00 - 26/10/31 10:00"
        );
    }

    #[test]
    fn toggle_starts_block_at_end_of_log_without_blocks() {
        let (_, text) = toggled("---\n---\n### Log\nStray.\n");

        assert_eq!(text, "---\n---\n### Log\nStray.\n#### 26/11/01 11:40 -\n");
    }

    #[test]
    fn toggle_finds_log_after_a_fenced_heading() {
        let (toggle, text) =
            toggled("---\n---\n### Log\n#### 26/11/01 09:00 -\n```sh\n# install\n```\n");

        assert_eq!(toggle, Toggle::Stopped);
        assert_eq!(
            text,
            "---\n---\n### Log\n#### 26/11/01 09:00 - 26/11/01 11:40\n```sh\n# install\n```\n"
        );
    }

    #[test]
    fn toggle_refuses_a_log_with_two_running_blocks() {
        let mut lines = lines("---\n---\n### Log\n#### 26/11/01 10:00 -\n#### 26/11/01 09:00 -");

        let error = toggle_block(&mut lines, now(), &test_settings(), "Port").unwrap_err();

        assert_eq!(
            error.to_string(),
            "Port has more than one running block, stop all but one by hand"
        );
    }

    #[test]
    fn toggle_refuses_to_stop_a_block_that_starts_after_now() {
        let mut lines = lines("---\n---\n### Log\n#### 26/11/01 11:41 -");

        let error = toggle_block(&mut lines, now(), &test_settings(), "Port").unwrap_err();

        assert_eq!(
            error.to_string(),
            "Port has a running block that starts after now: #### 26/11/01 11:41 -"
        );
    }

    #[test]
    fn toggle_discards_a_block_started_this_minute_with_an_empty_entry() {
        let (toggle, text) = toggled("---\n---\n### Log\n#### 26/11/01 11:40 -\n");

        assert_eq!(toggle, Toggle::Discarded);
        assert_eq!(text, "---\n---\n### Log\n");
    }

    #[test]
    fn toggle_refuses_to_stop_a_block_started_this_minute_with_an_entry() {
        let mut lines = lines("---\n---\n### Log\n#### 26/11/01 11:40 -\nentry");

        let error = toggle_block(&mut lines, now(), &test_settings(), "Port").unwrap_err();

        assert_eq!(
            error.to_string(),
            "Port has a running block with an entry that started this minute, stop it from the next minute on: #### 26/11/01 11:40 -"
        );
    }

    #[test]
    fn toggle_rejoins_a_block_that_ended_the_rejoin_limit_ago_part_way_through_a_minute() {
        let (toggle, text) = toggled("---\n---\n### Log\n#### 26/11/01 11:00 - 26/11/01 11:35");

        assert_eq!(toggle, Toggle::Resumed);
        assert_eq!(text, "---\n---\n### Log\n#### 26/11/01 11:00 -");
    }

    #[test]
    fn toggle_refuses_a_log_that_is_not_the_last_section() {
        let mut lines = lines("---\n---\n### Log\n#### 26/11/01 09:00 -\n### Notes\n");

        let error = toggle_block(&mut lines, now(), &test_settings(), "Port").unwrap_err();

        assert!(
            error
                .to_string()
                .starts_with("Port has a ### Log that is not its last section")
        );
    }

    #[test]
    fn toggle_fixtures_match_their_expected_json() {
        for_each_fixture("toggle", |path, text, expected: ExpectedToggle| {
            let mut lines = lines(text);
            let now = NaiveDateTime::parse_from_str(&expected.now, ISO_MINUTE).unwrap();
            let settings = test_settings();

            let toggle = toggle_block(&mut lines, now, &settings, "Port").unwrap();
            rebuild_derived(&mut lines, settings.day_start, "Port").unwrap();

            let name = path.display();
            assert_eq!(format!("{toggle:?}"), expected.toggle, "{name}");
            assert_eq!(lines.join("\n"), expected.after, "{name}");
        });
    }

    #[test]
    fn toggle_discards_a_short_block_whose_entry_is_only_blank_lines() {
        let (toggle, text) = toggled(
            "---\n---\n### Log\n#### 26/11/01 11:39 -\n\n#### 26/11/01 09:00 - 26/11/01 10:00",
        );

        assert_eq!(toggle, Toggle::Discarded);
        assert_eq!(
            text,
            "---\n---\n### Log\n#### 26/11/01 09:00 - 26/11/01 10:00"
        );
    }

    #[test]
    fn toggle_does_not_rejoin_a_block_that_ends_after_now() {
        let (toggle, _) = toggled("---\n---\n### Log\n#### 26/11/01 11:00 - 26/11/01 11:42");

        assert_eq!(toggle, Toggle::Started);
    }

    #[test]
    fn discarded_toggle_says_a_block_was_discarded() {
        assert!(Toggle::Discarded.to_string().contains("Block Discarded"));
    }

    #[test]
    fn sync_reports_bad_headings_and_extra_running_blocks() {
        let vault = TempVault::new("sync");
        let body =
            "---\n---\n### Log\n#### 26/11/01 10:00 -\n#### 26/11/01 09:00 -\n#### later -\n";
        fs::write(vault.path.join("Port.md"), body).unwrap();

        let synced = sync(&vault.path).unwrap();

        let heading = Synced::ReportedBadHeading {
            note: "Port".to_string(),
            heading: "#### later -".to_string(),
        };
        let running = Synced::ReportedExtraRunning {
            note: "Port".to_string(),
        };
        let rebuilt = Synced::Rebuilt {
            note: "Port".to_string(),
        };
        assert_eq!(synced, [heading, running, rebuilt]);
    }

    #[test]
    fn sync_reports_a_log_that_is_not_the_last_section() {
        let vault = TempVault::new("sync-buried");
        fs::write(vault.path.join("Port.md"), "---\n---\n### Log\n### Notes\n").unwrap();

        let synced = sync(&vault.path).unwrap();

        let buried = Synced::ReportedBuriedLog {
            note: "Port".to_string(),
        };
        assert_eq!(synced, [buried]);
    }

    #[test]
    fn sync_leaves_blocks_shorter_than_the_minimum() {
        let vault = TempVault::new("sync-short");
        let note = vault.path.join("Port.md");
        let text = "---\nworked_days:\n  - \"[[26-11-01]]\"\nworked_hours: 0.02\n---\n### Log\n#### 26/11/01 09:00 - 26/11/01 09:01\n";
        fs::write(&note, text).unwrap();

        assert_eq!(sync(&vault.path).unwrap(), []);
        assert_eq!(fs::read_to_string(&note).unwrap(), text);
    }

    #[test]
    fn rebuild_refuses_a_block_whose_day_falls_before_2000() {
        let mut lines = lines("---\n---\n### Log\n#### 00/01/01 00:30 - 00/01/01 01:00");

        let error = rebuild_derived(&mut lines, test_settings().day_start, "Port").unwrap_err();

        assert_eq!(
            error.to_string(),
            "Port has a block on 1999-12-31, which has no two-digit year"
        );
    }

    #[test]
    fn rebuild_replaces_days_and_hours_in_place() {
        let text = rebuilt(
            "---\nworked_days:\n  - \"[[26-10-30]]\"\nworked_hours: 9\ntags:\n---\n### Log\n#### 26/10/31 09:00 - 26/10/31 10:30",
        );

        assert_eq!(
            text,
            "---\nworked_days:\n  - \"[[26-10-31]]\"\nworked_hours: 1.5\ntags:\n---\n### Log\n#### 26/10/31 09:00 - 26/10/31 10:30"
        );
    }

    #[test]
    fn rebuild_appends_keys_when_missing() {
        let text = rebuilt("---\ntags:\n---\n### Log\n#### 26/10/31 09:00 - 26/10/31 10:00");

        assert!(
            text.starts_with(
                "---\ntags:\nworked_days:\n  - \"[[26-10-31]]\"\nworked_hours: 1\n---"
            )
        );
    }

    #[test]
    fn rebuild_removes_keys_when_log_is_empty() {
        let text = rebuilt("---\nworked_days:\n  - \"[[26-10-30]]\"\nworked_hours: 2\n---\nbody");

        assert_eq!(text, "---\n---\nbody");
    }

    #[test]
    fn rebuild_links_running_block_day_but_counts_no_hours() {
        let text = rebuilt("---\n---\n### Log\n#### 26/11/01 01:00 -");

        assert_eq!(
            text,
            "---\nworked_days:\n  - \"[[26-10-31]]\"\n---\n### Log\n#### 26/11/01 01:00 -"
        );
    }
}
