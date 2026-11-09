use std::ops::{Range, RangeBounds};

use chrono::{Datelike, NaiveDate, NaiveDateTime, NaiveTime, TimeDelta, Timelike};

// Each 0 is a digit, so 26/1/2 9:5 is refused.
const TIME_SHAPE: &str = "00/00/00 00:00";
const DAY_SHAPE: &str = "00-00-00";
// Every field is two digits and one separator wide.
const FIELD_WIDTH: usize = 3;
// A two-digit year always means 20YY, where chrono's %y would read 70-99 as 19YY.
const CENTURY: i32 = 2000;
pub const YEARS: Range<i32> = CENTURY..CENTURY + 100;
// Block times cross to the webview as wall-clock minutes since this moment.
pub const EPOCH: NaiveDateTime = NaiveDate::from_ymd_opt(CENTURY, 1, 1)
    .unwrap()
    .and_time(NaiveTime::MIN);

pub fn parse_time(text: &str) -> Option<NaiveDateTime> {
    let [year, month, day, hour, minute] = fields(text, TIME_SHAPE)?;
    date(year, month, day)?.and_hms_opt(hour, minute, 0)
}

// Every block time comes from parse_time or the serde boundary's year check; only a logical day can step outside YEARS.
pub fn format_time(time: NaiveDateTime) -> String {
    debug_assert!(YEARS.contains(&time.year()), "{time} has no two-digit year");
    let (hour, minute) = (time.hour(), time.minute());
    format!("{} {hour:02}:{minute:02}", format_date(time.date(), '/'))
}

pub fn parse_day(text: &str) -> Option<NaiveDate> {
    let [year, month, day] = fields(text, DAY_SHAPE)?;
    date(year, month, day)
}

pub fn format_day(day: NaiveDate) -> Option<String> {
    YEARS.contains(&day.year()).then(|| format_date(day, '-'))
}

// Headings hold minutes, so a `now` with seconds would compare unequal to a heading written this same minute.
pub fn wall_clock(now: NaiveDateTime) -> NaiveDateTime {
    let minute = now.date().and_hms_opt(now.hour(), now.minute(), 0);
    minute.expect("an hour and minute read from a time form a valid time")
}

pub fn to_minutes(time: NaiveDateTime) -> i64 {
    (time - EPOCH).num_minutes()
}

// An i32 count keeps TimeDelta::minutes from panicking on a huge one. A heading has a two-digit year,
// so a block time is in YEARS; only a day window's edge may reach past it.
pub fn block_time(minutes: i64, years: impl RangeBounds<i32>) -> Option<NaiveDateTime> {
    let minutes = i32::try_from(minutes).ok()?;
    let time = EPOCH.checked_add_signed(TimeDelta::minutes(minutes.into()))?;
    years.contains(&time.year()).then_some(time)
}

fn fields<const N: usize>(text: &str, shape: &str) -> Option<[u32; N]> {
    let shaped = text.len() == shape.len()
        && text.bytes().zip(shape.bytes()).all(|(byte, shape)| {
            if shape == b'0' {
                byte.is_ascii_digit()
            } else {
                byte == shape
            }
        });
    if !shaped {
        return None;
    }

    let bytes = text.as_bytes();
    let digit = |at: usize| u32::from(bytes[at] - b'0');
    Some(std::array::from_fn(|field| {
        let at = field * FIELD_WIDTH;
        digit(at) * 10 + digit(at + 1)
    }))
}

fn date(year: u32, month: u32, day: u32) -> Option<NaiveDate> {
    let year = CENTURY + i32::try_from(year).ok()?;
    NaiveDate::from_ymd_opt(year, month, day)
}

fn format_date(day: NaiveDate, separator: char) -> String {
    let (year, month, day) = (day.year() % 100, day.month(), day.day());
    format!("{year:02}{separator}{month:02}{separator}{day:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_round_trips_through_parse_and_format() {
        let time = parse_time("26/10/30 09:05").unwrap();

        assert_eq!(time.to_string(), "2026-10-30 09:05:00");
        assert_eq!(format_time(time), "26/10/30 09:05");
    }

    #[test]
    fn two_digit_years_are_always_this_century() {
        assert_eq!(parse_time("99/01/02 03:04").unwrap().year(), 2099);
    }

    #[test]
    fn time_refuses_other_shapes() {
        for text in [
            "26/1/02 09:05",
            "26/10/30 9:05",
            "26-10-30 09:05",
            "26/10/30 09:05 ",
            "yesterday",
        ] {
            assert_eq!(parse_time(text), None, "{text}");
        }
    }

    #[test]
    fn time_refuses_moments_that_never_existed() {
        for text in [
            "26/02/30 09:00",
            "26/10/30 24:00",
            "26/10/30 09:60",
            "26/13/01 09:00",
            "26/10/00 09:00",
        ] {
            assert_eq!(parse_time(text), None, "{text}");
        }
    }

    #[test]
    fn leap_day_exists_only_in_leap_years() {
        assert!(parse_time("28/02/29 12:00").is_some());
        assert_eq!(parse_time("26/02/29 12:00"), None);
    }

    #[test]
    fn day_round_trips_and_refuses_other_shapes() {
        assert_eq!(
            format_day(parse_day("26-11-02").unwrap()).unwrap(),
            "26-11-02"
        );
        assert_eq!(parse_day("26/11/02"), None);
        assert_eq!(parse_day("26-02-30"), None);
    }

    #[test]
    fn day_outside_the_two_digit_century_is_not_formatted() {
        for year in [1999, 2100] {
            let day = NaiveDate::from_ymd_opt(year, 1, 1).unwrap();
            assert_eq!(format_day(day), None, "{year}");
        }
    }
}
