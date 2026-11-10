import { expect, test } from "vitest";

import type { Opening, PunchLoadedRange } from "../../src/app/punch";
import punch from "../fixtures/punch.json" with { type: "json" };

// punch's timeline.rs test pins this JSON to its own structs, and `npm test` runs tsc, which fails here when the two sides drift apart.
const opening: Opening = { ...punch.opening, range: punch.opening.range as Opening["range"] };
const loaded: PunchLoadedRange = punch.loaded;

test("the page types read the JSON punch writes", () => {
  // A JSON import types `range` as any string, so its value is checked here instead.
  expect(["day", "week"]).toContain(opening.range);
  expect(opening.notes[0]?.url).toBe(loaded.notes[0]?.url);
  expect(loaded.blocks[0]?.span.end).toBeNull();
});
