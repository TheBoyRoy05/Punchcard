import type { Block, Minutes } from "../../wasm/core";

export type Range = "day" | "week";

export type SaveResult = { ok: true } | { ok: false; error: string };

// `running` is the note's state after the toggle.
export type ToggleResult = { ok: true; running: boolean } | { ok: false; error: string };

// `color` indexes the project palette, so a host keeps one project on one color across views.
export interface TaskNote {
  name: string;
  path: string;
  project: string;
  color: number;
  running: boolean;
}

// `notes` covers every note a block references, including completed notes missing from the task list.
export interface LoadedRange {
  blocks: Block[];
  notes: TaskNote[];
}

// Days are logical days as `logicalDay` writes them, and `now()` is wall clock as `wallClock` returns it.
// `blockMinutes` sizes a new block; `blockMinMinutes` is the shortest span an edit may leave.
export interface TimelineHost {
  load(firstDay: string, lastDay: string): Promise<LoadedRange>;
  notes: TaskNote[];
  dayStartHour: number;
  blockMinutes: number;
  blockMinMinutes: number;
  now(): Minutes;
  save(original: Block | null, block: Block | null): Promise<SaveResult>;
  // Starts the note's block at now, or stops it when it is running.
  toggle(path: string): Promise<ToggleResult>;
  openNote(path: string): void;
}
