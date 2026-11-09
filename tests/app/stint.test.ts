import { expect, test } from "vitest";

import type { Opening, StintLoadedRange } from "../../src/app/stint";
import stint from "../fixtures/stint.json" with { type: "json" };

// stint's timeline.rs test pins this JSON to its own structs, and `npm test` runs tsc, which fails here when the two sides drift apart.
const opening: Opening = { ...stint.opening, range: stint.opening.range as Opening["range"] };
const loaded: StintLoadedRange = stint.loaded;

test("the page types read the JSON stint writes", () => {
  // A JSON import types `range` as any string, so its value is checked here instead.
  expect(["day", "week"]).toContain(opening.range);
  expect(opening.notes[0]?.url).toBe(loaded.notes[0]?.url);
  expect(loaded.blocks[0]?.span.end).toBeNull();
});
