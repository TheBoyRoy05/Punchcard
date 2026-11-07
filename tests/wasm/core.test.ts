import { readdirSync, readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

import {
  clip,
  dayWindow,
  editBlock,
  EPOCH_MS,
  formatLog,
  formatSpan,
  formatTime,
  logicalDay,
  MS_PER_MINUTE,
  overlaps,
  parseLog,
  parseTime,
  wallClock,
  workedDays,
  workedHours,
  type Block,
  type Minutes,
} from "../../src/wasm/core";

interface Expected {
  blocks: { start: string; end: string | null; entry: string }[];
  bad_headings: string[];
  punchcard__worked_days: string[];
  punchcard__worked_hours: number;
  formatted?: string;
}

interface ExpectedToggle {
  after: string;
}

interface FixtureSettings {
  day_start_hour: number;
  block_min_minutes: number;
}

const settingsJson = readFileSync(new URL("../fixtures/settings.json", import.meta.url), "utf8");
const { day_start_hour: DAY_START_HOUR, block_min_minutes: BLOCK_MIN_MINUTES } = JSON.parse(
  settingsJson
) as FixtureSettings;

describe.each(fixtures("../fixtures/"))("fixture %s", (name) => {
  const body = readFileSync(new URL(`../fixtures/${name}.md`, import.meta.url), "utf8");
  const expected = JSON.parse(readFileSync(new URL(`../fixtures/${name}.json`, import.meta.url), "utf8")) as Expected;
  const log = parseLog(name, body);
  const spans = log.blocks.map((block) => block.span);

  it("parses the expected blocks", () => {
    expect(log.blocks).toEqual(
      expected.blocks.map(({ start, end, entry }) => ({
        note: name,
        span: { start: minutes(start), end: end === null ? null : minutes(end) },
        entry,
      }))
    );
  });

  it("reports the expected bad headings", () => {
    expect(log.badHeadings).toEqual(expected.bad_headings);
  });

  it("derives worked days and hours", () => {
    expect(workedDays(spans, DAY_START_HOUR)).toEqual(expected.punchcard__worked_days);
    expect(workedHours(spans)).toBe(expected.punchcard__worked_hours);
  });

  // A fixture with non-canonical headings sets `formatted`; every other one round trips byte for byte.
  it("formats back to the expected text", () => {
    expect(formatLog(log)).toBe(expected.formatted ?? body);
  });
});

describe.each(fixtures("../fixtures/toggle/"))("toggle fixture %s", (name) => {
  const json = readFileSync(new URL(`../fixtures/toggle/${name}.json`, import.meta.url), "utf8");
  const expected = JSON.parse(json) as ExpectedToggle;

  it("writes a log that formats back unchanged", () => {
    expect(formatLog(parseLog(name, expected.after))).toBe(expected.after);
  });
});

describe("the wasm boundary", () => {
  const block: Block = { note: "A", span: { start: minutes("2026-11-02T09:00"), end: null }, entry: "" };
  const now = minutes("2026-11-02T12:00");

  it("returns an edit refusal as a result, so callers never catch", () => {
    expect(editBlock([], block, block, now, BLOCK_MIN_MINUTES)).toEqual({
      ok: false,
      error: "The block is no longer in the log.",
    });
  });

  it("returns an accepted edit with every block", () => {
    const next = { ...block, entry: "Done." };
    expect(editBlock([block], block, next, now, BLOCK_MIN_MINUTES)).toEqual({ ok: true, blocks: [next] });
  });

  it("throws on a time no heading can hold", () => {
    expect(() => formatTime(-1)).toThrow("Not a block time: -1");
    expect(() => formatTime(1.5)).toThrow("Not a block time: 1.5");
    expect(() => formatTime(minutes("2100-01-01T00:00"))).toThrow("Not a block time");
  });

  it("throws on a day that is not a logical day", () => {
    expect(() => dayWindow("2026-11-02", DAY_START_HOUR)).toThrow("Not a logical day: 2026-11-02");
  });

  it("throws on a day start that is not an hour of the day", () => {
    for (const hour of [-1, 24, 30, 4.5]) {
      expect(() => logicalDay(now, hour)).toThrow(`Not an hour of the day: ${hour}`);
      expect(() => dayWindow("26-11-01", hour)).toThrow(`Not an hour of the day: ${hour}`);
      expect(() => workedDays([], hour)).toThrow(`Not an hour of the day: ${hour}`);
    }
  });

  it("throws on a logical day before 2000 instead of naming it 99", () => {
    expect(() => logicalDay(minutes("2000-01-01T00:30"), DAY_START_HOUR)).toThrow("1999-12-31 has no two-digit year");
  });

  it("takes back the window of the last day of 2099, which ends in 2100", () => {
    const window = dayWindow("99-12-31", DAY_START_HOUR);
    const late = { start: minutes("2099-12-31T23:00"), end: null };
    const lateNow = minutes("2099-12-31T23:30");

    expect(window.end).toBe(minutes("2100-01-01T04:00"));
    expect(clip(late, window, lateNow)).toEqual({ start: late.start, end: lateNow });
  });

  it("formats times and spans as headings write them", () => {
    expect(formatTime(now)).toBe("26/11/02 12:00");
    expect(formatSpan({ start: now, end: null })).toBe("26/11/02 12:00 -");
  });

  it("parses a heading time back to its minute", () => {
    expect(parseTime("26/11/02 12:00")).toBe(now);
    expect(() => parseTime("26/11/02 9:5")).toThrow("Not a block time: 26/11/02 9:5");
  });

  it("carries the day rules across", () => {
    const window = dayWindow("26-11-01", DAY_START_HOUR);
    const late = { start: minutes("2026-11-02T03:30"), end: minutes("2026-11-02T04:30") };

    expect(window).toEqual({ start: minutes("2026-11-01T04:00"), end: minutes("2026-11-02T04:00") });
    expect(logicalDay(late.start, DAY_START_HOUR)).toBe("26-11-01");
    expect(clip(late, window, now)).toEqual({ start: late.start, end: window.end });
    expect(clip({ start: now, end: null }, window, now)).toBeNull();
    expect(overlaps(late, window)).toBe(true);
  });
});

describe("wallClock", () => {
  it("keeps the local fields of now, dropping seconds", () => {
    expect(wallClock(new Date(2026, 9, 30, 9, 5, 30))).toBe(minutes("2026-10-30T09:05"));
  });
});

// Fixtures write zone-free ISO times; this reads one as wall-clock minutes the way the core counts them.
function minutes(iso: string): Minutes {
  return (Date.parse(`${iso}Z`) - EPOCH_MS) / MS_PER_MINUTE;
}

function fixtures(dir: string): string[] {
  return readdirSync(new URL(dir, import.meta.url))
    .filter((file) => file.endsWith(".md"))
    .map((file) => file.slice(0, -".md".length));
}
