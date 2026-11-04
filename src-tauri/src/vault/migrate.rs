use std::fmt;
use std::path::Path;

use punchcard_core::{BLOCK_HEADING, Block, LOG_HEADING, has_buried_log, insert_block};

use super::{TimerError, notes, read_lines, rebuild_derived, set_field, settings, write_lines};
use crate::note::LIST_ITEM;

const SPANS_KEY: &str = "worked_on";

#[derive(Debug, PartialEq)]
pub enum Migrated {
    Moved { note: String, blocks: usize },
    SkippedSpan { note: String, span: String },
    SkippedUnindented { note: String },
    SkippedFilledLog { note: String },
    SkippedBuriedLog { note: String },
}

impl fmt::Display for Migrated {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Migrated::Moved { note, blocks } => {
                write!(
                    formatter,
                    "moved {blocks} {SPANS_KEY} spans into the log of {note}"
                )
            }
            Migrated::SkippedSpan { note, span } => {
                write!(
                    formatter,
                    "skipped {note}: {SPANS_KEY} span does not parse: {span}"
                )
            }
            Migrated::SkippedUnindented { note } => {
                write!(
                    formatter,
                    "skipped {note}: {SPANS_KEY} has no items indented as {LIST_ITEM:?}, move its spans by hand"
                )
            }
            Migrated::SkippedFilledLog { note } => {
                write!(
                    formatter,
                    "skipped {note}: its log already has blocks, merge {SPANS_KEY} by hand"
                )
            }
            Migrated::SkippedBuriedLog { note } => {
                write!(
                    formatter,
                    "skipped {note}: its {LOG_HEADING} is not the last section, move it to the end by hand"
                )
            }
        }
    }
}

pub fn migrate(vault: &Path) -> Result<Vec<Migrated>, TimerError> {
    let day_start = settings::read(vault)?.day_start;
    let mut migrated = Vec::new();
    for note in notes(vault)? {
        let Some(spans) = note.frontmatter.fields.get(SPANS_KEY) else {
            continue;
        };
        let name = note.name().to_string();

        // Removing the key alone would leave its unindented span lines in the frontmatter.
        if spans.is_empty() {
            migrated.push(Migrated::SkippedUnindented { note: name });
            continue;
        }

        // Spans inserted above newer blocks would break newest-first order and could duplicate a block.
        if !note.log.blocks.is_empty() {
            migrated.push(Migrated::SkippedFilledLog { note: name });
            continue;
        }

        let blocks: Result<Vec<Block>, &String> = spans
            .iter()
            .map(|span| Block::parse(&format!("{BLOCK_HEADING}{span}")).ok_or(span))
            .collect();
        let mut blocks = match blocks {
            Ok(blocks) => blocks,
            Err(span) => {
                let span = span.clone();
                migrated.push(Migrated::SkippedSpan { note: name, span });
                continue;
            }
        };
        blocks.sort_by_key(|block| block.span.start);

        let mut lines = read_lines(&note.path)?;
        if has_buried_log(&lines) {
            migrated.push(Migrated::SkippedBuriedLog { note: name });
            continue;
        }

        for block in &blocks {
            insert_block(&mut lines, block);
        }
        set_field(&mut lines, SPANS_KEY, Vec::new(), &name)?;
        rebuild_derived(&mut lines, day_start, &name)?;
        write_lines(&note.path, &lines)?;

        let blocks = blocks.len();
        migrated.push(Migrated::Moved { note: name, blocks });
    }
    Ok(migrated)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::super::tests::TempVault;
    use super::*;

    #[test]
    fn spans_move_into_log_newest_first_and_key_is_removed() {
        let vault = TempVault::new("migrate-moved");
        let note = vault.path.join("Port.md");
        fs::write(
            &note,
            "---\nworked_on:\n  - 26/10/30 09:00 - 26/10/30 10:00\n  - 26/10/31 09:00 -\ntags:\n---\nbody\n",
        )
        .unwrap();

        let migrated = migrate(&vault.path).unwrap();

        let moved = Migrated::Moved {
            note: "Port".to_string(),
            blocks: 2,
        };
        assert_eq!(migrated, [moved]);
        assert_eq!(
            fs::read_to_string(&note).unwrap(),
            "---\ntags:\nworked_days:\n  - \"[[26-10-30]]\"\n  - \"[[26-10-31]]\"\nworked_hours: 1\n---\nbody\n### Log\n#### 26/10/31 09:00 -\n#### 26/10/30 09:00 - 26/10/30 10:00\n"
        );
    }

    #[test]
    fn note_whose_log_has_blocks_is_skipped_unchanged() {
        let vault = TempVault::new("migrate-skipped");
        let note = vault.path.join("Port.md");
        let text = "---\nworked_on:\n  - 26/10/30 09:00 -\n---\n### Log\n#### 26/10/31 09:00 -\n";
        fs::write(&note, text).unwrap();

        let migrated = migrate(&vault.path).unwrap();

        let skipped = Migrated::SkippedFilledLog {
            note: "Port".to_string(),
        };
        assert_eq!(migrated, [skipped]);
        assert_eq!(fs::read_to_string(&note).unwrap(), text);
    }

    #[test]
    fn note_whose_spans_are_not_indented_is_skipped_unchanged() {
        let vault = TempVault::new("migrate-unindented");
        let note = vault.path.join("Port.md");
        let text = "---\nworked_on:\n- 26/10/30 09:00 - 26/10/30 10:00\n---\nbody\n";
        fs::write(&note, text).unwrap();

        let migrated = migrate(&vault.path).unwrap();

        let skipped = Migrated::SkippedUnindented {
            note: "Port".to_string(),
        };
        assert_eq!(migrated, [skipped]);
        assert_eq!(fs::read_to_string(&note).unwrap(), text);
    }

    #[test]
    fn note_with_buried_log_is_skipped_unchanged() {
        let vault = TempVault::new("migrate-buried");
        let note = vault.path.join("Port.md");
        let text = "---\nworked_on:\n  - 26/10/30 09:00 -\n---\n### Log\n### Notes\n";
        fs::write(&note, text).unwrap();

        let migrated = migrate(&vault.path).unwrap();

        let skipped = Migrated::SkippedBuriedLog {
            note: "Port".to_string(),
        };
        assert_eq!(migrated, [skipped]);
        assert_eq!(fs::read_to_string(&note).unwrap(), text);
    }
}
