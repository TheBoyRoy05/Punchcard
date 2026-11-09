// Serde adapters for the JSON the webview reads and writes, declared for TypeScript in the wasm crate's TYPES section.
// Times are whole minutes since time::EPOCH, so both hosts and the wasm build cross the boundary one way.

use std::ops::{RangeBounds, RangeInclusive};

use chrono::NaiveDateTime;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::Block;
use crate::edit::NoteBlock;
use crate::span::Span;
use crate::time::{self, YEARS};

// The day window of 99-12-31 ends in 2100, one year past YEARS.
const WINDOW_YEARS: RangeInclusive<i32> = YEARS.start..=YEARS.end;

// The webview's `Block` names its note beside the block's own fields. `#[serde(flatten)]` would say so on
// NoteBlock itself, but serde-wasm-bindgen then writes a JS Map and cannot read the nested span back.
#[derive(Serialize, Deserialize)]
pub struct FlatBlock {
    note: String,
    span: Span,
    entry: String,
}

impl From<FlatBlock> for NoteBlock {
    fn from(FlatBlock { note, span, entry }: FlatBlock) -> Self {
        Self {
            note,
            block: Block { span, entry },
        }
    }
}

impl From<NoteBlock> for FlatBlock {
    fn from(NoteBlock { note, block }: NoteBlock) -> Self {
        Self {
            note,
            span: block.span,
            entry: block.entry,
        }
    }
}

pub mod minutes {
    use super::*;

    pub fn serialize<S: Serializer>(
        time: &NaiveDateTime,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.serialize_i64(time::to_minutes(*time))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<NaiveDateTime, D::Error> {
        from_minutes(i64::deserialize(deserializer)?, YEARS)
    }
}

pub mod optional_minutes {
    use super::*;

    pub fn serialize<S: Serializer>(
        time: &Option<NaiveDateTime>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match time {
            Some(time) => serializer.serialize_i64(time::to_minutes(*time)),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<NaiveDateTime>, D::Error> {
        let minutes = Option::<i64>::deserialize(deserializer)?;
        minutes
            .map(|minutes| from_minutes(minutes, YEARS))
            .transpose()
    }
}

pub mod window_minutes {
    use super::*;

    pub use super::minutes::serialize;

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<NaiveDateTime, D::Error> {
        from_minutes(i64::deserialize(deserializer)?, WINDOW_YEARS)
    }
}

fn from_minutes<E: serde::de::Error>(
    minutes: i64,
    years: impl RangeBounds<i32>,
) -> Result<NaiveDateTime, E> {
    time::block_time(minutes, years)
        .ok_or_else(|| E::custom(format!("Not a block time: {minutes}")))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::span::ClosedSpan;

    fn block(start: &str, end: Option<&str>) -> NoteBlock {
        let span = Span {
            start: time::parse_time(start).unwrap(),
            end: end.map(|end| time::parse_time(end).unwrap()),
        };
        NoteBlock {
            note: "Tasks/Port.md".to_string(),
            block: Block {
                span,
                entry: "Ported.".to_string(),
            },
        }
    }

    #[test]
    fn a_block_crosses_as_its_note_span_minutes_and_entry_and_comes_back_whole() {
        let running = block("00/01/01 00:05", None);

        let json = serde_json::to_value(&running).unwrap();

        assert_eq!(
            json,
            json!({ "note": "Tasks/Port.md", "span": { "start": 5, "end": null }, "entry": "Ported." })
        );
        assert_eq!(serde_json::from_value::<NoteBlock>(json).unwrap(), running);
    }

    #[test]
    fn a_span_refuses_a_time_without_a_two_digit_year_and_a_window_takes_2100() {
        let past_years = time::to_minutes(time::parse_time("99/12/31 23:59").unwrap()) + 1;

        let error = serde_json::from_value::<Span>(json!({ "start": -1, "end": null }));
        assert_eq!(error.unwrap_err().to_string(), "Not a block time: -1");
        assert!(
            serde_json::from_value::<Span>(json!({ "start": past_years, "end": null })).is_err()
        );
        assert!(
            serde_json::from_value::<ClosedSpan>(json!({ "start": 0, "end": past_years })).is_ok()
        );
        assert!(serde_json::from_value::<Span>(json!({ "start": 1.5, "end": null })).is_err());
    }
}
