pub mod day;
pub mod edit;
pub mod span;
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;
pub mod time;

use std::iter;
use std::ops::{Range, RangeInclusive};

use span::Span;

pub const LOG_HEADING: &str = "### Log";
pub const BLOCK_HEADING: &str = "#### ";
const FENCES: [&str; 2] = ["```", "~~~"];
// Heading levels that start a section, so a `####` block heading stays inside the log.
const SECTION_LEVELS: RangeInclusive<usize> = 1..=3;

#[derive(Debug, Default)]
pub struct Log {
    pub blocks: Vec<Block>,
    pub bad_headings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub span: Span,
    pub entry: String,
}

#[derive(Debug, Default)]
pub struct NoteBody {
    // The body up to the first block heading, so it keeps the `### Log` line and any stray text under it.
    pub before: String,
    pub log: Log,
}

impl Log {
    pub fn parse(lines: &[String]) -> Self {
        let mut log = Self::default();
        let Some(heading) = log_heading(lines) else {
            return log;
        };

        for line in &lines[heading + 1..body_end(lines)] {
            if let Some(block) = Block::parse(line) {
                log.blocks.push(block);
                continue;
            }

            if is_block_marker(line.trim_end()) {
                log.bad_headings.push(line.clone());
            }
            if let Some(block) = log.blocks.last_mut() {
                block.entry.push_str(line);
                block.entry.push('\n');
            }
        }

        for block in &mut log.blocks {
            block.entry.pop();
        }
        log
    }

    pub fn running(&self) -> bool {
        self.running_blocks().next().is_some()
    }

    pub fn running_blocks(&self) -> impl Iterator<Item = &Block> {
        self.blocks.iter().filter(|block| block.span.end.is_none())
    }

    pub fn spans(&self) -> impl Iterator<Item = Span> + '_ {
        self.blocks.iter().map(|block| block.span)
    }
}

impl Block {
    pub fn parse(line: &str) -> Option<Self> {
        let span = Span::parse(line.trim_end().strip_prefix(BLOCK_HEADING)?)?;
        Some(Self {
            span,
            entry: String::new(),
        })
    }

    pub fn heading(&self) -> String {
        format!("{BLOCK_HEADING}{}", self.span)
    }
}

impl NoteBody {
    pub fn parse(text: &str) -> Self {
        let lines: Vec<String> = text.split('\n').map(String::from).collect();
        let log = Log::parse(&lines);

        let body = &lines[..body_end(&lines)];
        let before_end = log_heading(body).map_or(body.len(), |heading| {
            let first_block = log_blocks(body, heading).next();
            first_block.map_or(body.len(), |(index, _)| index)
        });

        Self {
            before: body[..before_end].join("\n"),
            log,
        }
    }

    pub fn format(&self) -> String {
        if self.log.blocks.is_empty() {
            return format!("{}\n", self.before);
        }

        let file = match self.before.as_str() {
            "" => String::new(),
            before => format!("{before}\n"),
        };
        let mut lines: Vec<String> = file.split('\n').map(String::from).collect();

        let mut oldest_first: Vec<&Block> = self.log.blocks.iter().collect();
        oldest_first.sort_by_key(|block| block.span.start);
        for block in oldest_first {
            insert_block(&mut lines, block);
        }
        lines.join("\n")
    }
}

// The log is the note's last section, so a `### Log` inside an example earlier in the note is not it.
pub fn log_heading(lines: &[String]) -> Option<usize> {
    let last_section = *section_headings(lines).last()?;
    (lines[last_section] == LOG_HEADING).then_some(last_section)
}

// A later section hides a `### Log`, so its blocks drop out of the totals and toggle would start a second log.
pub fn has_buried_log(lines: &[String]) -> bool {
    let sections = section_headings(lines);
    let is_log = |index: &usize| lines[*index] == LOG_HEADING;
    sections
        .split_last()
        .is_some_and(|(last, earlier)| !is_log(last) && earlier.iter().any(is_log))
}

// The file's final newline ends the file, not the last entry.
pub fn body_end(lines: &[String]) -> usize {
    let trailing_newline = lines.last().is_some_and(String::is_empty);
    lines.len() - usize::from(trailing_newline)
}

pub fn log_blocks(lines: &[String], log: usize) -> impl Iterator<Item = (usize, Block)> + '_ {
    let blocks = lines.iter().enumerate().skip(log + 1);
    blocks.filter_map(|(index, line)| Some((index, Block::parse(line)?)))
}

pub fn entry_lines(lines: &[String], heading: usize) -> Range<usize> {
    let first = heading + 1;
    let end = body_end(lines);
    let next_heading = lines[first..end]
        .iter()
        .position(|line| Block::parse(line).is_some());
    first..next_heading.map_or(end, |offset| first + offset)
}

pub fn insert_block(lines: &mut Vec<String>, block: &Block) {
    let mut block_lines = vec![block.heading()];
    if !block.entry.is_empty() {
        block_lines.extend(block.entry.split('\n').map(String::from));
    }

    let end = body_end(lines);
    let Some(log) = log_heading(lines) else {
        let log = iter::once(LOG_HEADING.to_string()).chain(block_lines);
        lines.splice(end..end, log);
        return;
    };

    let first_block = log_blocks(lines, log)
        .next()
        .map_or(end, |(index, _)| index);
    lines.splice(first_block..first_block, block_lines);
}

fn section_headings(lines: &[String]) -> Vec<usize> {
    let mut fence = None;
    let mut sections = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if let Some(marker) = FENCES.into_iter().find(|marker| line.starts_with(marker)) {
            match fence {
                None => fence = Some(marker),
                Some(open) if open == marker => fence = None,
                Some(_) => {}
            }
        } else if fence.is_none() && is_section_heading(line) {
            sections.push(index);
        }
    }
    sections
}

fn is_section_heading(line: &str) -> bool {
    let level = line.len() - line.trim_start_matches('#').len();
    SECTION_LEVELS.contains(&level) && line[level..].starts_with(' ')
}

fn is_block_marker(line: &str) -> bool {
    line.strip_prefix(BLOCK_HEADING.trim_end())
        .is_some_and(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace))
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDateTime;
    use serde::Deserialize;

    use super::*;
    use crate::day::{DayStart, worked_days};
    use crate::span::worked_hours;
    use crate::test_support::{FIXTURE_SETTINGS, ISO_MINUTE, for_each_fixture};
    use crate::time::format_day;

    #[derive(Deserialize)]
    struct Expected {
        blocks: Vec<ExpectedBlock>,
        bad_headings: Vec<String>,
        worked_days: Vec<String>,
        worked_hours: f64,
        formatted: Option<String>,
    }

    #[derive(Deserialize, Debug, PartialEq)]
    struct ExpectedBlock {
        start: String,
        end: Option<String>,
        entry: String,
    }

    fn lines(text: &str) -> Vec<String> {
        text.split('\n').map(String::from).collect()
    }

    fn log(body: &str) -> Log {
        Log::parse(&lines(&format!("---\n---\n{body}")))
    }

    fn headings(log: &Log) -> Vec<String> {
        log.blocks.iter().map(Block::heading).collect()
    }

    fn days(log: &Log) -> Vec<String> {
        let day_start = DayStart::new(FIXTURE_SETTINGS.day_start_hour).unwrap();
        let days = worked_days(log.spans(), day_start).into_iter();
        days.map(|day| format_day(day).unwrap()).collect()
    }

    fn block(heading: &str, entry: &str) -> Block {
        let mut block = Block::parse(&format!("{BLOCK_HEADING}{heading}")).unwrap();
        block.entry = entry.to_string();
        block
    }

    fn formatted(before: &str, blocks: Vec<Block>) -> String {
        let log = Log {
            blocks,
            bad_headings: Vec::new(),
        };
        let before = before.to_string();
        NoteBody { before, log }.format()
    }

    fn fixture_blocks(log: &Log) -> Vec<ExpectedBlock> {
        let iso = |time: NaiveDateTime| time.format(ISO_MINUTE).to_string();
        log.blocks
            .iter()
            .map(|block| ExpectedBlock {
                start: iso(block.span.start),
                end: block.span.end.map(iso),
                entry: block.entry.clone(),
            })
            .collect()
    }

    #[test]
    fn headings_round_trip_through_parse_and_format() {
        let body = "### Log\n#### 26/10/31 00:25 -\n#### 26/10/30 23:33 - 26/10/31 00:40\nentry";

        let log = log(body);

        assert_eq!(
            headings(&log),
            [
                "#### 26/10/31 00:25 -",
                "#### 26/10/30 23:33 - 26/10/31 00:40"
            ]
        );
        assert_eq!(log.blocks[1].entry, "entry");
    }

    #[test]
    fn running_block_has_no_end_and_no_hours() {
        let log = log("### Log\n#### 26/10/31 00:25 -  ");

        assert!(log.running());
        assert_eq!(worked_hours(log.spans()), 0.0);
        assert_eq!(days(&log), ["26-10-30"]);
    }

    #[test]
    fn closed_blocks_are_not_running() {
        assert!(!log("### Log\n#### 26/10/31 00:25 - 26/10/31 01:00").running());
    }

    #[test]
    fn bad_headings_are_reported_and_stay_in_the_entry_above() {
        let log = log("### Log\n#### 26/10/29 10:00 - 26/10/29 11:30\nkept\n#### later -\nmore");

        assert_eq!(log.bad_headings, ["#### later -"]);
        assert_eq!(log.blocks.len(), 1);
        assert_eq!(log.blocks[0].entry, "kept\n#### later -\nmore");
    }

    #[test]
    fn deeper_headings_belong_to_the_entry() {
        let log = log("### Log\n#### 26/10/29 10:00 - 26/10/29 11:30\n##### Notes\n- item");

        assert_eq!(log.blocks[0].entry, "##### Notes\n- item");
    }

    #[test]
    fn only_the_last_exact_log_heading_starts_the_log() {
        let body = "### Log\n#### 26/10/30 09:00 -\n### Log\n#### 26/10/31 09:00 -";

        assert_eq!(headings(&log(body)), ["#### 26/10/31 09:00 -"]);
        assert!(log("### Log \n#### 26/10/31 09:00 -").blocks.is_empty());
    }

    #[test]
    fn an_earlier_log_section_stays_in_before() {
        let body =
            NoteBody::parse("### Log\n#### 26/10/29 09:00 -\n### Log\n#### 26/10/30 09:00 -\n");

        assert_eq!(body.before, "### Log\n#### 26/10/29 09:00 -\n### Log");
        assert_eq!(body.log.blocks.len(), 1);
    }

    #[test]
    fn log_example_before_the_last_section_is_not_the_log() {
        let body = "```md\n### Log\n#### 26/10/30 09:00 -\n```\n### Notes\ntext";

        assert!(log(body).blocks.is_empty());
    }

    #[test]
    fn a_block_added_after_a_log_example_starts_a_real_log() {
        let text = "```md\n### Log\n#### 26/10/30 09:00 -\n```\n### Notes\nText.\n";
        let mut body = NoteBody::parse(text);
        body.log.blocks.push(block("26/10/31 09:00 -", ""));

        assert_eq!(
            body.format(),
            format!("{text}### Log\n#### 26/10/31 09:00 -\n")
        );
    }

    #[test]
    fn log_followed_by_a_section_is_buried() {
        assert!(has_buried_log(&lines(
            "### Log\n#### 26/10/30 09:00 -\n### Notes"
        )));
        assert!(!has_buried_log(&lines("### Notes\n### Log")));
        assert!(!has_buried_log(&lines("```md\n### Log\n```\n### Notes")));
    }

    #[test]
    fn final_newline_is_not_part_of_the_last_entry() {
        let log = log("### Log\n#### 26/10/29 10:00 - 26/10/29 11:30\nentry\n\n");

        assert_eq!(log.blocks[0].entry, "entry\n");
    }

    #[test]
    fn blank_lines_between_blocks_round_trip() {
        let text = "### Log\n#### 26/10/30 10:00 -\nLater.\n\n#### 26/10/30 09:00 -\n";

        assert_eq!(NoteBody::parse(text).format(), text);
    }

    #[test]
    fn impossible_dates_are_bad_headings() {
        let log =
            log("### Log\n#### 26/02/30 10:00 -\n#### 26/10/29 24:00 -\n#### 26/10/29 10:60 -");

        assert!(log.blocks.is_empty());
        assert_eq!(log.bad_headings.len(), 3);
    }

    #[test]
    fn note_without_log_has_no_blocks() {
        let log = log("body\n### Notes");

        assert!(log.blocks.is_empty());
        assert!(days(&log).is_empty());
    }

    #[test]
    fn stray_text_under_the_log_heading_stays_before_the_blocks() {
        let body = NoteBody::parse("intro\n### Log\nstray\n#### 26/10/29 10:00 -\n");

        assert_eq!(body.before, "intro\n### Log\nstray");
        assert_eq!(
            body.format(),
            "intro\n### Log\nstray\n#### 26/10/29 10:00 -\n"
        );
    }

    #[test]
    fn blocks_are_written_newest_first() {
        let blocks = vec![block("26/10/29 09:00 -", ""), block("26/10/30 09:00 -", "")];

        assert_eq!(
            formatted("### Log", blocks),
            "### Log\n#### 26/10/30 09:00 -\n#### 26/10/29 09:00 -\n"
        );
    }

    #[test]
    fn format_adds_a_log_heading_when_blocks_exist_without_one() {
        let hi = || vec![block("26/10/29 10:00 -", "Hi.")];

        assert_eq!(
            formatted("intro", hi()),
            "intro\n### Log\n#### 26/10/29 10:00 -\nHi.\n"
        );
        assert_eq!(formatted("", hi()), "### Log\n#### 26/10/29 10:00 -\nHi.\n");
    }

    #[test]
    fn fixtures_match_their_expected_json() {
        for_each_fixture("", |path, text, expected: Expected| {
            let body = NoteBody::parse(text);

            let name = path.display();
            assert_eq!(fixture_blocks(&body.log), expected.blocks, "{name}");
            assert_eq!(body.log.bad_headings, expected.bad_headings, "{name}");
            assert_eq!(days(&body.log), expected.worked_days, "{name}");
            assert_eq!(
                worked_hours(body.log.spans()),
                expected.worked_hours,
                "{name}"
            );
            let formatted = expected.formatted.as_deref().unwrap_or(text);
            assert_eq!(body.format(), formatted, "{name}");
        });
    }
}
