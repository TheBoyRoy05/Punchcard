use chrono::NaiveDateTime;

use crate::note::Note;

// Smaller weighs recency more than frequency.
const HALF_LIFE_DAYS: f64 = 1.0;
const SECONDS_PER_DAY: f64 = 86_400.0;

pub fn ranked(notes: Vec<Note>, now: NaiveDateTime) -> Vec<Note> {
    let mut scored: Vec<(Note, f64)> = notes
        .into_iter()
        .filter(|note| note.log.running() || note.is_open())
        .map(|note| {
            let score = frecency(&note, now);
            (note, score)
        })
        .collect();

    scored.sort_by(|(a, a_score), (b, b_score)| {
        b.log
            .running()
            .cmp(&a.log.running())
            .then(b_score.total_cmp(a_score))
            .then_with(|| a.due().cmp(b.due()))
            .then_with(|| a.name().cmp(b.name()))
    });
    scored.into_iter().map(|(note, _)| note).collect()
}

fn frecency(note: &Note, now: NaiveDateTime) -> f64 {
    note.log
        .blocks
        .iter()
        .map(|block| {
            let age_days = (now - block.span.start).as_seconds_f64() / SECONDS_PER_DAY;
            0.5_f64.powf(age_days / HALF_LIFE_DAYS)
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use punchcard_core::time::parse_time;

    fn note(name: &str, frontmatter: &str, log: &str) -> Note {
        let text = format!("---\n{frontmatter}\n---\n### Log\n{log}");
        let lines: Vec<String> = text.split('\n').map(String::from).collect();
        Note::from_lines(PathBuf::from(format!("Tasks/{name}.md")), &lines).unwrap()
    }

    fn open_task(name: &str, extra: &str, log: &str) -> Note {
        note(
            name,
            &format!("tags:\n  - \"#Task\"\nstatus: In Progress\n{extra}"),
            log,
        )
    }

    fn names(notes: &[Note]) -> Vec<&str> {
        notes.iter().map(Note::name).collect()
    }

    fn now() -> NaiveDateTime {
        parse_time("26/11/02 12:00").unwrap()
    }

    #[test]
    fn running_note_ranks_first_even_when_closed() {
        let notes = vec![
            open_task("Busy", "", "#### 26/11/02 09:00 - 26/11/02 11:00"),
            note("Done", "status: Done", "#### 26/10/01 09:00 -"),
        ];

        assert_eq!(names(&ranked(notes, now())), ["Done", "Busy"]);
    }

    #[test]
    fn recent_work_outranks_more_but_older_work() {
        let older = "#### 26/10/29 09:00 - 26/10/29 10:00\n#### 26/10/28 09:00 - 26/10/28 10:00";
        let notes = vec![
            open_task("Older", "", older),
            open_task("Recent", "", "#### 26/11/02 09:00 - 26/11/02 10:00"),
        ];

        assert_eq!(names(&ranked(notes, now())), ["Recent", "Older"]);
    }

    #[test]
    fn ties_break_by_due_then_name() {
        let notes = vec![
            open_task("B No Due", "", ""),
            open_task("A No Due", "", ""),
            open_task("Due", "due: 2026-11-05", ""),
        ];

        assert_eq!(
            names(&ranked(notes, now())),
            ["Due", "A No Due", "B No Due"]
        );
    }

    #[test]
    fn closed_and_untimed_notes_are_left_out() {
        let notes = vec![
            note("Done", "tags:\n  - \"#Task\"\nstatus: Done", ""),
            note("Idea", "tags:\n  - \"#Idea\"\nstatus: In Progress", ""),
            note(
                "Project",
                "tags:\n  - \"#Project/Side\"\nstatus: Not Started",
                "",
            ),
        ];

        assert_eq!(names(&ranked(notes, now())), ["Project"]);
    }

    #[test]
    fn bad_heading_adds_no_score() {
        let notes = vec![
            open_task("B Worked", "", "#### 26/11/02 09:00 - 26/11/02 10:00"),
            open_task("A Bad", "", "#### yesterday -"),
        ];

        assert_eq!(names(&ranked(notes, now())), ["B Worked", "A Bad"]);
    }
}
