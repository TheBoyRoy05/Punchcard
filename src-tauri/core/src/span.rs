use std::fmt;

use chrono::{NaiveDateTime, TimeDelta};
use serde::{Deserialize, Serialize};

use crate::time::{format_time, parse_time};

// A block time comes from a heading, so it must have a two-digit year.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    #[serde(with = "crate::wire::minutes")]
    pub start: NaiveDateTime,
    #[serde(with = "crate::wire::optional_minutes")]
    pub end: Option<NaiveDateTime>,
}

// A ClosedSpan is a day window or a span clipped to one, so its end may reach 2100.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClosedSpan {
    #[serde(with = "crate::wire::window_minutes")]
    pub start: NaiveDateTime,
    #[serde(with = "crate::wire::window_minutes")]
    pub end: NaiveDateTime,
}

impl Span {
    pub fn parse(text: &str) -> Option<Self> {
        let (start, end) = match text.strip_suffix(" -") {
            Some(start) => (start, None),
            None => {
                let (start, end) = text.split_once(" - ")?;
                (start, Some(parse_time(end)?))
            }
        };
        Some(Self {
            start: parse_time(start)?,
            end,
        })
    }

    // A running span has no end yet, so it overlaps everything after its start.
    pub fn overlaps(&self, other: &Self) -> bool {
        let before_end =
            |time: NaiveDateTime, end: Option<NaiveDateTime>| end.is_none_or(|end| time < end);
        before_end(self.start, other.end) && before_end(other.start, self.end)
    }
}

impl fmt::Display for Span {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let start = format_time(self.start);
        match self.end {
            Some(end) => write!(formatter, "{start} - {}", format_time(end)),
            None => write!(formatter, "{start} -"),
        }
    }
}

impl From<ClosedSpan> for Span {
    fn from(ClosedSpan { start, end }: ClosedSpan) -> Self {
        Self {
            start,
            end: Some(end),
        }
    }
}

pub fn shorter_than(start: NaiveDateTime, end: NaiveDateTime, min_minutes: u32) -> bool {
    end - start < TimeDelta::minutes(i64::from(min_minutes))
}

// Parallel timers are real, so overlapping spans count once. A running span counts nothing until it stops.
pub fn worked_hours(spans: impl IntoIterator<Item = Span>) -> f64 {
    let mut closed: Vec<ClosedSpan> = spans
        .into_iter()
        .filter_map(|Span { start, end }| Some(ClosedSpan { start, end: end? }))
        .collect();
    closed.sort_by_key(|span| span.start);

    let mut minutes = 0;
    let mut counted_to: Option<NaiveDateTime> = None;
    for span in closed {
        let from = counted_to.map_or(span.start, |counted| counted.max(span.start));
        if span.end > from {
            minutes += (span.end - from).num_minutes();
            counted_to = Some(span.end);
        }
    }

    let hours = minutes as f64 / 60.0;
    (hours * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(text: &str) -> Span {
        Span::parse(text).unwrap()
    }

    #[test]
    fn closed_and_running_spans_round_trip() {
        for text in ["26/10/30 23:33 - 26/10/31 00:58", "26/10/30 09:05 -"] {
            assert_eq!(span(text).to_string(), text);
        }
        assert_eq!(span("26/10/30 09:05 -").end, None);
    }

    #[test]
    fn text_that_is_not_a_span_is_refused() {
        for text in [
            "yesterday afternoon",
            "26/10/30 09:05",
            "26/10/30 09:05 - later",
            "26/02/30 09:00 -",
        ] {
            assert_eq!(Span::parse(text), None, "{text}");
        }
    }

    #[test]
    fn spans_that_only_touch_do_not_overlap() {
        let morning = span("26/10/30 09:00 - 26/10/30 10:00");

        assert!(!morning.overlaps(&span("26/10/30 10:00 - 26/10/30 11:00")));
        assert!(morning.overlaps(&span("26/10/30 09:59 - 26/10/30 11:00")));
    }

    #[test]
    fn running_span_overlaps_everything_after_its_start() {
        let running = span("26/10/30 09:00 -");

        assert!(running.overlaps(&span("26/10/30 12:00 - 26/10/30 13:00")));
        assert!(!running.overlaps(&span("26/10/30 08:00 - 26/10/30 09:00")));
    }

    #[test]
    fn shorter_than_counts_the_minimum_itself_as_long_enough() {
        let start = parse_time("26/10/30 09:00").unwrap();
        let (too_soon, at_the_minimum) =
            (start + TimeDelta::minutes(2), start + TimeDelta::minutes(3));

        assert!(shorter_than(start, too_soon, 3));
        assert!(!shorter_than(start, at_the_minimum, 3));
    }

    #[test]
    fn hours_round_to_two_decimals_and_skip_the_running_span() {
        let spans = [
            span("26/10/30 09:00 - 26/10/30 09:25"),
            span("26/10/30 10:00 -"),
        ];

        assert_eq!(worked_hours(spans), 0.42);
    }

    #[test]
    fn hours_sum_before_rounding_to_two_decimals() {
        let spans = [
            span("26/10/29 10:00 - 26/10/29 10:20"),
            span("26/10/28 10:00 - 26/10/28 10:20"),
        ];

        assert_eq!(worked_hours(spans), 0.67);
    }

    #[test]
    fn hours_count_overlapping_spans_once() {
        let spans = [
            span("26/10/30 09:00 - 26/10/30 11:00"),
            span("26/10/30 10:00 - 26/10/30 10:30"),
            span("26/10/30 10:30 - 26/10/30 12:00"),
            span("26/10/30 13:00 - 26/10/30 13:30"),
        ];

        assert_eq!(worked_hours(spans), 3.5);
    }
}
