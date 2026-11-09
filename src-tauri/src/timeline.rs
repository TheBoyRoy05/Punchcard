use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};

use chrono::{Local, NaiveDate};
use punchcard_core::edit::NoteBlock;
use punchcard_core::time::{parse_day, wall_clock};
use serde::Serialize;
use tauri::{State, Url, WebviewWindowBuilder, WindowEvent};

#[cfg(target_os = "macos")]
use crate::macos;
use crate::note::Note;
use crate::vault::{self, TimerError, blocks, settings};
use crate::{popup, rank};

const WINDOW: &str = "timeline";
// The start of every Note::url, and the only link the window hands to another app.
const NOTE_URL: &str = "obsidian://open?path=";
#[cfg(target_os = "macos")]
const OPENER: &str = "open";
#[cfg(not(target_os = "macos"))]
const OPENER: &str = "xdg-open";

// Set once a note link hands focus to Obsidian, so the blur that follows closes the timeline instead of taking focus back.
static NOTE_OPENED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Copy, Serialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Range {
    Day,
    Week,
}

struct Vault(PathBuf);

// Opening, StintLoadedRange and StintNote are the same-named types in src/app/stint.ts, and a test pins their JSON.
// What the page needs before its first load, handed over in an init script so the window needs no command beyond load and save.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Opening {
    range: Range,
    notes: Vec<StintNote>,
    day_start_hour: u32,
    block_minutes: u32,
    block_min_minutes: u32,
}

#[derive(Debug, Serialize)]
struct StintLoadedRange {
    blocks: Vec<NoteBlock>,
    notes: Vec<StintNote>,
}

// `color` hashes the project name, so a project keeps one color in every range and every run.
#[derive(Debug, PartialEq, Serialize)]
struct StintNote {
    name: String,
    path: String,
    project: String,
    color: u32,
    running: bool,
    url: String,
}

pub fn open(vault: PathBuf, range: Range) -> Result<(), TimerError> {
    let opening =
        serde_json::to_string(&opening(&vault, range)?).map_err(|error| error.to_string())?;
    let script = format!("window.stint = {opening};");

    tauri::Builder::default()
        .manage(Vault(vault))
        .invoke_handler(tauri::generate_handler![load, save])
        .on_window_event(|window, event| {
            if let WindowEvent::Focused(false) = event {
                if !NOTE_OPENED.swap(false, Ordering::Relaxed) {
                    popup::on_blur(window);
                } else if let Err(error) = window.close() {
                    eprintln!("stint: {} not closed: {error}", window.label());
                }
            }
        })
        .setup(move |app| {
            // An accessory app has no Dock icon and may draw over another app's full-screen Space, like choose.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let windows = &app.config().app.windows;
            let config = windows.iter().find(|window| window.label == WINDOW);
            let config = config.ok_or_else(|| format!("tauri.conf.json has no {WINDOW} window"))?;
            let dev_url = tauri::is_dev()
                .then(|| app.config().build.dev_url.clone())
                .flatten();
            #[cfg_attr(not(target_os = "macos"), expect(unused_variables))]
            let window = WebviewWindowBuilder::from_config(app.handle(), config)?
                .initialization_script(&script)
                .on_navigation(move |url| navigate(url, dev_url.as_ref()))
                .build()?;

            #[cfg(target_os = "macos")]
            macos::place_popup(&window)?;
            Ok(())
        })
        .run(crate::context())
        .map_err(|error| format!("timeline failed: {error}").into())
}

fn opening(vault: &Path, range: Range) -> Result<Opening, TimerError> {
    let settings = settings::read(vault)?;
    let ranked = rank::ranked(vault::notes(vault)?, wall_clock(Local::now().naive_local()));
    Ok(Opening {
        range,
        notes: ranked.iter().map(StintNote::from_note).collect(),
        day_start_hour: settings.day_start.hour(),
        block_minutes: settings.timeline_block_minutes,
        block_min_minutes: settings.block_min_minutes,
    })
}

// Note links open in Obsidian. The window stays on its own pages, since it has no title bar to come back from another.
fn navigate(url: &Url, dev_url: Option<&Url>) -> bool {
    if url.as_str().starts_with(NOTE_URL) {
        match Command::new(OPENER).arg(url.as_str()).status() {
            Ok(status) if status.success() => NOTE_OPENED.store(true, Ordering::Relaxed),
            Ok(status) => eprintln!("stint: {url} not opened: {OPENER} {status}"),
            Err(error) => eprintln!("stint: {url} not opened: {error}"),
        }
        return false;
    }

    let own = is_app_page(url) || dev_url.is_some_and(|dev| dev.origin() == url.origin());
    if !own {
        eprintln!("stint: {url} not opened: the timeline shows only its own pages");
    }
    own
}

// Tauri serves the app at tauri://localhost, and at http(s)://tauri.localhost only on Windows.
fn is_app_page(url: &Url) -> bool {
    let (schemes, host): (&[&str], _) = if cfg!(windows) {
        (&["http", "https"], "tauri.localhost")
    } else {
        (&["tauri"], "localhost")
    };
    schemes.contains(&url.scheme()) && url.host_str() == Some(host) && url.port().is_none()
}

#[tauri::command(async)]
fn load(
    vault: State<'_, Vault>,
    first_day: String,
    last_day: String,
) -> Result<StintLoadedRange, TimerError> {
    load_range(&vault.0, &first_day, &last_day)
}

#[tauri::command(async)]
fn save(
    vault: State<'_, Vault>,
    original: Option<NoteBlock>,
    block: Option<NoteBlock>,
) -> Result<(), TimerError> {
    save_note_block(&vault.0, original, block)
}

fn load_range(
    vault: &Path,
    first_day: &str,
    last_day: &str,
) -> Result<StintLoadedRange, TimerError> {
    let touching = blocks::blocks_in(vault, logical_day(first_day)?, logical_day(last_day)?)?;

    let mut loaded = StintLoadedRange {
        blocks: Vec::new(),
        notes: Vec::new(),
    };
    for (note, blocks) in touching {
        let task = StintNote::from_note(&note);
        let blocks = blocks.into_iter().map(|block| NoteBlock {
            note: task.path.clone(),
            block,
        });
        loaded.blocks.extend(blocks);
        loaded.notes.push(task);
    }
    Ok(loaded)
}

// A save writes one note, so a block and its original must name the same one; a task move is two saves.
fn save_note_block(
    vault: &Path,
    original: Option<NoteBlock>,
    block: Option<NoteBlock>,
) -> Result<(), TimerError> {
    let note = match (&original, &block) {
        (Some(original), Some(block)) if original.note != block.note => {
            let message = format!(
                "{} and {} are two notes, so one save cannot write both",
                original.note, block.note
            );
            return Err(TimerError::from(message));
        }
        (Some(NoteBlock { note, .. }), _) | (None, Some(NoteBlock { note, .. })) => {
            PathBuf::from(note)
        }
        (None, None) => return Err("A save needs a block or an original".to_string().into()),
    };

    let original = original.map(|original| original.block);
    blocks::save_block(
        vault,
        &note,
        original.as_ref(),
        block.map(|block| block.block),
    )
}

fn logical_day(text: &str) -> Result<NaiveDate, TimerError> {
    parse_day(text).ok_or_else(|| format!("Not a logical day: {text}").into())
}

impl StintNote {
    fn from_note(note: &Note) -> Self {
        let project = note.project().to_string();
        Self {
            name: note.name().to_string(),
            path: note.path.to_string_lossy().into_owned(),
            color: project_color(&project),
            project,
            running: note.log.running(),
            url: note.url(),
        }
    }
}

// FNV-1a, so the color depends only on the name, never on which notes a range holds.
fn project_color(project: &str) -> u32 {
    project.bytes().fold(0x811c_9dc5, |hash, byte| {
        (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193)
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use punchcard_core::Block;
    use punchcard_core::span::Span;
    use punchcard_core::time;

    use super::*;
    use crate::vault::tests::TempVault;

    fn write(vault: &TempVault, file: &str, text: &str) {
        let path = vault.path.join(file);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn projects(loaded: &StintLoadedRange) -> Vec<(&str, &str)> {
        let notes = loaded.notes.iter();
        let mut projects: Vec<_> = notes
            .map(|note| (note.name.as_str(), note.project.as_str()))
            .collect();
        projects.sort();
        projects
    }

    #[test]
    fn load_names_a_project_note_as_its_own_project_and_a_task_by_its_first_tag_link() {
        let vault = TempVault::new("load-projects");
        let log = "### Log\n#### 26/11/01 09:00 - 26/11/01 10:00\n";
        write(
            &vault,
            "Projects/Port.md",
            &format!("---\ntags:\n  - \"#Project\"\ntag_links:\n  - \"[[Other]]\"\n---\n{log}"),
        );
        write(
            &vault,
            "Tasks/Ship.md",
            &format!(
                "---\ntags:\n  - \"#Task\"\ntag_links:\n  - \"[[Port|the port]]\"\n  - \"[[Other]]\"\n---\n{log}"
            ),
        );
        write(
            &vault,
            "Tasks/Loose.md",
            &format!("---\ntags:\n  - \"#Task\"\n---\n{log}"),
        );

        let loaded = load_range(&vault.path, "26-11-01", "26-11-01").unwrap();

        assert_eq!(
            projects(&loaded),
            [("Loose", ""), ("Port", "Port"), ("Ship", "Port")]
        );
    }

    #[test]
    fn load_lists_every_note_a_block_references_and_no_other() {
        let vault = TempVault::new("load-notes");
        write(
            &vault,
            "Tasks/Done.md",
            "---\nstatus: Completed\n---\n### Log\n#### 26/11/01 09:00 - 26/11/01 10:00\n",
        );
        write(
            &vault,
            "Tasks/Idle.md",
            "---\nstatus: In Progress\n---\n### Log\n#### 26/10/01 09:00 - 26/10/01 10:00\n",
        );

        let loaded = load_range(&vault.path, "26-11-01", "26-11-07").unwrap();

        let path = vault
            .path
            .join("Tasks/Done.md")
            .to_string_lossy()
            .into_owned();
        assert_eq!(loaded.notes.len(), 1);
        assert_eq!(loaded.notes[0].path, path);
        assert!(loaded.blocks.iter().all(|block| block.note == path));
    }

    #[test]
    fn load_refuses_a_day_that_is_not_a_logical_day() {
        let vault = TempVault::new("load-bad-day");

        let error = load_range(&vault.path, "2026-11-01", "26-11-01").unwrap_err();

        assert_eq!(error.to_string(), "Not a logical day: 2026-11-01");
    }

    #[test]
    fn save_refuses_a_block_and_an_original_from_two_notes() {
        let vault = TempVault::new("save-two-notes");
        let text = "---\n---\n### Log\n#### 26/11/01 09:00 - 26/11/01 10:00\n";
        write(&vault, "Port.md", text);
        write(&vault, "Ship.md", "---\n---\n");
        let block = |file: &str| NoteBlock {
            note: vault.path.join(file).to_string_lossy().into_owned(),
            block: Block {
                span: Span {
                    start: time::parse_time("26/11/01 09:00").unwrap(),
                    end: Some(time::parse_time("26/11/01 10:00").unwrap()),
                },
                entry: String::new(),
            },
        };

        let error = save_note_block(&vault.path, Some(block("Port.md")), Some(block("Ship.md")))
            .unwrap_err();

        assert!(
            error
                .to_string()
                .ends_with("are two notes, so one save cannot write both")
        );
        assert_eq!(
            fs::read_to_string(vault.path.join("Port.md")).unwrap(),
            text
        );
        assert_eq!(
            fs::read_to_string(vault.path.join("Ship.md")).unwrap(),
            "---\n---\n"
        );
    }

    #[test]
    fn navigation_stays_on_the_apps_own_pages() {
        let dev = Url::parse("http://127.0.0.1:1420/").unwrap();
        let allowed = |url: &str| navigate(&Url::parse(url).unwrap(), Some(&dev));

        assert!(allowed("http://127.0.0.1:1420/timeline.html"));
        assert!(!allowed("https://example.com/"));
        assert!(!allowed("http://127.0.0.1:1421/timeline.html"));
        assert!(!navigate(
            &Url::parse("http://127.0.0.1:1420/").unwrap(),
            None
        ));
    }

    #[test]
    fn navigation_allows_only_the_origin_tauri_serves_on_this_os() {
        let allowed = |url: &str| navigate(&Url::parse(url).unwrap(), None);
        let windows = cfg!(windows);

        assert_eq!(allowed("tauri://localhost/timeline.html"), !windows);
        assert_eq!(allowed("http://tauri.localhost/timeline.html"), windows);
        assert_eq!(allowed("https://tauri.localhost/timeline.html"), windows);
        assert!(!allowed("ftp://tauri.localhost/timeline.html"));
        assert!(!allowed("http://tauri.localhost:8080/timeline.html"));
        assert!(!allowed("tauri://example.com/timeline.html"));
    }

    // tests/app/stint.test.ts types the same file as src/app/stint.ts, so a field renamed on one side fails a test.
    #[test]
    fn the_page_reads_the_json_pinned_in_the_stint_fixture() {
        let note = || StintNote {
            name: "Port".to_string(),
            path: "/Vault/Tasks/Port.md".to_string(),
            project: "Punchcard".to_string(),
            color: 7,
            running: true,
            url: "obsidian://open?path=/Vault/Tasks/Port.md".to_string(),
        };
        let block = NoteBlock {
            note: note().path,
            block: Block {
                span: Span {
                    start: time::parse_time("26/11/01 09:00").unwrap(),
                    end: None,
                },
                entry: "Ported.".to_string(),
            },
        };
        let opening = Opening {
            range: Range::Week,
            notes: vec![note()],
            day_start_hour: 4,
            block_minutes: 30,
            block_min_minutes: 3,
        };
        let loaded = StintLoadedRange {
            blocks: vec![block],
            notes: vec![note()],
        };

        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/stint.json");
        let expected: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(fixture).unwrap()).unwrap();
        assert_eq!(
            serde_json::json!({ "opening": opening, "loaded": loaded }),
            expected
        );
    }

    #[test]
    fn different_projects_get_different_colors() {
        assert_ne!(project_color("Port"), project_color("Ship"));
    }
}
