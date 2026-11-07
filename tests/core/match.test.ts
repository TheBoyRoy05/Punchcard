import { describe, expect, it } from "vitest";

import { matches } from "../../src/core/match";

describe("matches", () => {
  it("matches characters in order, ignoring case", () => {
    expect(matches("ptr", "Port Task Timer")).toBe(true);
  });

  it("rejects characters out of order", () => {
    expect(matches("rtp", "Port Task Timer")).toBe(false);
  });

  it("matches everything for an empty query", () => {
    expect(matches("", "Port Task Timer")).toBe(true);
  });

  it("needs one name character per repeated query character", () => {
    expect(matches("tt", "Task Timer")).toBe(true);
    expect(matches("ttt", "Task Timer")).toBe(false);
  });
});
