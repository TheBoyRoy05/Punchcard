use std::ops::{RangeBounds, RangeInclusive};

use chrono::{Datelike, NaiveDate, NaiveDateTime, NaiveTime, TimeDelta};
use punchcard_core::day::{self, DayStart};
use punchcard_core::edit::{self, NoteBlock};
use punchcard_core::span::{self, ClosedSpan, Span};
use punchcard_core::time::{self, YEARS, format_day, parse_day};
use punchcard_core::{Block, Log, NoteBody};
use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize, Serializer};
use wasm_bindgen::prelude::*;

#[wasm_bindgen(typescript_custom_section)]
const TYPES: &str = r#"
/** Wall-clock minutes since 2000-01-01 00:00, with no time zone. */
export type Minutes = number;
export interface Span { start: Minutes; end: Minutes | null }
export interface ClosedSpan { start: Minutes; end: Minutes }
export interface Block { note: string; span: Span; entry: string }
export interface Log { before: string; blocks: Block[] }
export interface ParsedLog extends Log { badHeadings: string[] }
export type EditResult = { ok: true; blocks: Block[] } | { ok: false; error: string };
"#;

const EPOCH: NaiveDateTime = NaiveDate::from_ymd_opt(2000, 1, 1)
    .unwrap()
    .and_time(NaiveTime::MIN);
// The window of 99-12-31 ends in 2100, and clip must still take it back.
const WINDOW_YEARS: RangeInclusive<i32> = YEARS.start..=YEARS.end;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "Span")]
    pub type JsSpan;
    #[wasm_bindgen(typescript_type = "Span[]")]
    pub type JsSpans;
    #[wasm_bindgen(typescript_type = "ClosedSpan")]
    pub type JsClosedSpan;
    #[wasm_bindgen(typescript_type = "ClosedSpan | null")]
    pub type JsClipped;
    #[wasm_bindgen(typescript_type = "Block")]
    pub type JsBlock;
    #[wasm_bindgen(typescript_type = "Block[]")]
    pub type JsBlocks;
    #[wasm_bindgen(typescript_type = "Log")]
    pub type JsLog;
    #[wasm_bindgen(typescript_type = "ParsedLog")]
    pub type JsParsedLog;
    #[wasm_bindgen(typescript_type = "EditResult")]
    pub type JsEditResult;
    #[wasm_bindgen(typescript_type = "string[]")]
    pub type JsStrings;
}

#[derive(Serialize, Deserialize)]
struct WireSpan {
    start: f64,
    end: Option<f64>,
}

#[derive(Serialize, Deserialize)]
struct WireClosedSpan {
    start: f64,
    end: f64,
}

#[derive(Serialize, Deserialize)]
struct WireBlock {
    note: String,
    span: WireSpan,
    entry: String,
}

#[derive(Deserialize)]
struct WireLog {
    before: String,
    blocks: Vec<WireBlock>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WireParsedLog {
    before: String,
    blocks: Vec<WireBlock>,
    bad_headings: Vec<String>,
}

enum WireEditResult {
    Edited(Vec<WireBlock>),
    Refused(String),
}

impl Serialize for WireEditResult {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut result = serializer.serialize_struct("EditResult", 2)?;
        match self {
            Self::Edited(blocks) => {
                result.serialize_field("ok", &true)?;
                result.serialize_field("blocks", blocks)?;
            }
            Self::Refused(error) => {
                result.serialize_field("ok", &false)?;
                result.serialize_field("error", error)?;
            }
        }
        result.end()
    }
}

impl WireSpan {
    fn from_core(Span { start, end }: Span) -> Self {
        Self {
            start: to_minutes(start),
            end: end.map(to_minutes),
        }
    }

    fn into_core(self) -> Result<Span, JsError> {
        let end = self.end.map(|end| from_minutes(end, YEARS)).transpose()?;
        Ok(Span {
            start: from_minutes(self.start, YEARS)?,
            end,
        })
    }
}

impl WireClosedSpan {
    fn from_core(ClosedSpan { start, end }: ClosedSpan) -> Self {
        Self {
            start: to_minutes(start),
            end: to_minutes(end),
        }
    }

    fn into_window(self) -> Result<ClosedSpan, JsError> {
        Ok(ClosedSpan {
            start: from_minutes(self.start, WINDOW_YEARS)?,
            end: from_minutes(self.end, WINDOW_YEARS)?,
        })
    }
}

impl WireBlock {
    fn from_core(NoteBlock { note, block }: NoteBlock) -> Self {
        Self {
            note,
            span: WireSpan::from_core(block.span),
            entry: block.entry,
        }
    }

    fn into_core(self) -> Result<NoteBlock, JsError> {
        let block = Block {
            span: self.span.into_core()?,
            entry: self.entry,
        };
        Ok(NoteBlock {
            note: self.note,
            block,
        })
    }
}

#[wasm_bindgen(js_name = parseLog)]
pub fn parse_log(note: &str, body: &str) -> Result<JsParsedLog, JsError> {
    let NoteBody { before, log } = NoteBody::parse(body);
    let blocks = log.blocks.into_iter().map(|block| {
        let note = note.to_string();
        WireBlock::from_core(NoteBlock { note, block })
    });

    let parsed = WireParsedLog {
        before,
        blocks: blocks.collect(),
        bad_headings: log.bad_headings,
    };
    Ok(to_js(&parsed)?.unchecked_into())
}

#[wasm_bindgen(js_name = formatLog)]
pub fn format_log(log: JsLog) -> Result<String, JsError> {
    let WireLog { before, blocks } = from_js(log.into())?;
    let blocks = core_blocks(blocks)?.into_iter().map(|block| block.block);

    let log = Log {
        blocks: blocks.collect(),
        bad_headings: Vec::new(),
    };
    Ok(NoteBody { before, log }.format())
}

#[wasm_bindgen(js_name = editBlock)]
pub fn edit_block(
    blocks: JsBlocks,
    original: JsBlock,
    next: JsBlock,
    now: f64,
    block_min_minutes: u32,
) -> Result<JsEditResult, JsError> {
    let blocks = core_blocks(from_js(blocks.into())?)?;
    let original = from_js::<WireBlock>(original.into())?.into_core()?;
    let next = from_js::<WireBlock>(next.into())?.into_core()?;
    let now = from_minutes(now, YEARS)?;

    let edited = edit::edit_block(&blocks, &original, next, now, block_min_minutes);

    let result = match edited {
        Ok(edited) => {
            WireEditResult::Edited(edited.into_iter().map(WireBlock::from_core).collect())
        }
        Err(error) => WireEditResult::Refused(error.to_string()),
    };
    Ok(to_js(&result)?.unchecked_into())
}

#[wasm_bindgen(js_name = workedHours)]
pub fn worked_hours(spans: JsSpans) -> Result<f64, JsError> {
    Ok(span::worked_hours(core_spans(spans)?))
}

#[wasm_bindgen(js_name = workedDays)]
pub fn worked_days(spans: JsSpans, day_start_hour: f64) -> Result<JsStrings, JsError> {
    let days = day::worked_days(core_spans(spans)?, day_start(day_start_hour)?).into_iter();
    let days: Vec<String> = days.map(day_text).collect::<Result<_, _>>()?;
    Ok(to_js(&days)?.unchecked_into())
}

#[wasm_bindgen(js_name = logicalDay)]
pub fn logical_day(time: f64, day_start_hour: f64) -> Result<String, JsError> {
    let time = from_minutes(time, YEARS)?;
    day_text(day::logical_day(time, day_start(day_start_hour)?))
}

#[wasm_bindgen(js_name = dayWindow)]
pub fn day_window(day: &str, day_start_hour: f64) -> Result<JsClosedSpan, JsError> {
    let date = parse_day(day).ok_or_else(|| JsError::new(&format!("Not a logical day: {day}")))?;
    let window = day::day_window(date, day_start(day_start_hour)?);
    Ok(to_js(&WireClosedSpan::from_core(window))?.unchecked_into())
}

#[wasm_bindgen]
pub fn clip(span: JsSpan, window: JsClosedSpan, now: f64) -> Result<JsClipped, JsError> {
    let span = from_js::<WireSpan>(span.into())?.into_core()?;
    let window = from_js::<WireClosedSpan>(window.into())?.into_window()?;

    let clipped = day::clip(span, window, from_minutes(now, YEARS)?);
    Ok(to_js(&clipped.map(WireClosedSpan::from_core))?.unchecked_into())
}

#[wasm_bindgen]
pub fn overlaps(a: JsSpan, b: JsSpan) -> Result<bool, JsError> {
    let a = from_js::<WireSpan>(a.into())?.into_core()?;
    let b = from_js::<WireSpan>(b.into())?.into_core()?;
    Ok(a.overlaps(&b))
}

#[wasm_bindgen(js_name = formatSpan)]
pub fn format_span(span: JsSpan) -> Result<String, JsError> {
    Ok(from_js::<WireSpan>(span.into())?.into_core()?.to_string())
}

#[wasm_bindgen(js_name = formatTime)]
pub fn format_time(time: f64) -> Result<String, JsError> {
    Ok(time::format_time(from_minutes(time, YEARS)?))
}

#[wasm_bindgen(js_name = parseTime)]
pub fn parse_time(text: &str) -> Result<f64, JsError> {
    let time =
        time::parse_time(text).ok_or_else(|| JsError::new(&format!("Not a block time: {text}")))?;
    Ok(to_minutes(time))
}

fn from_minutes(minutes: f64, years: impl RangeBounds<i32>) -> Result<NaiveDateTime, JsError> {
    let time = whole_number(minutes)
        .and_then(|minutes| EPOCH.checked_add_signed(TimeDelta::minutes(minutes)))
        .filter(|time| years.contains(&time.year()));
    time.ok_or_else(|| JsError::new(&format!("Not a block time: {minutes}")))
}

fn to_minutes(time: NaiveDateTime) -> f64 {
    (time - EPOCH).num_minutes() as f64
}

// A u32 parameter would wrap -1 to 4294967295 and truncate 4.5 to 4, so the hour comes in as a plain number.
fn day_start(hour: f64) -> Result<DayStart, JsError> {
    let day_start = whole_number(hour)
        .and_then(|hour| u32::try_from(hour).ok())
        .and_then(DayStart::new);
    day_start.ok_or_else(|| JsError::new(&format!("Not an hour of the day: {hour}")))
}

fn day_text(day: NaiveDate) -> Result<String, JsError> {
    format_day(day).ok_or_else(|| JsError::new(&format!("{day} has no two-digit year")))
}

fn whole_number(number: f64) -> Option<i64> {
    // The i32 bound keeps TimeDelta::minutes from panicking on a huge count, so do not widen it to i64.
    let whole = number.fract() == 0.0 && number.abs() < f64::from(i32::MAX);
    whole.then_some(number as i64)
}

fn core_blocks(blocks: Vec<WireBlock>) -> Result<Vec<NoteBlock>, JsError> {
    blocks.into_iter().map(WireBlock::into_core).collect()
}

fn core_spans(spans: JsSpans) -> Result<Vec<Span>, JsError> {
    let spans: Vec<WireSpan> = from_js(spans.into())?;
    spans.into_iter().map(WireSpan::into_core).collect()
}

fn from_js<T: for<'de> Deserialize<'de>>(value: JsValue) -> Result<T, JsError> {
    Ok(serde_wasm_bindgen::from_value(value)?)
}

// A running span's end must reach JS as null, not as undefined.
fn to_js<T: Serialize>(value: &T) -> Result<JsValue, JsError> {
    let serializer = serde_wasm_bindgen::Serializer::new().serialize_missing_as_null(true);
    Ok(value.serialize(&serializer)?)
}
