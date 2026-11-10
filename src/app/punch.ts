import type { LoadedRange, Range, TaskNote } from "../core/timeline/host";

// What punch's timeline window hands the page, serialized by the same-named structs in src-tauri/src/timeline.rs.
// tests/app/punch.test.ts checks these against JSON that a punch test writes, so the two sides cannot drift apart.

// `url` is the note's Obsidian link, built by punch so it matches the links its notifications open.
export interface PunchNote extends TaskNote {
  url: string;
}

export interface PunchLoadedRange extends LoadedRange {
  notes: PunchNote[];
}

// Written by punch's init script before the page runs.
export interface Opening {
  range: Range;
  notes: PunchNote[];
  dayStartHour: number;
  blockMinutes: number;
  blockMinMinutes: number;
}
