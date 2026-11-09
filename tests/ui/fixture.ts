import type { TaskNote } from "../../src/core/timeline/host";
import type { Block } from "../../src/wasm/core";
import { at } from "../time";

export interface SavedBlock {
  note: string;
  span: string;
  entry: string;
}

export interface HarnessState {
  saves: { original: SavedBlock | null; block: SavedBlock | null }[];
  opened: string[];
  // The index in `saves` of a write the host refuses, as a full disk would.
  refused: number | null;
  // How long each save takes, so a test can act while one is still in flight.
  saveMs: number;
}

declare global {
  interface Window {
    punchcard: HarnessState;
  }
}

export const HARNESS = "/tests/ui/harness.html";
export const NOW = at("11-02 18:09");
export const DAY_START_HOUR = 4;
export const BLOCK_MINUTES = 30;
export const BLOCK_MIN_MINUTES = 3;

export const NOTES: TaskNote[] = [
  { name: "Port Task Timer to Rust Stint", path: "Tasks/Port Task Timer.md", project: "Punchcard", color: 0, running: false },
  { name: "Design Timeline Look", path: "Tasks/Design Timeline Look.md", project: "Punchcard", color: 0, running: true },
  { name: "Serialize Hold per Domain", path: "Tasks/Serialize Hold.md", project: "Knowledge Anchor", color: 1, running: false },
  { name: "Knowledge Anchor", path: "Projects/Knowledge Anchor.md", project: "Knowledge Anchor", color: 1, running: false },
  { name: "PA5 Writeup", path: "Tasks/PA5 Writeup.md", project: "CSE 291", color: 2, running: false },
  { name: "Review Gate", path: "Tasks/Review Gate.md", project: "Knowledge Anchor", color: 1, running: false },
];

// A completed note: `load` returns it for its block, but the picker list leaves it out.
export const COMPLETED: TaskNote = { name: "Archive Old Spans", path: "Tasks/Archive Old Spans.md", project: "Vault", color: 3, running: false };

export const BLOCKS: Block[] = [
  block("Tasks/Port Task Timer.md", "11-02 09:30", "11-02 11:45", "Ported the toggle."),
  block("Tasks/Port Task Timer.md", "11-02 13:00", "11-02 14:43"),
  block("Tasks/Design Timeline Look.md", "11-02 17:25", null),
  block("Tasks/Serialize Hold.md", "11-02 07:15", "11-02 08:55"),
  block("Tasks/Serialize Hold.md", "11-02 15:00", "11-02 17:05", "Hold now keyed by domain."),
  block("Projects/Knowledge Anchor.md", "11-02 11:30", "11-02 12:25", "Planning."),
  block("Tasks/PA5 Writeup.md", "10-28 19:00", "10-29 02:10"),
  block("Tasks/Review Gate.md", "10-27 13:00", "10-27 17:30"),
  block("Tasks/Port Task Timer.md", "11-01 19:30", "11-01 23:55"),
  block("Tasks/Serialize Hold.md", "10-29 10:00", "10-29 10:20"),
  block("Tasks/PA5 Writeup.md", "10-30 02:30", "10-30 05:00"),
  // Inside Port Task Timer's 09:30-11:45, so the day's union hours stay the same.
  block(COMPLETED.path, "11-02 10:00", "11-02 10:30"),
  // A few pixels wide, so it draws without edge handles. Also inside Port Task Timer's span.
  block(COMPLETED.path, "11-02 10:40", "11-02 10:45"),
];

function block(note: string, start: string, end: string | null, entry = ""): Block {
  return { note, span: { start: at(start), end: end === null ? null : at(end) }, entry };
}
