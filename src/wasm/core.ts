import base64 from "./pkg/bytes";
import { initSync, type Minutes } from "./pkg/punchcard_wasm";

export {
  clip,
  dayWindow,
  editBlock,
  formatLog,
  formatSpan,
  formatTime,
  logicalDay,
  overlaps,
  parseLog,
  workedDays,
  workedHours,
} from "./pkg/punchcard_wasm";
export type { Block, ClosedSpan, EditResult, Log, Minutes, ParsedLog, Span } from "./pkg/punchcard_wasm";

export const EPOCH_MS = Date.UTC(2000, 0, 1);
export const MS_PER_MINUTE = 60_000;

// Obsidian loads only main.js, so the wasm travels inside the bundle and compiles synchronously on import.
initSync({ module: Uint8Array.from(atob(base64), (char) => char.charCodeAt(0)) });

// Block times are zone-free wall clock, so the host's clock is read in its own zone and dropped to the minute.
export function wallClock(date: Date): Minutes {
  const fields = Date.UTC(date.getFullYear(), date.getMonth(), date.getDate(), date.getHours(), date.getMinutes());
  return (fields - EPOCH_MS) / MS_PER_MINUTE;
}
