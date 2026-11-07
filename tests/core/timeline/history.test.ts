import { describe, expect, it } from "vitest";

import { History, planChanges } from "../../../src/core/timeline/history";
import { editBlock, type Block } from "../../../src/wasm/core";
import { at } from "../../time";

const NOW = at("11-02 18:00");
const BLOCK_MIN_MINUTES = 3;

function block(note: string, start: string, end: string, entry = ""): Block {
  return { note, span: { start: at(`11-02 ${start}`), end: at(`11-02 ${end}`) }, entry };
}

function edited(blocks: Block[], original: Block, next: Block): Block[] {
  const result = editBlock(blocks, original, next, NOW, BLOCK_MIN_MINUTES);
  if (!result.ok) {
    throw new Error(result.error);
  }
  return result.blocks;
}

describe("planChanges", () => {
  it("writes an edit as one replace", () => {
    const morning = block("A.md", "09:00", "10:00");
    const next = block("A.md", "09:00", "11:00");
    expect(planChanges([morning], edited([morning], morning, next))).toEqual([{ original: morning, block: next }]);
  });

  it("adds a moved block to its new note before deleting it from the old one", () => {
    const moved = block("A.md", "09:00", "10:00");
    const next = { ...moved, note: "B.md" };
    expect(planChanges([moved], edited([moved], moved, next))).toEqual([
      { original: null, block: next },
      { original: moved, block: null },
    ]);
  });

  it("writes a merge over the target's block, then deletes the moved one", () => {
    const moved = block("A.md", "09:00", "10:00", "First.");
    const target = block("B.md", "09:30", "11:00", "Second.");
    const blocks = [moved, target];
    expect(planChanges(blocks, edited(blocks, moved, { ...moved, note: "B.md" }))).toEqual([
      { original: target, block: block("B.md", "09:00", "11:00", "First.\nSecond.") },
      { original: moved, block: null },
    ]);
  });

  it("deletes every extra block a merge swallows", () => {
    // A note's own blocks never overlap, so each block it swallows must overlap the moved block itself.
    const moved = block("A.md", "09:00", "11:00");
    const second = block("B.md", "09:30", "10:00");
    const third = block("B.md", "10:30", "12:00");
    const blocks = [moved, second, third];
    const changes = planChanges(blocks, edited(blocks, moved, { ...moved, note: "B.md" }));
    expect(changes.map(({ original, block: written }) => [original?.note, written?.note])).toEqual([
      ["B.md", "B.md"],
      ["A.md", undefined],
      ["B.md", undefined],
    ]);
  });
});

describe("History", () => {
  const first = block("A.md", "09:00", "10:00");
  const second = block("A.md", "09:00", "11:00");
  const third = block("A.md", "09:00", "12:00");

  it("undoes by sending the current block as the original", () => {
    const history = new History();
    history.push([{ original: first, block: second }]);
    expect(history.peek()).toEqual([{ original: second, block: first }]);
    history.drop();
    expect(history.peek()).toBeUndefined();
  });

  it("keeps the edit until it is dropped, so a failed undo can run again", () => {
    const history = new History();
    history.push([{ original: first, block: second }]);
    history.peek();
    expect(history.peek()).toEqual([{ original: second, block: first }]);
  });

  it("undoes the newest edit first", () => {
    const history = new History();
    history.push([{ original: first, block: second }]);
    history.push([{ original: second, block: third }]);
    expect(history.peek()).toEqual([{ original: third, block: second }]);
    history.drop();
    expect(history.peek()).toEqual([{ original: second, block: first }]);
  });

  it("reverses a multi-write edit in reverse order", () => {
    const moved = block("A.md", "09:00", "10:00");
    const merged = block("B.md", "09:00", "11:00");
    const target = block("B.md", "09:30", "11:00");
    const history = new History();
    history.push([
      { original: target, block: merged },
      { original: moved, block: null },
    ]);
    expect(history.peek()).toEqual([
      { original: null, block: moved },
      { original: merged, block: target },
    ]);
  });

  it("folds edits of one group into a single undo", () => {
    const history = new History();
    history.push([{ original: first, block: second }], 1);
    history.push([{ original: second, block: third }], 1);
    expect(history.peek()).toEqual([{ original: third, block: first }]);
    history.drop();
    expect(history.peek()).toBeUndefined();
  });

  it("keeps edits of different groups apart", () => {
    const history = new History();
    history.push([{ original: first, block: second }], 1);
    history.push([{ original: second, block: third }], 2);
    expect(history.peek()).toEqual([{ original: third, block: second }]);
  });
});
