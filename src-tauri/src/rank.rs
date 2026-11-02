use chrono::NaiveDateTime;

use crate::note::{Note, SPAN_FORMAT, SPANS_KEY};
use crate::vault::TimerError;

// Smaller weighs recency more than frequency.
const HALF_LIFE_DAYS: f64 = 1.0;
const SECONDS_PER_DAY: f64 = 86_400.0;

pub fn ranked(notes: Vec<Note>, now: NaiveDateTime) -> Result<Vec<Note>, TimerError> {
    let mut scored = Vec::new();
    for note in notes {
        if note.running() || note.is_open() {
            let score = frecency(&note, now)?;
            scored.push((note, score));
        }
    }

    scored.sort_by(|(a, a_score), (b, b_score)| {
        b.running()
            .cmp(&a.running())
            .then(b_score.total_cmp(a_score))
            .then_with(|| a.due().cmp(b.due()))
            .then_with(|| a.name().cmp(b.name()))
    });
    Ok(scored.into_iter().map(|(note, _)| note).collect())
}

fn frecency(note: &Note, now: NaiveDateTime) -> Result<f64, TimerError> {
    let mut score = 0.0;
    for span in note.list(SPANS_KEY) {
        let start = span.split(" -").next().unwrap_or_default();
        let Ok(start) = NaiveDateTime::parse_from_str(start, SPAN_FORMAT) else {
            let message = format!("{} has a bad {SPANS_KEY} span: {span}", note.name());
            return Err(message.into());
        };

        let age_days = (now - start).as_seconds_f64() / SECONDS_PER_DAY;
        score += 0.5_f64.powf(age_days / HALF_LIFE_DAYS);
    }
    Ok(score)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::note::parse_frontmatter;

    fn note(name: &str, frontmatter: &str) -> Note {
        let text = format!("---\n{frontmatter}\n---");
        let lines: Vec<String> = text.split('\n').map(String::from).collect();
        Note {
            path: PathBuf::from(format!("Tasks/{name}.md")),
            fields: parse_frontmatter(&lines).unwrap(),
        }
    }

    fn open_task(name: &str, extra: &str) -> Note {
        note(
            name,
            &format!("tags:\n  - \"#Task\"\nstatus: In Progress\n{extra}"),
        )
    }

    fn names(notes: &[Note]) -> Vec<&str> {
        notes.iter().map(Note::name).collect()
    }

    fn now() -> NaiveDateTime {
        NaiveDateTime::parse_from_str("26/11/02 12:00", SPAN_FORMAT).unwrap()
    }

    #[test]
    fn running_note_ranks_first_even_when_closed() {
        let notes = vec![
            open_task("Busy", "worked_on:\n  - 26/11/02 09:00 - 26/11/02 11:00"),
            note("Done", "status: Done\nworked_on:\n  - 26/10/01 09:00 -"),
        ];

        assert_eq!(names(&ranked(notes, now()).unwrap()), ["Done", "Busy"]);
    }

    #[test]
    fn recent_work_outranks_more_but_older_work() {
        let older =
            "worked_on:\n  - 26/10/28 09:00 - 26/10/28 10:00\n  - 26/10/29 09:00 - 26/10/29 10:00";
        let notes = vec![
            open_task("Older", older),
            open_task("Recent", "worked_on:\n  - 26/11/02 09:00 - 26/11/02 10:00"),
        ];

        assert_eq!(names(&ranked(notes, now()).unwrap()), ["Recent", "Older"]);
    }

    #[test]
    fn ties_break_by_due_then_name() {
        let notes = vec![
            open_task("B No Due", ""),
            open_task("A No Due", ""),
            open_task("Due", "due: 2026-11-05"),
        ];

        assert_eq!(
            names(&ranked(notes, now()).unwrap()),
            ["Due", "A No Due", "B No Due"]
        );
    }

    #[test]
    fn closed_and_untimed_notes_are_left_out() {
        let notes = vec![
            note("Done", "tags:\n  - \"#Task\"\nstatus: Done"),
            note("Idea", "tags:\n  - \"#Idea\"\nstatus: In Progress"),
            note(
                "Project",
                "tags:\n  - \"#Project/Side\"\nstatus: Not Started",
            ),
        ];

        assert_eq!(names(&ranked(notes, now()).unwrap()), ["Project"]);
    }

    #[test]
    fn bad_span_is_an_error() {
        let notes = vec![open_task("Port", "worked_on:\n  - yesterday -")];

        let error = ranked(notes, now()).unwrap_err();

        assert_eq!(
            error.to_string(),
            "Port has a bad worked_on span: yesterday -"
        );
    }
}
