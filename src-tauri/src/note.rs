use std::collections::HashMap;
use std::path::PathBuf;

use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};

pub const SPANS_KEY: &str = "worked_on";
pub const DAYS_KEY: &str = "worked_days";
pub const LIST_ITEM: &str = "  - ";
// Same escaping as Python's urllib.parse.quote, so links match the ones task-timer sent.
const URL_PATH: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'_')
    .remove(b'.')
    .remove(b'-')
    .remove(b'~')
    .remove(b'/');

// Perhaps Field enum in the future if we read/write scaler values
pub type Fields = HashMap<String, Vec<String>>;

pub struct Note {
    pub path: PathBuf,
    pub fields: Fields,
}

impl Note {
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

    pub fn days(&self) -> Vec<String> {
        let mut days = Vec::new();
        for span in self.list(SPANS_KEY) {
            let date = span.split_once(' ').map_or(span.as_str(), |(date, _)| date);
            let day = format!("[[{}]]", date.replace('/', "-"));
            if !days.contains(&day) {
                days.push(day);
            }
        }
        days
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
        let fields = parse_frontmatter(&lines(text)).unwrap();
        Note {
            path: PathBuf::from("Tasks/Port.md"),
            fields,
        }
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
        let fields = parse_frontmatter(&lines("---\nworked_on:\n---")).unwrap();

        assert!(fields["worked_on"].is_empty());
    }

    #[test]
    fn note_without_closed_frontmatter_has_no_fields() {
        assert!(parse_frontmatter(&lines("no frontmatter")).is_none());
        assert!(parse_frontmatter(&lines("---\nstatus: Done")).is_none());
    }

    #[test]
    fn days_link_each_start_date_once_in_order() {
        let note = note(
            "---\nworked_on:\n  - 26/10/30 23:33 - 26/10/31 00:40\n  - 26/10/31 09:00 -\n  - 26/10/30 08:00 - 26/10/30 09:00\n---",
        );

        assert_eq!(note.days(), ["[[26-10-30]]", "[[26-10-31]]"]);
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
        };

        assert_eq!(
            note.url(),
            "obsidian://open?path=/Vault/Tasks/Port%20Task_v1.0-~%C3%A9.md"
        );
    }
}
