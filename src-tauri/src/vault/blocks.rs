use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use punchcard_core::day::day_window;
use punchcard_core::edit::{EditError, check_block, check_running};
use punchcard_core::span::Span;
use punchcard_core::{Block, Log, NoteBody, has_buried_log};

use super::{
    TimerError, buried_log, is_hidden, is_markdown, notes, read_lines, rebuild_derived, settings,
    write_lines,
};
use crate::note::{Note, note_name};

// Every note with a block that touches the logical days from `first` to `last`, with only those blocks.
pub fn blocks_in(
    vault: &Path,
    first: NaiveDate,
    last: NaiveDate,
) -> Result<Vec<(Note, Vec<Block>)>, TimerError> {
    let day_start = settings::read(vault)?.day_start;
    let range = Span {
        start: day_window(first, day_start).start,
        end: Some(day_window(last, day_start).end),
    };

    let mut touching = Vec::new();
    for note in notes(vault)? {
        let blocks = note
            .log
            .blocks
            .iter()
            .filter(|block| block.span.overlaps(&range));
        let blocks: Vec<Block> = blocks.cloned().collect();
        if !blocks.is_empty() {
            touching.push((note, blocks));
        }
    }
    Ok(touching)
}

// No original adds `block`, no block deletes `original`. The original, entry included, must still be in the
// log, so an edit made over a log that changed since it was read, such as by a toggle or a sync, is refused.
// Overlap is not checked: a merge move writes the merged block before it deletes the blocks it swallowed.
pub fn save_block(
    vault: &Path,
    path: &Path,
    original: Option<&Block>,
    block: Option<Block>,
) -> Result<(), TimerError> {
    let refused = || TimerError(format!("{} is not a note in the vault", path.display()));
    let path = note_in_vault(vault, path).ok_or_else(refused)?;
    let name = note_name(&path);
    let day_start = settings::read(vault)?.day_start;
    let lines = read_lines(&path)?;
    if Note::from_lines(path.clone(), &lines).is_none() {
        return Err(refused());
    }
    if has_buried_log(&lines) {
        return Err(buried_log(name));
    }
    let refused_edit = |error: EditError| TimerError(format!("{name}: {error}"));
    if let Some(block) = &block {
        check_block(block).map_err(refused_edit)?;
    }

    let mut body = NoteBody::parse(&lines.join("\n"));
    if let Some(original) = original {
        let blocks = &mut body.log.blocks;
        let index = blocks.iter().position(|kept| kept == original);
        let index = index.ok_or_else(|| {
            let message = format!(
                "{name} changed since it was loaded and no longer has the block {}",
                original.heading()
            );
            TimerError(message)
        })?;
        blocks.remove(index);
    }
    body.log.blocks.extend(block);
    check_running(&body.log).map_err(refused_edit)?;

    let mut lines: Vec<String> = body.format().split('\n').map(String::from).collect();
    rebuild_derived(&mut lines, day_start, name)?;

    // Entry text the rules above missed could still end the log or split a block, and a write would then drop the derived properties.
    if has_buried_log(&lines) || !same_blocks(&Log::parse(&lines).blocks, &body.log.blocks) {
        let message =
            format!("{name} would not read back as the log being saved, so it was not written");
        return Err(TimerError(message));
    }
    write_lines(&path, &lines)?;
    Ok(())
}

// The path comes from the webview, so it must resolve, through any symlink, to a file that collect_notes reads as a note.
fn note_in_vault(vault: &Path, path: &Path) -> Option<PathBuf> {
    let resolved = path.canonicalize().ok()?;
    let relative = resolved.strip_prefix(vault.canonicalize().ok()?).ok()?;
    let hidden = relative.iter().any(is_hidden);
    (!hidden && is_markdown(relative)).then_some(resolved)
}

// The log is formatted newest first, so order alone never makes two logs differ.
fn same_blocks(written: &[Block], saved: &[Block]) -> bool {
    let sorted = |blocks: &[Block]| {
        let mut blocks = blocks.to_vec();
        blocks.sort_by(|a, b| {
            (a.span.start, a.span.end, &a.entry).cmp(&(b.span.start, b.span.end, &b.entry))
        });
        blocks
    };
    sorted(written) == sorted(saved)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use punchcard_core::time::parse_day;

    use super::super::tests::TempVault;
    use super::*;

    fn day(text: &str) -> NaiveDate {
        parse_day(text).unwrap()
    }

    fn block(heading: &str, entry: &str) -> Block {
        let mut block = Block::parse(&format!("#### {heading}")).unwrap();
        block.entry = entry.to_string();
        block
    }

    fn write(vault: &TempVault, file: &str, text: &str) -> PathBuf {
        let path = vault.path.join(file);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, text).unwrap();
        path
    }

    fn headings(touching: &[(Note, Vec<Block>)]) -> Vec<(String, Vec<String>)> {
        let headings = touching.iter().map(|(note, blocks)| {
            let blocks = blocks.iter().map(Block::heading).collect();
            (note.name().to_string(), blocks)
        });
        headings.collect()
    }

    // The fixture day starts at 04:00, so 26-11-01 runs from 26/11/01 04:00 to 26/11/02 04:00.
    #[test]
    fn blocks_in_keeps_blocks_that_overlap_the_range_and_drops_ones_that_only_meet_its_edges() {
        let vault = TempVault::new("blocks-in-edges");
        let log = "---\n---\n### Log\n#### 26/11/02 04:00 - 26/11/02 05:00\n#### 26/11/02 03:00 - 26/11/02 04:30\n#### 26/11/01 03:00 - 26/11/01 04:00\n#### 26/11/01 03:30 - 26/11/01 04:10\n";
        write(&vault, "Tasks/Port.md", log);
        write(
            &vault,
            "Tasks/Idle.md",
            "---\n---\n### Log\n#### 26/10/20 09:00 - 26/10/20 10:00\n",
        );

        let touching = blocks_in(&vault.path, day("26-11-01"), day("26-11-01")).unwrap();

        let kept = vec![
            "#### 26/11/02 03:00 - 26/11/02 04:30".to_string(),
            "#### 26/11/01 03:30 - 26/11/01 04:10".to_string(),
        ];
        assert_eq!(headings(&touching), [("Port".to_string(), kept)]);
    }

    #[test]
    fn blocks_in_keeps_a_running_block_that_started_before_the_range() {
        let vault = TempVault::new("blocks-in-running");
        write(
            &vault,
            "Port.md",
            "---\n---\n### Log\n#### 26/10/30 09:00 -\n",
        );

        let touching = blocks_in(&vault.path, day("26-11-01"), day("26-11-07")).unwrap();

        assert_eq!(touching.len(), 1);
    }

    #[test]
    fn save_adds_a_block_and_rebuilds_derived_properties() {
        let vault = TempVault::new("save-add");
        let path = write(&vault, "Port.md", "---\nstatus: Done\n---\n### Log\n");

        let added = block("26/11/01 09:00 - 26/11/01 10:30", "Wrote it.");
        save_block(&vault.path, &path, None, Some(added)).unwrap();

        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "---\nstatus: Done\npunchcard__worked_days:\n  - \"[[26-11-01]]\"\npunchcard__worked_hours: 1.5\n---\n### Log\n#### 26/11/01 09:00 - 26/11/01 10:30\nWrote it.\n"
        );
    }

    #[test]
    fn save_replaces_the_original_block_and_keeps_the_log_newest_first() {
        let vault = TempVault::new("save-edit");
        let text = "---\n---\n### Log\n#### 26/11/01 12:00 - 26/11/01 13:00\n#### 26/11/01 09:00 - 26/11/01 10:00\nOld.\n";
        let path = write(&vault, "Port.md", text);

        let original = block("26/11/01 09:00 - 26/11/01 10:00", "Old.");
        let moved = block("26/11/01 14:00 - 26/11/01 15:00", "New.");
        save_block(&vault.path, &path, Some(&original), Some(moved)).unwrap();

        let text = fs::read_to_string(&path).unwrap();
        assert!(text.ends_with(
            "### Log\n#### 26/11/01 14:00 - 26/11/01 15:00\nNew.\n#### 26/11/01 12:00 - 26/11/01 13:00\n"
        ));
        assert!(text.contains("punchcard__worked_hours: 2\n"));
    }

    #[test]
    fn save_without_a_block_deletes_the_original_and_its_derived_properties() {
        let vault = TempVault::new("save-delete");
        let text = "---\npunchcard__worked_days:\n  - \"[[26-11-01]]\"\npunchcard__worked_hours: 1\n---\n### Log\n#### 26/11/01 09:00 - 26/11/01 10:00\n";
        let path = write(&vault, "Port.md", text);

        let original = block("26/11/01 09:00 - 26/11/01 10:00", "");
        save_block(&vault.path, &path, Some(&original), None).unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "---\n---\n### Log\n");
    }

    #[test]
    fn save_restores_a_deleted_zero_length_block_as_undo_does() {
        let vault = TempVault::new("save-zero");
        let text = "---\n---\n### Log\n#### 26/10/01 09:00 - 26/10/01 09:00\nOld.\n";
        let path = write(&vault, "Port.md", text);
        let zero = block("26/10/01 09:00 - 26/10/01 09:00", "Old.");

        save_block(&vault.path, &path, Some(&zero), None).unwrap();
        save_block(&vault.path, &path, None, Some(zero)).unwrap();

        let text = fs::read_to_string(&path).unwrap();
        assert!(text.ends_with("### Log\n#### 26/10/01 09:00 - 26/10/01 09:00\nOld.\n"));
    }

    #[test]
    fn save_refuses_an_original_the_log_no_longer_has() {
        let vault = TempVault::new("save-stale");
        let text = "---\n---\n### Log\n#### 26/11/01 09:00 -\n";
        let path = write(&vault, "Port.md", text);

        let original = block("26/11/01 09:00 - 26/11/01 09:30", "");
        let error = save_block(&vault.path, &path, Some(&original), None).unwrap_err();

        assert_eq!(
            error.to_string(),
            "Port changed since it was loaded and no longer has the block #### 26/11/01 09:00 - 26/11/01 09:30"
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), text);
    }

    #[test]
    fn save_refuses_a_path_outside_the_vault() {
        let vault = TempVault::new("save-outside");
        let outside = TempVault::new("save-outside-other");
        let path = write(&outside, "Port.md", "---\n---\n");
        let escape = vault
            .path
            .join("..")
            .join(outside.path.file_name().unwrap())
            .join("Port.md");

        let added = block("26/11/01 09:00 - 26/11/01 10:00", "");
        let error = save_block(&vault.path, &escape, None, Some(added)).unwrap_err();

        assert!(error.to_string().ends_with("is not a note in the vault"));
        assert_eq!(fs::read_to_string(&path).unwrap(), "---\n---\n");
    }

    #[test]
    fn save_refuses_an_entry_that_would_end_the_log_and_keeps_the_note() {
        let vault = TempVault::new("save-heading-entry");
        let text =
            "---\npunchcard__worked_hours: 1\n---\n### Log\n#### 26/11/01 09:00 - 26/11/01 10:00\n";
        let path = write(&vault, "Port.md", text);

        let original = block("26/11/01 09:00 - 26/11/01 10:00", "");
        for entry in ["Done.\n### Next", "#### 26/11/01 11:00 -"] {
            let edited = block("26/11/01 09:00 - 26/11/01 10:00", entry);
            let error = save_block(&vault.path, &path, Some(&original), Some(edited)).unwrap_err();
            assert_eq!(
                error.to_string(),
                "Port: A log entry cannot hold a section or block heading line."
            );
        }
        assert_eq!(fs::read_to_string(&path).unwrap(), text);
    }

    #[test]
    fn save_refuses_an_original_whose_entry_changed_since_it_was_loaded() {
        let vault = TempVault::new("save-stale-entry");
        let text =
            "---\n---\n### Log\n#### 26/11/01 09:00 - 26/11/01 10:00\nWritten on the phone.\n";
        let path = write(&vault, "Port.md", text);

        let original = block("26/11/01 09:00 - 26/11/01 10:00", "");
        let moved = block("26/11/01 10:00 - 26/11/01 11:00", "");
        let error = save_block(&vault.path, &path, Some(&original), Some(moved)).unwrap_err();

        assert!(
            error
                .to_string()
                .starts_with("Port changed since it was loaded")
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), text);
    }

    #[test]
    fn save_refuses_a_second_running_block_and_an_end_before_the_start() {
        let vault = TempVault::new("save-block-rules");
        let text = "---\n---\n### Log\n#### 26/11/01 09:00 -\n";
        let path = write(&vault, "Port.md", text);

        let cases = [
            (
                "26/11/01 07:00 -",
                "Port: A note can have only one running block.",
            ),
            (
                "26/11/01 08:00 - 26/11/01 07:00",
                "Port: The end must be after the start.",
            ),
        ];
        for (heading, message) in cases {
            let error = save_block(&vault.path, &path, None, Some(block(heading, ""))).unwrap_err();
            assert_eq!(error.to_string(), message);
        }
        assert_eq!(fs::read_to_string(&path).unwrap(), text);
    }

    #[test]
    fn save_refuses_files_that_collect_notes_does_not_read_as_notes() {
        let vault = TempVault::new("save-not-notes");
        let files = [
            write(&vault, "_Meta/Other.md", "---\n---\n"),
            write(&vault, ".obsidian/Port.md", "---\n---\n"),
            write(&vault, ".trash/Port.md", "---\n---\n"),
            write(&vault, "Plain.md", "No frontmatter.\n\n---\n"),
        ];

        for path in files {
            let before = fs::read_to_string(&path).unwrap();
            let added = block("26/11/01 09:00 - 26/11/01 10:00", "");
            let error = save_block(&vault.path, &path, None, Some(added)).unwrap_err();
            assert!(
                error.to_string().ends_with("is not a note in the vault"),
                "{}",
                path.display()
            );
            assert_eq!(fs::read_to_string(&path).unwrap(), before);
        }
    }

    #[cfg(unix)]
    #[test]
    fn save_refuses_a_symlink_that_leads_out_of_the_vault() {
        let vault = TempVault::new("save-symlink");
        let outside = TempVault::new("save-symlink-other");
        let target = write(&outside, "Port.md", "---\n---\n");
        let link = vault.path.join("Port.md");
        std::os::unix::fs::symlink(&target, &link).unwrap();

        let added = block("26/11/01 09:00 - 26/11/01 10:00", "");
        let error = save_block(&vault.path, &link, None, Some(added)).unwrap_err();

        assert!(error.to_string().ends_with("is not a note in the vault"));
        assert_eq!(fs::read_to_string(&target).unwrap(), "---\n---\n");
    }

    #[test]
    fn save_refuses_a_file_that_is_not_a_note() {
        let vault = TempVault::new("save-not-md");
        let path = write(&vault, "Port.txt", "---\n---\n");

        let added = block("26/11/01 09:00 - 26/11/01 10:00", "");
        let error = save_block(&vault.path, &path, None, Some(added)).unwrap_err();

        assert!(error.to_string().ends_with("is not a note in the vault"));
        assert_eq!(fs::read_to_string(&path).unwrap(), "---\n---\n");
    }
}
