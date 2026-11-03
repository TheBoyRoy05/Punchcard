use std::collections::BTreeSet;
use std::ops::RangeInclusive;

use chrono::{NaiveDate, NaiveDateTime, NaiveTime, TimeDelta};

use crate::span::{ClosedSpan, Span};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DayStart(u32);

impl DayStart {
    const LAST_HOUR: u32 = 23;
    pub const HOURS: RangeInclusive<u32> = 0..=Self::LAST_HOUR;

    pub const fn new(hour: u32) -> Option<Self> {
        if hour <= Self::LAST_HOUR {
            Some(Self(hour))
        } else {
            None
        }
    }

    fn offset(self) -> TimeDelta {
        TimeDelta::hours(i64::from(self.0))
    }
}

// A day runs from its start hour to the same hour next morning, so 01:30 still belongs to the night before.
pub fn logical_day(time: NaiveDateTime, day_start: DayStart) -> NaiveDate {
    (time - day_start.offset()).date()
}

pub fn day_window(day: NaiveDate, day_start: DayStart) -> ClosedSpan {
    let start = day.and_time(NaiveTime::MIN) + day_start.offset();
    ClosedSpan {
        start,
        end: start + TimeDelta::days(1),
    }
}

// A running span is drawn up to now. In the repeated fall-back hour it can start after now, and then draws nothing.
pub fn clip(span: Span, window: ClosedSpan, now: NaiveDateTime) -> Option<ClosedSpan> {
    let drawn = ClosedSpan {
        start: span.start,
        end: span.end.unwrap_or(now),
    };
    if drawn.end <= drawn.start || !Span::from(drawn).overlaps(&window.into()) {
        return None;
    }

    Some(ClosedSpan {
        start: drawn.start.max(window.start),
        end: drawn.end.min(window.end),
    })
}

pub fn worked_days(spans: impl IntoIterator<Item = Span>, day_start: DayStart) -> Vec<NaiveDate> {
    let days: BTreeSet<NaiveDate> = spans
        .into_iter()
        .map(|span| logical_day(span.start, day_start))
        .collect();
    days.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::{format_day, parse_day, parse_time};

    const DAY_START: DayStart = DayStart::new(4).unwrap();
    const NOW: &str = "26/11/02 12:00";

    fn time(text: &str) -> NaiveDateTime {
        parse_time(text).unwrap()
    }

    fn day_of(text: &str) -> String {
        format_day(logical_day(time(text), DAY_START)).unwrap()
    }

    fn window(day: &str) -> ClosedSpan {
        day_window(parse_day(day).unwrap(), DAY_START)
    }

    fn span(text: &str) -> Span {
        Span::parse(text).unwrap()
    }

    fn clipped(text: &str, day: &str) -> Option<String> {
        let clipped = clip(span(text), window(day), time(NOW))?;
        Some(Span::from(clipped).to_string())
    }

    #[test]
    fn a_minute_before_the_day_start_is_the_night_before() {
        assert_eq!(day_of("26/11/02 03:59"), "26-11-01");
    }

    #[test]
    fn the_day_start_itself_is_the_new_day() {
        assert_eq!(day_of("26/11/02 04:00"), "26-11-02");
    }

    #[test]
    fn the_night_before_steps_back_across_a_month() {
        assert_eq!(day_of("26/11/01 01:30"), "26-10-31");
    }

    #[test]
    fn window_runs_from_the_start_hour_to_the_same_hour_next_morning() {
        assert_eq!(
            Span::from(window("26-10-31")).to_string(),
            "26/10/31 04:00 - 26/11/01 04:00"
        );
    }

    #[test]
    fn clip_cuts_a_block_crossing_the_day_start_at_the_window_edge() {
        let text = "26/11/01 03:30 - 26/11/01 04:30";

        assert_eq!(
            clipped(text, "26-10-31").as_deref(),
            Some("26/11/01 03:30 - 26/11/01 04:00")
        );
        assert_eq!(
            clipped(text, "26-11-01").as_deref(),
            Some("26/11/01 04:00 - 26/11/01 04:30")
        );
    }

    #[test]
    fn clip_draws_a_running_block_up_to_now() {
        assert_eq!(
            clipped("26/11/02 10:00 -", "26-11-02").as_deref(),
            Some("26/11/02 10:00 - 26/11/02 12:00")
        );
    }

    #[test]
    fn clip_skips_a_block_outside_the_window() {
        assert_eq!(clipped("26/11/02 04:00 - 26/11/02 05:00", "26-11-01"), None);
    }

    #[test]
    fn clip_skips_a_running_block_that_starts_after_now() {
        assert_eq!(clipped("26/11/02 12:30 -", "26-11-02"), None);
    }

    #[test]
    fn day_start_is_an_hour_of_the_day() {
        assert!(DayStart::new(0).is_some());
        assert!(DayStart::new(23).is_some());
        assert_eq!(DayStart::new(24), None);
    }

    #[test]
    fn worked_days_lists_each_logical_day_once_sorted() {
        let spans = [
            span("26/11/02 01:00 -"),
            span("26/11/01 20:00 - 26/11/01 21:00"),
            span("26/10/30 09:00 - 26/10/30 10:00"),
            span("26/10/30 11:00 - 26/10/30 12:00"),
        ];

        let days: Vec<String> = worked_days(spans, DAY_START)
            .into_iter()
            .map(|day| format_day(day).unwrap())
            .collect();

        assert_eq!(days, ["26-10-30", "26-11-01"]);
    }
}
