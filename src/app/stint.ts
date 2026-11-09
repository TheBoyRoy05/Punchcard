import type { LoadedRange, Range, TaskNote } from "../core/timeline/host";

// What stint's timeline window hands the page, serialized by the same-named structs in src-tauri/src/timeline.rs.
// tests/app/stint.test.ts checks these against JSON that a stint test writes, so the two sides cannot drift apart.

// `url` is the note's Obsidian link, built by stint so it matches the links its notifications open.
export interface StintNote extends TaskNote {
  url: string;
}

export interface StintLoadedRange extends LoadedRange {
  notes: StintNote[];
}

// Written by stint's init script before the page runs.
export interface Opening {
  range: Range;
  notes: StintNote[];
  dayStartHour: number;
  blockMinutes: number;
  blockMinMinutes: number;
}
