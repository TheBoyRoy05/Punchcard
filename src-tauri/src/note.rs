use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use punchcard_core::Log;
use serde::{Serialize, Serializer};

pub const DAYS_KEY: &str = "punchcard__worked_days";
pub const HOURS_KEY: &str = "punchcard__worked_hours";
pub const LIST_ITEM: &str = "  - ";
pub const TASK_TAG: &str = "Task";
const PROJECT_TAG: &str = "Project";
const NO_DUE: &str = "9999";
// Same escaping as Python's urllib.parse.quote, so links match the ones task-timer sent.
const URL_PATH: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'_')
    .remove(b'.')
    .remove(b'-')
    .remove(b'~')
    .remove(b'/');

// Perhaps Field enum in the future if we read/write scaler values
pub type Fields = HashMap<String, Vec<String>>;

#[derive(Debug, Default)]
pub struct Frontmatter {
    pub fields: Fields,
    pub skipped: Vec<SkippedLine>,
}

// A frontmatter line the parser could not read, under the key it follows, if any.
#[derive(Debug, PartialEq)]
pub struct SkippedLine {
    pub key: Option<String>,
    pub line: String,
}

#[derive(Debug)]
pub struct Note {
    pub path: PathBuf,
    pub frontmatter: Frontmatter,
    pub log: Log,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    NotStarted,
    InProgress,
    InReview,
    Paused,
    Completed,
    Dropped,
}

impl Note {
    pub fn from_lines(path: PathBuf, lines: &[String]) -> Option<Self> {
        let frontmatter = parse_frontmatter(lines)?;
        let log = Log::parse(lines);
        Some(Self {
            path,
            frontmatter,
            log,
        })
    }

    pub fn name(&self) -> &str {
        note_name(&self.path)
    }

    pub fn url(&self) -> String {
        let path = self.path.to_string_lossy();
        let path = utf8_percent_encode(&path, URL_PATH);
        format!("obsidian://open?path={path}")
    }

    pub fn list(&self, key: &str) -> &[String] {
        self.frontmatter.fields.get(key).map_or(&[], Vec::as_slice)
    }

    pub fn is_open(&self) -> bool {
        let timed = self
            .list("tags")
            .iter()
            .any(|tag| tag_is_within(tag, TASK_TAG) || tag_is_within(tag, PROJECT_TAG));
        timed && self.scalar("status").parse().is_ok_and(Status::is_open)
    }

    pub fn due(&self) -> &str {
        match self.scalar("due") {
            "" => NO_DUE,
            due => due,
        }
    }

    fn scalar(&self, key: &str) -> &str {
        self.list(key).first().map_or("", String::as_str)
    }
}

impl Status {
    pub const ALL: [Self; 6] = [
        Self::NotStarted,
        Self::InProgress,
        Self::InReview,
        Self::Paused,
        Self::Completed,
        Self::Dropped,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotStarted => "Not Started",
            Self::InProgress => "In Progress",
            Self::InReview => "In Review",
            Self::Paused => "Paused",
            Self::Completed => "Completed",
            Self::Dropped => "Dropped",
        }
    }

    pub fn is_open(self) -> bool {
        matches!(self, Self::NotStarted | Self::InProgress | Self::InReview)
    }

    pub fn is_listed(self) -> bool {
        matches!(self, Self::NotStarted | Self::InProgress)
    }

    pub fn is_finished(self) -> bool {
        matches!(self, Self::Completed | Self::Dropped)
    }
}

impl FromStr for Status {
    type Err = ();

    fn from_str(status: &str) -> Result<Self, ()> {
        Self::ALL
            .into_iter()
            .find(|known| known.as_str() == status)
            .ok_or(())
    }
}

impl Serialize for Status {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

pub fn parse_frontmatter(lines: &[String]) -> Option<Frontmatter> {
    if lines.first()? != "---" {
        return None;
    }

    let mut frontmatter = Frontmatter::default();
    let mut key: Option<&str> = None;
    for line in &lines[1..] {
        if line == "---" {
            return Some(frontmatter);
        }

        if let (Some(item), Some(key)) = (line.strip_prefix(LIST_ITEM), key) {
            frontmatter
                .fields
                .entry(key.to_string())
                .or_default()
                .push(unquote(item));
        } else if let Some((name, value)) = key_value(line) {
            let value = unquote(value);
            let values = if value.is_empty() {
                Vec::new()
            } else {
                vec![value]
            };
            frontmatter.fields.insert(name.to_string(), values);
            key = Some(name);
        } else if !line.trim().is_empty() {
            // An unread key like `Related Notes:` ends the previous key, so its items don't join it.
            if !line.starts_with([' ', '-']) {
                key = None;
            }
            frontmatter.skipped.push(SkippedLine {
                key: key.map(String::from),
                line: line.clone(),
            });
        }
    }
    None
}

// Only identifier keys are read, so an unindented `- 26/10/30 09:00 - ...` item is not read as one.
fn key_value(line: &str) -> Option<(&str, &str)> {
    let (name, value) = line.split_once(':')?;
    let mut chars = name.chars();
    let first = chars.next()?;
    let valid = (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|char| char.is_alphanumeric() || char == '_' || char == '-');
    valid.then_some((name, value))
}

pub fn note_name(path: &Path) -> &str {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default()
}

// Tags may be written with or without their leading #.
pub fn tag_is_within(tag: &str, parent: &str) -> bool {
    let tag = tag.trim_start_matches('#');
    let parent = parent.trim_start_matches('#');
    tag.strip_prefix(parent)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

fn unquote(value: &str) -> String {
    value.trim().trim_matches('"').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(text: &str) -> Vec<String> {
        text.split('\n').map(String::from).collect()
    }

    fn note(text: &str) -> Note {
        Note::from_lines(PathBuf::from("Tasks/Port.md"), &lines(text)).unwrap()
    }

    #[test]
    fn parses_scalars_and_lists_without_quotes() {
        let fields = parse_frontmatter(&lines(
            "---\nstatus: \"In Progress\"\ntags:\n  - \"#Task\"\n---\nbody",
        ))
        .unwrap()
        .fields;

        assert_eq!(fields["status"], ["In Progress"]);
        assert_eq!(fields["tags"], ["#Task"]);
    }

    #[test]
    fn empty_value_starts_an_empty_list() {
        let fields = parse_frontmatter(&lines("---\npunchcard__worked_days:\n---"))
            .unwrap()
            .fields;

        assert!(fields["punchcard__worked_days"].is_empty());
    }

    #[test]
    fn unreadable_lines_are_skipped_under_their_key_and_blank_lines_dropped() {
        let frontmatter = parse_frontmatter(&lines(
            "---\nloose\nworked_on:\n- 26/11/03 10:00 - 26/11/03 11:00\n\n---",
        ))
        .unwrap();

        assert!(frontmatter.fields["worked_on"].is_empty());
        assert_eq!(
            frontmatter.skipped,
            [
                SkippedLine {
                    key: None,
                    line: "loose".to_string(),
                },
                SkippedLine {
                    key: Some("worked_on".to_string()),
                    line: "- 26/11/03 10:00 - 26/11/03 11:00".to_string(),
                },
            ]
        );
    }

    #[test]
    fn unread_key_ends_the_previous_key() {
        let frontmatter = parse_frontmatter(&lines(
            "---\ntags:\n  - \"#Task\"\nRelated Notes:\n  - \"[[A]]\"\n---",
        ))
        .unwrap();

        assert_eq!(frontmatter.fields["tags"], ["#Task"]);
        assert_eq!(frontmatter.skipped[1].key, None);
    }

    #[test]
    fn note_without_closed_frontmatter_has_no_fields() {
        assert!(parse_frontmatter(&lines("no frontmatter")).is_none());
        assert!(parse_frontmatter(&lines("---\nstatus: Done")).is_none());
    }

    #[test]
    fn name_is_file_stem() {
        assert_eq!(note("---\n---").name(), "Port");
    }

    #[test]
    fn status_parses_its_vault_string_and_nothing_else() {
        for status in Status::ALL {
            assert_eq!(status.as_str().parse(), Ok(status));
        }
        assert_eq!("Someday".parse::<Status>(), Err(()));
    }

    #[test]
    fn unknown_status_is_not_open() {
        let note = note("---\nstatus: Someday\ntags:\n  - \"#Task\"\n---");

        assert!(!note.is_open());
    }

    #[test]
    fn url_escapes_path_like_python_quote() {
        let note = Note {
            path: PathBuf::from("/Vault/Tasks/Port Task_v1.0-~é.md"),
            frontmatter: Frontmatter::default(),
            log: Log::default(),
        };

        assert_eq!(
            note.url(),
            "obsidian://open?path=/Vault/Tasks/Port%20Task_v1.0-~%C3%A9.md"
        );
    }
}
