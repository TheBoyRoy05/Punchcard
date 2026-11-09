use std::fmt;

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

use crate::span::{Span, shorter_than};
use crate::{Block, Log, section_headings};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "crate::wire::FlatBlock", into = "crate::wire::FlatBlock")]
pub struct NoteBlock {
    pub note: String,
    pub block: Block,
}

#[derive(Debug, PartialEq, Eq)]
pub enum EditError {
    Gone,
    EndNotAfterStart,
    TooShort(u32),
    EndInFuture,
    RunningStartsAfterNow,
    Overlaps(Span),
    HeadingInEntry,
    SecondRunning,
}

impl fmt::Display for EditError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Gone => write!(formatter, "The block is no longer in the log."),
            Self::EndNotAfterStart => write!(formatter, "The end must be after the start."),
            Self::TooShort(minutes) => {
                write!(
                    formatter,
                    "A block must be at least {minutes} minutes long."
                )
            }
            Self::EndInFuture => write!(formatter, "The end cannot be in the future."),
            Self::RunningStartsAfterNow => {
                write!(formatter, "A running block must start before now.")
            }
            Self::Overlaps(span) => write!(formatter, "It overlaps the block {span}."),
            Self::HeadingInEntry => write!(
                formatter,
                "A log entry cannot hold a section or block heading line."
            ),
            Self::SecondRunning => write!(formatter, "A note can have only one running block."),
        }
    }
}

impl std::error::Error for EditError {}

pub fn edit_block(
    blocks: &[NoteBlock],
    original: &NoteBlock,
    next: NoteBlock,
    now: NaiveDateTime,
    block_min_minutes: u32,
) -> Result<Vec<NoteBlock>, EditError> {
    let index = blocks
        .iter()
        .position(|block| {
            block.note == original.note && block.block.span.start == original.block.span.start
        })
        .ok_or(EditError::Gone)?;

    check_block(&next.block)?;
    let Span { start, end } = next.block.span;
    if let Some(end) = end
        && next.block.span != original.block.span
    {
        if end == start {
            return Err(EditError::EndNotAfterStart);
        }
        if shorter_than(start, end, block_min_minutes) {
            return Err(EditError::TooShort(block_min_minutes));
        }
    }

    match end {
        Some(end) if end > now => return Err(EditError::EndInFuture),
        None if start >= now => return Err(EditError::RunningStartsAfterNow),
        _ => {}
    }

    let mut others = blocks.to_vec();
    others.remove(index);
    if next.note != original.note {
        return Ok(move_into(others, next));
    }

    let clash = others
        .iter()
        .find(|block| block.note == next.note && block.block.span.overlaps(&next.block.span));
    if let Some(clash) = clash {
        return Err(EditError::Overlaps(clash.block.span));
    }

    let mut edited = blocks.to_vec();
    edited[index] = next;
    Ok(edited)
}

// A block moved to another task merges with what it overlaps there, instead of being refused.
fn move_into(others: Vec<NoteBlock>, moved: NoteBlock) -> Vec<NoteBlock> {
    // A note's own blocks never overlap each other, so only blocks overlapping the moved one can join it.
    let (mut parts, mut kept): (Vec<_>, Vec<_>) = others.into_iter().partition(|block| {
        block.note == moved.note && block.block.span.overlaps(&moved.block.span)
    });
    parts.insert(0, moved);
    parts.sort_by_key(|part| part.block.span.start);

    let start = parts[0].block.span.start;
    let ends: Option<Vec<NaiveDateTime>> = parts.iter().map(|part| part.block.span.end).collect();
    let end = ends.and_then(|ends| ends.into_iter().max());

    let entries: Vec<&str> = parts
        .iter()
        .map(|part| part.block.entry.as_str())
        .filter(|entry| !entry.is_empty())
        .collect();

    let merged = NoteBlock {
        note: parts[0].note.clone(),
        block: Block {
            span: Span { start, end },
            entry: entries.join("\n"),
        },
    };
    kept.push(merged);
    kept
}

// The rules one block keeps whatever else the log holds, so a host's save holds every write to them too.
// Older blocks can be zero-length, and an undo or a task move writes them back as they were, so only an inverted span is refused.
pub fn check_block(next: &Block) -> Result<(), EditError> {
    let Span { start, end } = next.span;
    if end.is_some_and(|end| end < start) {
        return Err(EditError::EndNotAfterStart);
    }

    // A section heading would end the log, and a block heading would split the entry into another block.
    let lines: Vec<String> = next.entry.split('\n').map(String::from).collect();
    let block_heading = lines.iter().any(|line| Block::parse(line).is_some());
    if block_heading || !section_headings(&lines).is_empty() {
        return Err(EditError::HeadingInEntry);
    }
    Ok(())
}

// edit_block needs no call: two running blocks always overlap, so it already refuses or merges them.
pub fn check_running(log: &Log) -> Result<(), EditError> {
    if log.running_blocks().nth(1).is_some() {
        return Err(EditError::SecondRunning);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NoteBody;
    use crate::test_support::{FIXTURE_SETTINGS, ISO_MINUTE, for_each_fixture};
    use crate::time::parse_time;

    const NOW: &str = "26/10/30 18:00";

    fn block(note: &str, start: &str, end: Option<&str>, entry: &str) -> NoteBlock {
        let time = |clock: &str| parse_time(&format!("26/10/30 {clock}")).unwrap();
        let span = Span {
            start: time(start),
            end: end.map(time),
        };
        NoteBlock {
            note: note.to_string(),
            block: Block {
                span,
                entry: entry.to_string(),
            },
        }
    }

    fn closed(note: &str, start: &str, end: &str) -> NoteBlock {
        block(note, start, Some(end), "")
    }

    fn moved_to(note: &str, block: &NoteBlock) -> NoteBlock {
        NoteBlock {
            note: note.to_string(),
            ..block.clone()
        }
    }

    fn edit(
        blocks: &[NoteBlock],
        original: &NoteBlock,
        next: NoteBlock,
    ) -> Result<Vec<NoteBlock>, EditError> {
        edit_block(
            blocks,
            original,
            next,
            parse_time(NOW).unwrap(),
            FIXTURE_SETTINGS.block_min_minutes,
        )
    }

    fn morning_and_noon() -> (NoteBlock, NoteBlock) {
        (closed("A", "09:00", "10:00"), closed("A", "12:00", "13:00"))
    }

    #[test]
    fn edit_replaces_the_block_in_place() {
        let (morning, noon) = morning_and_noon();
        let next = closed("A", "09:00", "11:00");

        let edited = edit(&[noon.clone(), morning.clone()], &morning, next.clone());

        assert_eq!(edited, Ok(vec![noon, next]));
    }

    #[test]
    fn edit_refuses_an_end_at_or_before_the_start() {
        let (morning, noon) = morning_and_noon();
        let blocks = [noon, morning.clone()];

        for end in ["09:00", "08:00"] {
            let next = closed("A", "09:00", end);
            assert_eq!(
                edit(&blocks, &morning, next),
                Err(EditError::EndNotAfterStart)
            );
        }
    }

    #[test]
    fn edit_refuses_an_end_in_the_future() {
        let (morning, _) = morning_and_noon();

        let edited = edit(
            std::slice::from_ref(&morning),
            &morning,
            closed("A", "09:00", "19:00"),
        );

        assert_eq!(edited, Err(EditError::EndInFuture));
    }

    #[test]
    fn edit_refuses_an_overlap_in_the_same_note() {
        let (morning, noon) = morning_and_noon();

        let edited = edit(
            &[noon.clone(), morning.clone()],
            &morning,
            closed("A", "09:00", "12:30"),
        );

        assert_eq!(edited, Err(EditError::Overlaps(noon.block.span)));
    }

    #[test]
    fn edit_refuses_a_block_that_is_not_in_the_list() {
        let (morning, noon) = morning_and_noon();
        let missing = closed("A", "15:00", "16:00");

        let edited = edit(&[noon, morning], &missing, closed("A", "15:00", "16:30"));

        assert_eq!(edited, Err(EditError::Gone));
    }

    #[test]
    fn edit_allows_an_overlap_with_another_note() {
        let (morning, noon) = morning_and_noon();
        let other = closed("B", "09:30", "10:30");
        let next = closed("A", "09:00", "11:00");

        let edited = edit(
            &[noon.clone(), morning.clone(), other.clone()],
            &morning,
            next.clone(),
        );

        assert_eq!(edited, Ok(vec![noon, next, other]));
    }

    #[test]
    fn edit_allows_a_block_that_only_touches_another() {
        let (morning, noon) = morning_and_noon();

        let edited = edit(
            &[noon, morning.clone()],
            &morning,
            closed("A", "09:00", "12:00"),
        );

        assert!(edited.is_ok());
    }

    #[test]
    fn edit_refuses_a_changed_span_below_the_minimum() {
        let (morning, _) = morning_and_noon();

        let edited = edit(
            std::slice::from_ref(&morning),
            &morning,
            closed("A", "09:00", "09:02"),
        );

        let too_short = EditError::TooShort(FIXTURE_SETTINGS.block_min_minutes);
        assert_eq!(edited, Err(too_short));
    }

    #[test]
    fn edit_allows_a_block_exactly_the_minimum_long() {
        let (morning, _) = morning_and_noon();

        assert!(
            edit(
                std::slice::from_ref(&morning),
                &morning,
                closed("A", "09:00", "09:03")
            )
            .is_ok()
        );
    }

    #[test]
    fn edit_saves_an_entry_on_an_existing_short_or_empty_block() {
        for end in ["15:02", "15:00"] {
            let short = closed("A", "15:00", end);
            let next = block("A", "15:00", Some(end), "Logged.");

            let edited = edit(std::slice::from_ref(&short), &short, next.clone());

            assert_eq!(edited, Ok(vec![next]), "{end}");
        }
    }

    #[test]
    fn edit_allows_a_running_block_started_under_the_minimum_ago() {
        let (morning, _) = morning_and_noon();

        assert!(
            edit(
                std::slice::from_ref(&morning),
                &morning,
                block("A", "17:59", None, "")
            )
            .is_ok()
        );
    }

    #[test]
    fn edit_refuses_a_running_block_that_starts_in_the_future() {
        let (morning, _) = morning_and_noon();

        let edited = edit(
            std::slice::from_ref(&morning),
            &morning,
            block("A", "19:00", None, ""),
        );

        assert_eq!(edited, Err(EditError::RunningStartsAfterNow));
    }

    #[test]
    fn move_merges_with_an_overlapping_block_entries_in_time_order() {
        let moved = block("A", "09:00", Some("10:00"), "First.");
        let target = block("B", "09:30", Some("11:00"), "Second.");

        let edited = edit(&[moved.clone(), target], &moved, moved_to("B", &moved));

        let merged = block("B", "09:00", Some("11:00"), "First.\nSecond.");
        assert_eq!(edited, Ok(vec![merged]));
    }

    #[test]
    fn move_merges_every_block_the_moved_one_overlaps() {
        let moved = block("A", "09:00", Some("12:00"), "First.");
        let second = block("B", "09:30", Some("10:00"), "Second.");
        let third = block("B", "11:00", Some("12:30"), "Third.");

        let edited = edit(
            &[moved.clone(), second, third],
            &moved,
            moved_to("B", &moved),
        );

        let merged = block("B", "09:00", Some("12:30"), "First.\nSecond.\nThird.");
        assert_eq!(edited, Ok(vec![merged]));
    }

    #[test]
    fn move_into_a_running_block_stays_running() {
        let moved = block("A", "09:00", Some("10:00"), "First.");
        let running = block("B", "09:30", None, "");

        let edited = edit(&[moved.clone(), running], &moved, moved_to("B", &moved));

        assert_eq!(edited, Ok(vec![block("B", "09:00", None, "First.")]));
    }

    #[test]
    fn move_keeps_blocks_that_only_touch_separate() {
        let moved = closed("A", "09:00", "10:00");
        let target = closed("B", "10:00", "11:00");
        let next = moved_to("B", &moved);

        let edited = edit(&[moved.clone(), target.clone()], &moved, next.clone());

        assert_eq!(edited, Ok(vec![target, next]));
    }

    #[test]
    fn move_keeps_an_existing_short_block_as_it_is() {
        let short = closed("A", "15:00", "15:02");
        let next = moved_to("B", &short);

        let edited = edit(std::slice::from_ref(&short), &short, next.clone());

        assert_eq!(edited, Ok(vec![next]));
    }

    #[test]
    fn move_still_checks_the_minimum() {
        let moved = closed("A", "09:00", "10:00");
        let next = closed("B", "09:00", "09:01");

        let edited = edit(std::slice::from_ref(&moved), &moved, next);

        let too_short = EditError::TooShort(FIXTURE_SETTINGS.block_min_minutes);
        assert_eq!(edited, Err(too_short));
    }

    #[test]
    fn move_still_checks_the_future() {
        let moved = closed("A", "09:00", "10:00");
        let next = closed("B", "09:00", "19:00");

        let edited = edit(std::slice::from_ref(&moved), &moved, next);

        assert_eq!(edited, Err(EditError::EndInFuture));
    }

    #[test]
    fn edit_errors_read_as_sentences() {
        let (_, noon) = morning_and_noon();
        let messages = [
            (EditError::Gone, "The block is no longer in the log."),
            (
                EditError::EndNotAfterStart,
                "The end must be after the start.",
            ),
            (
                EditError::TooShort(3),
                "A block must be at least 3 minutes long.",
            ),
            (EditError::EndInFuture, "The end cannot be in the future."),
            (
                EditError::RunningStartsAfterNow,
                "A running block must start before now.",
            ),
            (
                EditError::Overlaps(noon.block.span),
                "It overlaps the block 26/10/30 12:00 - 26/10/30 13:00.",
            ),
            (
                EditError::HeadingInEntry,
                "A log entry cannot hold a section or block heading line.",
            ),
            (
                EditError::SecondRunning,
                "A note can have only one running block.",
            ),
        ];

        for (error, message) in messages {
            assert_eq!(error.to_string(), message);
        }
    }

    #[test]
    fn edit_refuses_an_entry_line_that_would_end_the_log_or_start_a_block() {
        let (morning, _) = morning_and_noon();

        for entry in [
            "Done.\n### Next",
            "# Title",
            "#### 26/10/30 11:00 -",
            "#### 26/10/30 11:00 - 26/10/30 12:00 ",
        ] {
            let next = block("A", "09:00", Some("10:00"), entry);
            let edited = edit(std::slice::from_ref(&morning), &morning, next);
            assert_eq!(edited, Err(EditError::HeadingInEntry), "{entry}");
        }
    }

    #[test]
    fn edit_keeps_heading_like_lines_that_stay_entry_text() {
        let (morning, _) = morning_and_noon();

        for entry in ["#### not a time", "####no space", "```\n### fenced\n```"] {
            let next = block("A", "09:00", Some("10:00"), entry);
            let edited = edit(std::slice::from_ref(&morning), &morning, next.clone());
            assert_eq!(edited, Ok(vec![next]), "{entry}");
        }
    }

    #[test]
    fn check_block_refuses_an_end_before_the_start_and_keeps_a_zero_length_block() {
        let inverted = closed("A", "09:00", "08:00").block;
        let zero = closed("A", "09:00", "09:00").block;

        assert_eq!(check_block(&inverted), Err(EditError::EndNotAfterStart));
        assert_eq!(check_block(&zero), Ok(()));
    }

    #[test]
    fn edit_keeps_an_old_zero_length_block_whose_span_is_left_alone() {
        let zero = closed("A", "09:00", "09:00");
        let next = block("A", "09:00", Some("09:00"), "Renamed.");

        let edited = edit(std::slice::from_ref(&zero), &zero, next.clone());

        assert_eq!(edited, Ok(vec![next]));
    }

    #[test]
    fn check_running_allows_one_running_block_and_refuses_a_second() {
        let one = NoteBody::parse(
            "### Log\n#### 26/10/30 12:00 -\n#### 26/10/30 09:00 - 26/10/30 10:00\n",
        );
        let two = NoteBody::parse("### Log\n#### 26/10/30 12:00 -\n#### 26/10/30 09:00 -\n");

        assert_eq!(check_running(&one.log), Ok(()));
        assert_eq!(check_running(&two.log), Err(EditError::SecondRunning));
    }

    #[test]
    fn edit_stopping_a_toggle_fixtures_running_block_gives_its_edit_error() {
        for_each_fixture("toggle", |path, text, expected: serde_json::Value| {
            let Some(edit_error) = expected.get("edit_error") else {
                return;
            };

            let body = NoteBody::parse(text);
            let blocks: Vec<NoteBlock> = body
                .log
                .blocks
                .into_iter()
                .map(|block| NoteBlock {
                    note: String::new(),
                    block,
                })
                .collect();
            let running = blocks.iter().find(|block| block.block.span.end.is_none());
            let running = running.expect("a toggle fixture with an edit_error has a running block");
            let iso = expected["now"].as_str().unwrap();
            let now = NaiveDateTime::parse_from_str(iso, ISO_MINUTE).unwrap();

            let mut stopped = running.clone();
            stopped.block.span.end = Some(now);
            let min_minutes = FIXTURE_SETTINGS.block_min_minutes;
            let edited = edit_block(&blocks, running, stopped, now, min_minutes);

            let error = edited.err().map(|error| error.to_string());
            assert_eq!(error.as_deref(), edit_error.as_str(), "{}", path.display());
        });
    }
}
