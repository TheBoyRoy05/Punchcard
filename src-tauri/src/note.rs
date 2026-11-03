use std::collections::HashMap;
use std::path::PathBuf;

use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use punchcard_core::Log;

pub const DAYS_KEY: &str = "worked_days";
pub const HOURS_KEY: &str = "worked_hours";
pub const LIST_ITEM: &str = "  - ";
const OPEN_STATUSES: [&str; 3] = ["Not Started", "In Progress", "In Review"];
const TIMED_TYPES: [&str; 2] = ["Task", "Project"];
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

#[derive(Debug)]
pub struct Note {
    pub path: PathBuf,
    pub fields: Fields,
    pub log: Log,
}

impl Note {
    pub fn from_lines(path: PathBuf, lines: &[String]) -> Option<Self> {
        let fields = parse_frontmatter(lines)?;
        let log = Log::parse(lines);
        Some(Self { path, fields, log })
    }

    pub fn name(&self) -> &str {
        self.path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or_default()
    }

    pub fn url(&self) -> String {
        let path = self.path.to_string_lossy();
        let path = utf8_percent_encode(&path, URL_PATH);
        format!("obsidian://open?path={path}")
    }

    pub fn list(&self, key: &str) -> &[String] {
        self.fields.get(key).map_or(&[], Vec::as_slice)
    }

    pub fn is_open(&self) -> bool {
        let timed = self.list("tags").iter().any(|tag| {
            let tag = tag.trim_start_matches('#');
            TIMED_TYPES.iter().any(|kind| {
                tag.strip_prefix(kind)
                    .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
            })
        });
        timed && OPEN_STATUSES.contains(&self.scalar("status"))
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

pub fn parse_frontmatter(lines: &[String]) -> Option<Fields> {
    if lines.first()? != "---" {
        return None;
    }

    let mut fields = Fields::new();
    let mut key: Option<&str> = None;
    for line in &lines[1..] {
        if line == "---" {
            return Some(fields);
        }

        if let (Some(item), Some(key)) = (line.strip_prefix(LIST_ITEM), key) {
            fields
                .entry(key.to_string())
                .or_default()
                .push(unquote(item));
        } else if !line.starts_with(' ')
            && let Some((name, value)) = line.split_once(':')
        {
            let value = unquote(value);
            let values = if value.is_empty() {
                Vec::new()
            } else {
                vec![value]
            };
            fields.insert(name.to_string(), values);
            key = Some(name);
        }
    }
    None
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
        .unwrap();

        assert_eq!(fields["status"], ["In Progress"]);
        assert_eq!(fields["tags"], ["#Task"]);
    }

    #[test]
    fn empty_value_starts_an_empty_list() {
        let fields = parse_frontmatter(&lines("---\nworked_days:\n---")).unwrap();

        assert!(fields["worked_days"].is_empty());
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
    fn url_escapes_path_like_python_quote() {
        let note = Note {
            path: PathBuf::from("/Vault/Tasks/Port Task_v1.0-~é.md"),
            fields: Fields::new(),
            log: Log::default(),
        };

        assert_eq!(
            note.url(),
            "obsidian://open?path=/Vault/Tasks/Port%20Task_v1.0-~%C3%A9.md"
        );
    }
}
