import {
  dayWindow,
  EPOCH_WEEKDAY_INDEX,
  logicalDay,
  workedHours,
  type Block,
  type ClosedSpan,
  type Minutes,
  type Span,
} from "../../wasm/core";

export type DragMode = "start" | "end" | "move";

// `full` reads "Monday 2 November", `date` "2 Nov" and `weekday` "Mon 2".
export type DayStyle = "full" | "date" | "weekday";

const SNAP_MINUTES = 5;
const MINUTES_PER_DAY = 1440;
// The colon is optional so a value pasted from a note still reads.
const CLOCK_SHAPE = /^(\d{2}):?(\d{2})$/;
const DAYS_PER_WEEK = 7;
const WEEKDAYS = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
const MONTHS = [
  "January",
  "February",
  "March",
  "April",
  "May",
  "June",
  "July",
  "August",
  "September",
  "October",
  "November",
  "December",
];

export function snap(time: Minutes): Minutes {
  return Math.round(time / SNAP_MINUTES) * SNAP_MINUTES;
}

export function percentOf(time: Minutes, window: ClosedSpan): number {
  return ((time - window.start) / MINUTES_PER_DAY) * 100;
}

// The result is fractional, so a caller snaps it before it becomes a block time.
export function pixelsToMinutes(pixels: number, width: number): number {
  return (pixels / width) * MINUTES_PER_DAY;
}

// The edge being dragged snaps, and an edge never crosses the other, so a drag cannot invert a block.
export function dragSpan({ start, end }: Span, mode: DragMode, shift: number): Span {
  if (mode === "move") {
    const moved = snap(start + shift);
    return { start: moved, end: end === null ? null : moved + end - start };
  }

  if (mode === "start") {
    const moved = snap(start + shift);
    return { start: end === null ? moved : Math.min(moved, end - SNAP_MINUTES), end };
  }

  const moved = snap((end ?? start) + shift);
  return { start, end: Math.max(moved, start + SNAP_MINUTES) };
}

// The spec's Day Boundary rule: a block counts toward the logical day it starts on, whole, even past the day start.
export function loggedHours(blocks: Block[], days: string[], dayStartHour: number, now: Minutes): number {
  const spans = blocks
    .filter((block) => days.includes(logicalDay(block.span.start, dayStartHour)))
    .map(({ span }) => ({ start: span.start, end: span.end ?? now }));
  return workedHours(spans);
}

export function formatHours(hours: number): string {
  const minutes = Math.round(hours * 60);
  return `${Math.floor(minutes / 60)}h ${twoDigits(minutes % 60)}m`;
}

export function formatClock(time: Minutes): string {
  const [hours, minutes] = clockFields(time);
  return `${hours}:${minutes}`;
}

// A clock time names the one moment of that logical day it falls in, so 01:30 is the night after, not the morning before.
export function parseClock(text: string, day: string, dayStartHour: number): Minutes | null {
  const clock = parseMinuteOfDay(text);
  if (clock === null) {
    return null;
  }

  const sinceDayStart = (clock - dayStartHour * 60 + MINUTES_PER_DAY) % MINUTES_PER_DAY;
  return dayWindow(day, dayStartHour).start + sinceDayStart;
}

// Only a block that already crossed midnight rolls its end to the next date, so a typo is refused, not stretched by a day.
export function parseEnd(text: string, start: Minutes, original: ClosedSpan): Minutes | null {
  const clock = parseMinuteOfDay(text);
  if (clock === null) {
    return null;
  }

  const sameDate = calendarDay(start) * MINUTES_PER_DAY + clock;
  const crossedMidnight = calendarDay(original.end) > calendarDay(original.start);
  return sameDate <= start && crossedMidnight ? sameDate + MINUTES_PER_DAY : sameDate;
}

// The popup's fields read "1450", which is quicker to type than the notes' "14:50".
export function formatClockDigits(time: Minutes): string {
  return clockFields(time).join("");
}

export function shiftDay(day: string, days: number, dayStartHour: number): string {
  return logicalDay(dayWindow(day, dayStartHour).start + days * MINUTES_PER_DAY, dayStartHour);
}

export function weekOf(day: string, dayStartHour: number): string[] {
  const sinceMonday = weekdayOf(day, dayStartHour);
  const monday = shiftDay(day, -sinceMonday, dayStartHour);
  return Array.from({ length: DAYS_PER_WEEK }, (_, index) => shiftDay(monday, index, dayStartHour));
}

export function formatDay(day: string, dayStartHour: number, style: DayStyle): string {
  const [, month = 1, date = 1] = day.split("-").map(Number);
  const weekday = WEEKDAYS[weekdayOf(day, dayStartHour)] ?? "";
  const monthName = MONTHS[month - 1] ?? "";

  switch (style) {
    case "full":
      return `${weekday} ${date} ${monthName}`;
    case "date":
      return `${date} ${monthName.slice(0, 3)}`;
    case "weekday":
      return `${weekday.slice(0, 3)} ${date}`;
  }
}

function weekdayOf(day: string, dayStartHour: number): number {
  const daysSinceEpoch = calendarDay(dayWindow(day, dayStartHour).start);
  return (daysSinceEpoch + EPOCH_WEEKDAY_INDEX) % DAYS_PER_WEEK;
}

function calendarDay(time: Minutes): number {
  return Math.floor(time / MINUTES_PER_DAY);
}

function parseMinuteOfDay(text: string): number | null {
  const fields = CLOCK_SHAPE.exec(text.trim())?.slice(1).map(Number);
  if (fields === undefined) {
    return null;
  }

  const [hour = 0, minute = 0] = fields;
  return hour > 23 || minute > 59 ? null : hour * 60 + minute;
}

function clockFields(time: Minutes): [string, string] {
  const clock = time % MINUTES_PER_DAY;
  return [twoDigits(Math.floor(clock / 60)), twoDigits(clock % 60)];
}

function twoDigits(value: number): string {
  return String(value).padStart(2, "0");
}
