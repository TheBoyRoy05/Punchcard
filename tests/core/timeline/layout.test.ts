import { describe, expect, it } from "vitest";

import {
  dragSpan,
  formatClock,
  formatClockDigits,
  formatDay,
  formatHours,
  loggedHours,
  parseClock,
  parseEnd,
  percentOf,
  pixelsToMinutes,
  shiftDay,
  snap,
  weekOf,
} from "../../../src/core/timeline/layout";
import { editBlock, type ClosedSpan, type EditResult } from "../../../src/wasm/core";
import { at } from "../../time";

const DAY_START_HOUR = 4;
const BLOCK_MIN_MINUTES = 3;
const WINDOW = { start: at("11-02 04:00"), end: at("11-03 04:00") };

describe("snap", () => {
  it("rounds to the nearest 5 minutes", () => {
    expect(snap(at("11-02 09:32"))).toEqual(at("11-02 09:30"));
    expect(snap(at("11-02 09:33"))).toEqual(at("11-02 09:35"));
  });
});

describe("pixel and time mapping", () => {
  it("places a time as a percent of the 24-hour window", () => {
    expect(percentOf(at("11-02 16:00"), WINDOW)).toBe(50);
    expect(percentOf(at("11-03 01:00"), WINDOW)).toBe(87.5);
  });

  it("turns pixels into minutes for the track width", () => {
    expect(pixelsToMinutes(60, 1440)).toBe(60);
  });
});

describe("dragSpan", () => {
  const span = { start: at("11-02 09:30"), end: at("11-02 11:45") };

  it("moves the body at the same length, snapping the start", () => {
    expect(dragSpan(span, "move", 32)).toEqual({ start: at("11-02 10:00"), end: at("11-02 12:15") });
  });

  it("snaps a dragged end", () => {
    expect(dragSpan(span, "end", 17)).toEqual({ start: span.start, end: at("11-02 12:00") });
  });

  it("keeps a dragged start 5 minutes before the end", () => {
    expect(dragSpan(span, "start", 5 * 60)).toEqual({ start: at("11-02 11:40"), end: span.end });
  });

  it("keeps a dragged end 5 minutes after the start", () => {
    expect(dragSpan(span, "end", -5 * 60)).toEqual({ start: span.start, end: at("11-02 09:35") });
  });

  it("moves a running block's start and leaves it running", () => {
    expect(dragSpan({ start: span.start, end: null }, "start", -12)).toEqual({
      start: at("11-02 09:20"),
      end: null,
    });
  });
});

describe("loggedHours", () => {
  const blocks = [
    { note: "a", span: { start: at("11-02 02:00"), end: at("11-02 05:00") }, entry: "" },
    { note: "a", span: { start: at("11-02 20:00"), end: at("11-03 06:00") }, entry: "" },
    { note: "b", span: { start: at("11-02 21:00"), end: at("11-02 22:00") }, entry: "" },
    { note: "b", span: { start: at("11-03 05:00"), end: null }, entry: "" },
  ];
  const now = at("11-03 07:00");

  it("counts a block that crosses the day start whole, on the day it starts", () => {
    expect(loggedHours(blocks, ["26-11-01"], DAY_START_HOUR, now)).toBe(3);
    expect(loggedHours(blocks, ["26-11-02"], DAY_START_HOUR, now)).toBe(10);
  });

  it("counts overlapping blocks once and a running block up to now", () => {
    expect(loggedHours(blocks, ["26-11-02", "26-11-03"], DAY_START_HOUR, now)).toBe(11);
  });
});

describe("formatHours", () => {
  it("writes hours and padded minutes", () => {
    expect(formatHours(9.12)).toBe("9h 07m");
    expect(formatHours(0)).toBe("0h 00m");
  });
});

describe("clock times", () => {
  it("formats the wall-clock time", () => {
    expect(formatClock(at("11-02 07:05"))).toBe("07:05");
  });

  it("formats the popup's clock as 4 padded digits", () => {
    expect(formatClockDigits(at("10-30 02:30"))).toBe("0230");
    expect(formatClockDigits(at("11-02 14:50"))).toBe("1450");
  });

  it("reads a time after the day start on the same date", () => {
    expect(parseClock("1500", "26-11-02", DAY_START_HOUR)).toEqual(at("11-02 15:00"));
  });

  it("reads a time before the day start as the next morning", () => {
    expect(parseClock("0130", "26-11-02", DAY_START_HOUR)).toEqual(at("11-03 01:30"));
  });

  it("reads 4 digits, a pasted HH:MM and surrounding spaces", () => {
    expect(parseClock("1450", "26-11-02", DAY_START_HOUR)).toEqual(at("11-02 14:50"));
    expect(parseClock("0430", "26-11-02", DAY_START_HOUR)).toEqual(at("11-02 04:30"));
    expect(parseClock("14:50", "26-11-02", DAY_START_HOUR)).toEqual(at("11-02 14:50"));
    expect(parseClock(" 0930 ", "26-11-02", DAY_START_HOUR)).toEqual(at("11-02 09:30"));
  });

  it("refuses text that is not a clock time", () => {
    const original = { start: at("11-02 09:00"), end: at("11-02 10:00") };
    for (const text of ["", "145", "2460", "1260", "ab12", "9:30", "24:00", "12:60", "14500"]) {
      expect(parseClock(text, "26-11-02", DAY_START_HOUR)).toBeNull();
      expect(parseEnd(text, original.start, original)).toBeNull();
    }
  });
});

describe("parseEnd", () => {
  const now = at("11-03 12:00");
  const afternoon = { start: at("11-02 15:00"), end: at("11-02 17:05") };
  const pastDayStart = { start: at("10-30 02:30"), end: at("10-30 05:00") };
  const overnight = { start: at("10-29 23:00"), end: at("10-30 01:00") };

  // The popup sends the resolved span to the core, which owns the end-after-start rule.
  function save(span: ClosedSpan, text: string): EditResult {
    const original = { note: "A.md", span, entry: "" };
    const typed = parseEnd(text, span.start, span);
    return editBlock([original], original, { ...original, span: { start: span.start, end: typed } }, now, BLOCK_MIN_MINUTES);
  }

  it("reads an end on the start's date, as HHmm or a pasted HH:MM", () => {
    expect(parseEnd("1630", afternoon.start, afternoon)).toEqual(at("11-02 16:30"));
    expect(parseEnd("16:30", afternoon.start, afternoon)).toEqual(at("11-02 16:30"));
  });

  it("keeps a block that crosses the day start on one date", () => {
    expect(parseEnd("0530", pastDayStart.start, pastDayStart)).toEqual(at("10-30 05:30"));
  });

  it("does not roll a same-date block's end before its start to the next date", () => {
    expect(parseEnd("1450", afternoon.start, afternoon)).toEqual(at("11-02 14:50"));
  });

  it("lets the core refuse an end before the start", () => {
    expect(save(afternoon, "1450")).toEqual({ ok: false, error: "The end must be after the start." });
  });

  it("rolls a block that crossed midnight to end on the next date", () => {
    expect(parseEnd("0130", overnight.start, overnight)).toEqual(at("10-30 01:30"));
  });

  it("keeps an overnight block's end when its start moves past midnight", () => {
    expect(parseEnd("0100", at("10-30 00:30"), overnight)).toEqual(at("10-30 01:00"));
  });

  it("ends an overnight block before midnight on the start's date", () => {
    expect(parseEnd("2330", overnight.start, overnight)).toEqual(at("10-29 23:30"));
  });
});

describe("days", () => {
  it("steps across a month boundary", () => {
    expect(shiftDay("26-10-31", 1, DAY_START_HOUR)).toBe("26-11-01");
    expect(shiftDay("26-11-01", -1, DAY_START_HOUR)).toBe("26-10-31");
  });

  it("lists the week Monday to Sunday", () => {
    expect(weekOf("26-10-29", DAY_START_HOUR)).toEqual([
      "26-10-26",
      "26-10-27",
      "26-10-28",
      "26-10-29",
      "26-10-30",
      "26-10-31",
      "26-11-01",
    ]);
  });

  it("starts a week on a Sunday at the Monday before", () => {
    expect(weekOf("26-11-01", DAY_START_HOUR)[0]).toBe("26-10-26");
  });

  it("names a day from its date, not the host time zone", () => {
    expect(formatDay("26-11-02", DAY_START_HOUR, "full")).toBe("Monday 2 November");
  });

  it("writes the short day styles", () => {
    expect(formatDay("26-11-02", DAY_START_HOUR, "date")).toBe("2 Nov");
    expect(formatDay("26-10-29", DAY_START_HOUR, "weekday")).toBe("Thu 29");
  });

  it("names the weekday of the first logical day the core can write", () => {
    expect(formatDay("00-01-01", DAY_START_HOUR, "full")).toBe("Saturday 1 January");
  });
});
