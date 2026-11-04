use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime};
use serde::{Serialize, Serializer};

use super::{TimerError, read_lines};
use crate::note::{Note, Status, TASK_TAG, note_name, tag_is_within};

const TASKS_DIR: &str = "Tasks";
const ESTIMATE_KEY: &str = "agent__estimate_hours";
const BLOCKED_BY_KEY: &str = "blocked_by";
const AUTHORSHIP_TAG: &str = "AI/Agent/Written";
// Notes keep times to the minute, so printing one minute format drops nothing.
const DUE_FORMAT: &str = "%Y-%m-%dT%H:%M";
const DUE_FORMATS: [&str; 4] = [
    "%Y-%m-%dT%H:%M:%S%.f",
    "%Y-%m-%dT%H:%M",
    "%Y-%m-%d %H:%M:%S%.f",
    "%Y-%m-%d %H:%M",
];

#[derive(Serialize)]
struct Task {
    name: String,
    path: PathBuf,
    status: Status,
    #[serde(serialize_with = "serialize_due")]
    due: Option<NaiveDateTime>,
    tags: Vec<String>,
    estimate_hours: Option<f64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    blocked_by: Vec<String>,
}

pub struct Listing {
    pub lines: Vec<String>,
    pub skipped: Vec<Skipped>,
}

pub struct Skipped {
    pub path: PathBuf,
    pub reason: String,
}

struct Malformed(String);

impl fmt::Display for Skipped {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(
            formatter,
            "skipped {}: {}",
            self.path.display(),
            self.reason
        )
    }
}

pub fn tasks(vault: &Path, tag: Option<&str>) -> Result<Listing, TimerError> {
    let mut tasks = Vec::new();
    let mut skipped = Vec::new();
    for path in task_paths(&vault.join(TASKS_DIR))? {
        match read_task(&path) {
            Ok(task) => tasks.push(task),
            Err(Malformed(reason)) => skipped.push(Skipped { path, reason }),
        }
    }

    let malformed: HashSet<&str> = skipped.iter().map(|note| note_name(&note.path)).collect();
    let listed = listed(tasks, tag, &malformed);
    let lines = listed.iter().map(|task| {
        serde_json::to_string(task).expect("UTF-8 paths, strings and finite numbers serialize")
    });
    let lines = lines.collect();

    Ok(Listing { lines, skipped })
}

fn task_paths(dir: &Path) -> Result<Vec<PathBuf>, TimerError> {
    if !dir.is_dir() {
        return Err(TimerError(format!("no task folder at {}", dir.display())));
    }

    let entries = fs::read_dir(dir)?.map(|entry| entry.map(|entry| entry.path()));
    let mut paths: Vec<PathBuf> = entries.collect::<io::Result<_>>()?;
    paths.retain(|path| {
        path.extension().is_some_and(|extension| extension == "md")
            && !note_name(path).starts_with('.')
    });
    paths.sort();
    Ok(paths)
}

fn read_task(path: &Path) -> Result<Task, Malformed> {
    // JSON strings are UTF-8, so serializing the path would fail later.
    if path.to_str().is_none() {
        return Err(Malformed("path is not UTF-8".to_string()));
    }

    let lines = read_lines(path).map_err(|error| Malformed(format!("cannot read: {error}")))?;
    let note = Note::from_lines(path.to_path_buf(), &lines)
        .ok_or_else(|| Malformed("no frontmatter between --- lines".to_string()))?;
    Task::read(note)
}

impl Task {
    fn read(note: Note) -> Result<Self, Malformed> {
        let tags = field(&note, "tags")?;
        if !tags.iter().any(|tag| tag_is_within(tag, TASK_TAG)) {
            return Err(Malformed(format!("no \"#{TASK_TAG}\" tag")));
        }
        let tags = tags
            .iter()
            .filter(|tag| !tag_is_within(tag, TASK_TAG) && !tag_is_within(tag, AUTHORSHIP_TAG));
        let tags = tags.cloned().collect();

        let status = single(&note, "status")?;
        let status = status.parse().map_err(|()| {
            let known = Status::ALL.map(Status::as_str);
            Malformed(format!("status {status:?} is not one of {known:?}"))
        })?;

        let due = due_time(single(&note, "due")?)?;
        let estimate_hours = estimate(single(&note, ESTIMATE_KEY)?)?;
        let blocked_by = field(&note, BLOCKED_BY_KEY)?;
        let blocked_by = blocked_by.iter().map(|link| link_target(link));
        let blocked_by = blocked_by.collect::<Result<_, _>>()?;

        Ok(Self {
            name: note.name().to_string(),
            path: note.path,
            status,
            due,
            tags,
            estimate_hours,
            blocked_by,
        })
    }

    fn has_tag(&self, tag: Option<&str>) -> bool {
        tag.is_none_or(|tag| self.tags.iter().any(|shown| tag_is_within(shown, tag)))
    }
}

fn single<'a>(note: &'a Note, key: &str) -> Result<&'a str, Malformed> {
    match field(note, key)? {
        [] => Ok(""),
        [value] => Ok(value),
        values => {
            let message = format!("{key} has {} values, expected one", values.len());
            Err(Malformed(message))
        }
    }
}

// A skipped line under a read key would silently drop a tag or a blocker; unread fields may hold any YAML.
fn field<'a>(note: &'a Note, key: &str) -> Result<&'a [String], Malformed> {
    let mut skipped = note.frontmatter.skipped.iter();
    if let Some(skipped) = skipped.find(|skipped| skipped.key.as_deref() == Some(key)) {
        let message = format!(
            "cannot parse frontmatter line {:?} under {key}",
            skipped.line
        );
        return Err(Malformed(message));
    }

    Ok(note.list(key))
}

fn estimate(value: &str) -> Result<Option<f64>, Malformed> {
    if value.is_empty() {
        return Ok(None);
    }

    match value.parse::<f64>() {
        Ok(hours) if hours.is_finite() => Ok(Some(hours)),
        _ => Err(Malformed(format!(
            "{ESTIMATE_KEY} {value:?} is not a number"
        ))),
    }
}

fn due_time(due: &str) -> Result<Option<NaiveDateTime>, Malformed> {
    if due.is_empty() {
        return Ok(None);
    }

    let time = DUE_FORMATS
        .iter()
        .find_map(|format| NaiveDateTime::parse_from_str(due, format).ok());
    let day = || NaiveDate::parse_from_str(due, "%Y-%m-%d").ok();
    if let Some(time) = time.or_else(|| Some(day()?.and_time(NaiveTime::MIN))) {
        return Ok(Some(time));
    }

    // Dues sort against each other, and the vault writes them in local time, so an offset is a mistake.
    let problem = if DateTime::parse_from_rfc3339(due).is_ok() {
        "has a UTC offset; write local time without one"
    } else {
        "is not an ISO datetime"
    };
    Err(Malformed(format!("due {due:?} {problem}")))
}

fn link_target(link: &str) -> Result<String, Malformed> {
    let inner = link
        .strip_prefix("[[")
        .and_then(|link| link.strip_suffix("]]"))
        .filter(|inner| !inner.is_empty() && !inner.contains(']'));
    let Some(inner) = inner else {
        let message = format!("{BLOCKED_BY_KEY} entry {link:?} is not a [[Task Name]] link");
        return Err(Malformed(message));
    };

    // Obsidian writes [[Folder/Name|alias]] on rename or autocomplete; the note name sits between them.
    let target = inner.split_once('|').map_or(inner, |(target, _)| target);
    let name = target.rsplit_once('/').map_or(target, |(_, name)| name);
    Ok(name.to_string())
}

fn listed(mut tasks: Vec<Task>, tag: Option<&str>, malformed: &HashSet<&str>) -> Vec<Task> {
    let statuses: HashMap<String, Status> = tasks
        .iter()
        .map(|task| (task.name.clone(), task.status))
        .collect();

    tasks.retain(|task| task.status.is_listed() && task.has_tag(tag));
    tasks.sort_by(|a, b| {
        let undated = a.due.is_none().cmp(&b.due.is_none());
        undated
            .then(a.due.cmp(&b.due))
            .then_with(|| a.name.cmp(&b.name))
    });

    for task in &mut tasks {
        task.blocked_by = unfinished_blockers(&task.blocked_by, &statuses, malformed);
    }
    tasks
}

fn unfinished_blockers(
    names: &[String],
    statuses: &HashMap<String, Status>,
    malformed: &HashSet<&str>,
) -> Vec<String> {
    let unfinished = names.iter().filter_map(|name| {
        let state = match statuses.get(name) {
            Some(status) if status.is_finished() => return None,
            Some(status) => status.as_str(),
            None if malformed.contains(name.as_str()) => "malformed",
            None => "missing",
        };
        Some(format!("{name} ({state})"))
    });
    unfinished.collect()
}

fn serialize_due<S: Serializer>(
    due: &Option<NaiveDateTime>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match due {
        Some(due) => serializer.collect_str(&due.format(DUE_FORMAT)),
        None => serializer.serialize_none(),
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::symlink;

    use serde_json::{Value, json};

    use super::super::tests::TempVault;
    use super::*;

    struct NoteSpec<'a> {
        status: &'a str,
        tags: &'a [&'a str],
        due: &'a str,
        estimate: &'a str,
        blocked_by: &'a [&'a str],
    }

    const DEFAULT: NoteSpec = NoteSpec {
        status: "Not Started",
        tags: &["#Task"],
        due: "",
        estimate: "",
        blocked_by: &[],
    };

    impl NoteSpec<'_> {
        fn text(&self) -> String {
            let mut lines = vec![
                "---".to_string(),
                format!("status: {}", self.status),
                "tags:".to_string(),
            ];
            lines.extend(self.tags.iter().map(|tag| format!("  - \"{tag}\"")));
            lines.push(format!("due: {}", self.due));
            if !self.estimate.is_empty() {
                lines.push(format!("{ESTIMATE_KEY}: {}", self.estimate));
            }
            if !self.blocked_by.is_empty() {
                lines.push(format!("{BLOCKED_BY_KEY}:"));
                lines.extend(
                    self.blocked_by
                        .iter()
                        .map(|name| format!("  - \"[[{name}]]\"")),
                );
            }
            lines.extend(["---", "", "Body text."].map(String::from));
            lines.join("\n") + "\n"
        }
    }

    struct TaskVault {
        vault: TempVault,
    }

    impl TaskVault {
        fn new(test: &str) -> Self {
            let vault = TempVault::new(&format!("tasks-{test}"));
            fs::create_dir_all(vault.path.join(TASKS_DIR)).unwrap();
            Self { vault }
        }

        fn path(&self, name: &str) -> PathBuf {
            self.vault.path.join(TASKS_DIR).join(format!("{name}.md"))
        }

        fn write(&self, name: &str, text: &str) {
            fs::write(self.path(name), text).unwrap();
        }

        fn note(&self, name: &str, spec: NoteSpec) {
            self.write(name, &spec.text());
        }

        fn listing(&self, tag: Option<&str>) -> Listing {
            tasks(&self.vault.path, tag).unwrap()
        }

        fn lines(&self, tag: Option<&str>) -> Vec<Value> {
            let lines = self.listing(tag).lines;
            let lines = lines.iter().map(|line| serde_json::from_str(line).unwrap());
            lines.collect()
        }

        fn names(&self, tag: Option<&str>) -> Vec<String> {
            let lines = self.lines(tag);
            lines.iter().map(|line| text(line, "name")).collect()
        }

        fn only_task(&self) -> Value {
            let mut lines = self.lines(None);
            assert_eq!(lines.len(), 1, "{lines:?}");
            lines.remove(0)
        }

        fn skipped_reasons(&self) -> Vec<(String, String)> {
            let skipped = self.listing(None).skipped.into_iter();
            skipped
                .map(|note| (note_name(&note.path).to_string(), note.reason))
                .collect()
        }
    }

    fn text(line: &Value, key: &str) -> String {
        line[key].as_str().unwrap().to_string()
    }

    #[test]
    fn keeps_only_not_started_and_in_progress() {
        let vault = TaskVault::new("status");
        for status in Status::ALL.map(Status::as_str) {
            vault.note(status, NoteSpec { status, ..DEFAULT });
        }

        assert_eq!(vault.names(None), ["In Progress", "Not Started"]);
    }

    #[test]
    fn sorts_by_due_then_name_with_undated_last() {
        let vault = TaskVault::new("sort");
        vault.note("Undated", DEFAULT);
        let later = "2026-11-20T23:59:00";
        let sooner = "2026-11-06T23:59:00";
        vault.note(
            "Later",
            NoteSpec {
                due: later,
                ..DEFAULT
            },
        );
        vault.note(
            "Sooner B",
            NoteSpec {
                due: sooner,
                ..DEFAULT
            },
        );
        vault.note(
            "Sooner A",
            NoteSpec {
                due: sooner,
                ..DEFAULT
            },
        );
        vault.note(
            "Day Only",
            NoteSpec {
                due: "2026-11-10",
                ..DEFAULT
            },
        );
        vault.note(
            "Space Minutes",
            NoteSpec {
                due: "2026-11-08 09:00",
                ..DEFAULT
            },
        );
        vault.note(
            "Space Seconds",
            NoteSpec {
                due: "2026-11-07 09:00:30.5",
                ..DEFAULT
            },
        );

        assert_eq!(
            vault.names(None),
            [
                "Sooner A",
                "Sooner B",
                "Space Seconds",
                "Space Minutes",
                "Day Only",
                "Later",
                "Undated"
            ]
        );
    }

    #[test]
    fn a_finished_blocker_does_not_block() {
        let vault = TaskVault::new("finished-blocker");
        vault.note(
            "Done",
            NoteSpec {
                status: "Completed",
                ..DEFAULT
            },
        );
        vault.note(
            "Gave Up",
            NoteSpec {
                status: "Dropped",
                ..DEFAULT
            },
        );
        let blocked_by = &["Done", "Gave Up"];
        vault.note(
            "Free",
            NoteSpec {
                blocked_by,
                ..DEFAULT
            },
        );

        assert!(vault.only_task().get("blocked_by").is_none());
    }

    #[test]
    fn an_open_blocker_is_reported_on_the_still_listed_task() {
        let vault = TaskVault::new("open-blocker");
        vault.note(
            "Review",
            NoteSpec {
                status: "In Review",
                ..DEFAULT
            },
        );
        vault.note(
            "Waiting",
            NoteSpec {
                blocked_by: &["Review"],
                ..DEFAULT
            },
        );

        let task = vault.only_task();

        assert_eq!(task["name"], "Waiting");
        assert_eq!(task["blocked_by"], json!(["Review (In Review)"]));
    }

    #[test]
    fn a_missing_blocker_is_reported() {
        let vault = TaskVault::new("missing-blocker");
        vault.note(
            "Waiting",
            NoteSpec {
                blocked_by: &["Ghost"],
                ..DEFAULT
            },
        );

        assert_eq!(vault.only_task()["blocked_by"], json!(["Ghost (missing)"]));
    }

    #[test]
    fn an_aliased_or_folder_link_names_the_note() {
        let vault = TaskVault::new("alias-blocker");
        vault.note(
            "Done",
            NoteSpec {
                status: "Completed",
                ..DEFAULT
            },
        );
        let blocked_by = &["Done|the first step", "Tasks/Done"];
        vault.note(
            "Free",
            NoteSpec {
                blocked_by,
                ..DEFAULT
            },
        );

        assert!(vault.only_task().get("blocked_by").is_none());
    }

    #[test]
    fn a_malformed_blocker_is_reported() {
        let vault = TaskVault::new("malformed-blocker");
        vault.write("Broken", "no frontmatter\n");
        vault.note(
            "Waiting",
            NoteSpec {
                blocked_by: &["Broken"],
                ..DEFAULT
            },
        );

        assert_eq!(
            vault.only_task()["blocked_by"],
            json!(["Broken (malformed)"])
        );
    }

    #[test]
    fn a_blocker_that_is_not_a_link_is_skipped() {
        let vault = TaskVault::new("bad-link");
        let text = DEFAULT
            .text()
            .replace("---\n\n", "blocked_by:\n  - Done\n---\n\n");
        vault.write("Task", &text);

        let reason = "blocked_by entry \"Done\" is not a [[Task Name]] link";
        assert_eq!(vault.skipped_reasons(), [("Task".into(), reason.into())]);
    }

    #[test]
    fn tags_leave_out_task_and_authorship_tags() {
        let vault = TaskVault::new("shown-tags");
        let tags = &[
            "#Task",
            "#Config",
            "#AI/Agent/Written/Partial",
            "#AI/Agent/Written",
        ];
        vault.note("Task", NoteSpec { tags, ..DEFAULT });

        assert_eq!(vault.only_task()["tags"], json!(["#Config"]));
    }

    #[test]
    fn a_child_task_tag_marks_a_task_and_is_left_out_of_tags() {
        let vault = TaskVault::new("child-task-tag");
        let tags = &["#Task/Chore", "#Config"];
        vault.note("Chore", NoteSpec { tags, ..DEFAULT });

        let task = vault.only_task();

        assert_eq!(task["name"], "Chore");
        assert_eq!(task["tags"], json!(["#Config"]));
    }

    #[test]
    fn tag_filter_matches_the_tag_and_its_children_only() {
        let vault = TaskVault::new("tag-filter");
        vault.note(
            "Root",
            NoteSpec {
                tags: &["#Task", "#KnowledgeAnchor"],
                ..DEFAULT
            },
        );
        let child = &["#Task", "#KnowledgeAnchor/Goal/Exit"];
        vault.note(
            "Child",
            NoteSpec {
                tags: child,
                ..DEFAULT
            },
        );
        let lookalike = &["#Task", "#KnowledgeAnchorage"];
        vault.note(
            "Lookalike",
            NoteSpec {
                tags: lookalike,
                ..DEFAULT
            },
        );
        vault.note(
            "Other",
            NoteSpec {
                tags: &["#Task", "#Config"],
                ..DEFAULT
            },
        );

        assert_eq!(vault.names(Some("KnowledgeAnchor")), ["Child", "Root"]);
    }

    #[test]
    fn tag_filter_matches_tags_written_without_a_hash() {
        let vault = TaskVault::new("tag-no-hash");
        vault.note(
            "Class",
            NoteSpec {
                tags: &["#Task", "UC/SD/COGS/101/C"],
                ..DEFAULT
            },
        );

        assert_eq!(vault.names(Some("#UC/SD")), ["Class"]);
    }

    #[test]
    fn a_malformed_note_is_skipped_and_the_rest_still_listed() {
        let vault = TaskVault::new("malformed");
        vault.note("Good", DEFAULT);
        vault.write("No Frontmatter", "just text\n");
        vault.note(
            "No Task Tag",
            NoteSpec {
                tags: &["#Idea"],
                ..DEFAULT
            },
        );
        vault.note(
            "Bad Status",
            NoteSpec {
                status: "Someday",
                ..DEFAULT
            },
        );
        vault.note(
            "Bad Estimate",
            NoteSpec {
                estimate: "lots",
                ..DEFAULT
            },
        );
        vault.note(
            "Infinite Estimate",
            NoteSpec {
                estimate: "inf",
                ..DEFAULT
            },
        );
        vault.note(
            "NaN Estimate",
            NoteSpec {
                estimate: "NaN",
                ..DEFAULT
            },
        );
        vault.note(
            "Bad Due",
            NoteSpec {
                due: "next week",
                ..DEFAULT
            },
        );
        fs::write(vault.path("Not UTF-8"), b"---\nstatus: \xff\n---\n").unwrap();
        symlink(vault.path("nowhere"), vault.path("Dangling")).unwrap();

        assert_eq!(vault.names(None), ["Good"]);
        let reasons = vault.skipped_reasons();
        let skipped: Vec<&str> = reasons.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(
            skipped,
            [
                "Bad Due",
                "Bad Estimate",
                "Bad Status",
                "Dangling",
                "Infinite Estimate",
                "NaN Estimate",
                "No Frontmatter",
                "No Task Tag",
                "Not UTF-8",
            ]
        );
    }

    #[test]
    fn a_due_with_a_utc_offset_is_skipped_and_the_rest_still_sort() {
        let vault = TaskVault::new("offset-due");
        vault.note(
            "Offset",
            NoteSpec {
                due: "2026-11-06T23:59:00-08:00",
                ..DEFAULT
            },
        );
        vault.note(
            "Local",
            NoteSpec {
                due: "2026-11-06T23:59:00",
                ..DEFAULT
            },
        );
        vault.note("Undated", DEFAULT);

        assert_eq!(vault.names(None), ["Local", "Undated"]);
        let reason =
            "due \"2026-11-06T23:59:00-08:00\" has a UTC offset; write local time without one";
        assert_eq!(vault.skipped_reasons(), [("Offset".into(), reason.into())]);
    }

    #[test]
    fn a_scalar_with_two_values_is_skipped() {
        let vault = TaskVault::new("two-statuses");
        let text = DEFAULT.text().replace(
            "status: Not Started",
            "status:\n  - Not Started\n  - Paused",
        );
        vault.write("Task", &text);

        let reason = "status has 2 values, expected one";
        assert_eq!(vault.skipped_reasons(), [("Task".into(), reason.into())]);
    }

    #[test]
    fn an_unindented_blocker_is_skipped_rather_than_dropped() {
        let vault = TaskVault::new("unindented-blocker");
        let text = DEFAULT
            .text()
            .replace("---\n\n", "blocked_by:\n- \"[[Done]]\"\n---\n\n");
        vault.write("Task", &text);

        let reason = "cannot parse frontmatter line \"- \\\"[[Done]]\\\"\" under blocked_by";
        assert_eq!(vault.skipped_reasons(), [("Task".into(), reason.into())]);
    }

    #[test]
    fn an_unparsed_line_under_tags_or_a_scalar_is_skipped() {
        for key in ["tags", "status", "due", ESTIMATE_KEY] {
            let vault = TaskVault::new(&format!("unparsed-{key}"));
            let spec = NoteSpec {
                estimate: "1",
                ..DEFAULT
            };
            let prefix = format!("{key}:");
            let text = spec.text();
            let lines = text.lines().flat_map(|line| {
                let unparsed = line.starts_with(&prefix).then_some("- \"x\"");
                std::iter::once(line).chain(unparsed)
            });
            let text = lines.collect::<Vec<_>>().join("\n") + "\n";
            vault.write("Task", &text);

            let reason = format!("cannot parse frontmatter line \"- \\\"x\\\"\" under {key}");
            assert_eq!(vault.skipped_reasons(), [("Task".into(), reason)]);
        }
    }

    #[test]
    fn spans_and_the_log_are_not_read() {
        let vault = TaskVault::new("spans-unread");
        let spans = "worked_on:\n- 2026-11-03 10:00 -\n---\n\n";
        let buried_log = "### Log\n#### yesterday -\n### Notes\n";
        let text = DEFAULT.text().replace("---\n\n", spans) + buried_log;
        vault.write("Task", &text);

        assert_eq!(vault.names(None), ["Task"]);
    }

    #[test]
    fn unparsed_yaml_in_an_unread_field_or_a_blank_line_is_ignored() {
        let vault = TaskVault::new("unread-yaml");
        let text = DEFAULT
            .text()
            .replace("---\n\n", "\nnotes: |\n  free text\n---\n\n");
        vault.write("Task", &text);

        assert_eq!(vault.names(None), ["Task"]);
    }

    #[test]
    fn a_missing_task_folder_is_an_error() {
        let vault = TempVault::new("tasks-no-folder");

        let Err(error) = tasks(&vault.path, None) else {
            panic!("listed tasks without a task folder");
        };

        let message = format!("no task folder at {}", vault.path.join(TASKS_DIR).display());
        assert_eq!(error.to_string(), message);
    }

    #[test]
    fn json_lines_hold_only_tasks_and_skipped_notes_come_back_apart() {
        let vault = TaskVault::new("json");
        let tags = &["#Task", "#Config"];
        let due = "2026-11-06T23:59:00";
        vault.note(
            "Ready",
            NoteSpec {
                tags,
                due,
                estimate: "1",
                blocked_by: &["Ghost"],
                ..DEFAULT
            },
        );
        vault.write("Broken", "just text\n");

        let listing = vault.listing(None);

        let ready = vault.path("Ready").display().to_string();
        let expected = json!({
            "name": "Ready", "path": ready, "status": "Not Started", "due": "2026-11-06T23:59",
            "tags": ["#Config"], "estimate_hours": 1.0, "blocked_by": ["Ghost (missing)"],
        });
        let parsed: Vec<Value> = listing
            .lines
            .iter()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(parsed, [expected]);

        let broken = vault.path("Broken").display().to_string();
        let skipped: Vec<String> = listing.skipped.iter().map(ToString::to_string).collect();
        let message = format!("skipped {broken}: no frontmatter between --- lines");
        assert_eq!(skipped, [message]);
    }
}
