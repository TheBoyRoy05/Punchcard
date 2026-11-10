import { expect, test, type Locator, type Page } from "@playwright/test";

import { formatSpan, type Minutes, type Span } from "../../src/wasm/core";
import { at } from "../time";
import { COMPLETED, HARNESS, NOTES, NOW, type HarnessState } from "./fixture";

const MINUTES_PER_DAY = 1440;
const MONDAY = at("11-02 04:00");
const PORT = "Tasks/Port Task Timer.md";
const SERIALIZE = "Tasks/Serialize Hold.md";
const DESIGN = "Tasks/Design Timeline Look.md";
const ANCHOR = "Projects/Knowledge Anchor.md";
const WRITEUP = "Tasks/PA5 Writeup.md";

type Box = NonNullable<Awaited<ReturnType<Locator["boundingBox"]>>>;

test.beforeEach(async ({ page }) => {
  await page.goto(HARNESS);
  await expect(page.locator("[data-key]").first()).toBeVisible();
});

test("draws each block at its times, with union hours in the header", async ({ page }) => {
  const drawn: [string, string, string | null][] = [
    [PORT, "11-02 09:30", "11-02 11:45"],
    [PORT, "11-02 13:00", "11-02 14:43"],
    [SERIALIZE, "11-02 07:15", "11-02 08:55"],
    [SERIALIZE, "11-02 15:00", "11-02 17:05"],
    [ANCHOR, "11-02 11:30", "11-02 12:25"],
    [DESIGN, "11-02 17:25", null],
  ];
  for (const [note, start, end] of drawn) {
    const box = await boxOf(blockOf(page, note, at(start)));
    expect(box.x).toBeCloseTo(await xAt(page, note, at(start)), 0);
    expect(box.x + box.width).toBeCloseTo(await xAt(page, note, end === null ? NOW : at(end)), 0);
  }

  // 15 minutes of Port Task Timer and Knowledge Anchor overlap, so the union is 9h 07m, not the 9h 22m sum.
  await expect(page.locator("[data-total]")).toHaveText("9h 07m");
  await expect(page.locator(`[data-row="${PORT}"] [data-hours]`)).toHaveText("3h 58m");
  await expect(page.locator("h1")).toHaveText("Monday 2 November");
});

test("draws a now line and leaves a running block's end fixed", async ({ page }) => {
  const line = await boxOf(page.locator(`[data-track="${DESIGN}"] [data-now]`));
  expect(line.x).toBeCloseTo(await xAt(page, DESIGN, NOW), 0);

  const running = blockOf(page, DESIGN, at("11-02 17:25"));
  await expect(running.locator("[data-edge=end]")).toHaveCount(0);
  await expect(running.locator("[data-edge=start]")).toHaveCount(1);
  await expect(page.locator(`[data-row="${DESIGN}"] [data-hours]`)).toHaveClass(/pc:text-now/);
});

test("a running block's body does not drag", async ({ page }) => {
  const running = blockOf(page, DESIGN, at("11-02 17:25"));
  const box = await boxOf(running);

  await drag(page, box.x + box.width / 2, box.y + box.height / 2, -60);

  expect(await saves(page)).toEqual([]);
  expect((await boxOf(running)).x).toBeCloseTo(box.x, 0);
});

test("dragging the start edge snaps to 5 minutes and saves", async ({ page }) => {
  const box = await boxOf(blockOf(page, SERIALIZE, at("11-02 15:00")));

  await drag(page, box.x + 2, box.y + box.height / 2, -(await pixelsFor(page, SERIALIZE, 23)));

  await expect.poll(() => saves(page)).toEqual([
    {
      original: saved(SERIALIZE, "11-02 15:00", "11-02 17:05", "Hold now keyed by domain."),
      block: saved(SERIALIZE, "11-02 14:35", "11-02 17:05", "Hold now keyed by domain."),
    },
  ]);
  const moved = await boxOf(blockOf(page, SERIALIZE, at("11-02 14:35")));
  expect(moved.x).toBeCloseTo(await xAt(page, SERIALIZE, at("11-02 14:35")), 0);
});

test("a drag released off its row still saves, and the next drag works", async ({ page }) => {
  const box = await boxOf(blockOf(page, SERIALIZE, at("11-02 07:15")));
  const x = box.x + box.width / 2;
  const y = box.y + box.height / 2;
  const shift = await pixelsFor(page, SERIALIZE, 32);

  await page.mouse.move(x, y);
  await page.mouse.down();
  await page.mouse.move(x + shift, y, { steps: 5 });
  await page.mouse.move(x + shift, y + 300, { steps: 5 });
  await page.mouse.up();

  await expect.poll(() => saves(page)).toEqual([
    { original: saved(SERIALIZE, "11-02 07:15", "11-02 08:55"), block: saved(SERIALIZE, "11-02 07:45", "11-02 09:25") },
  ]);

  const port = await boxOf(blockOf(page, PORT, at("11-02 09:30")));
  await drag(page, port.x + port.width - 2, port.y + port.height / 2, await pixelsFor(page, PORT, 17));
  await expect.poll(async () => (await saves(page))[1]?.block).toEqual(saved(PORT, "11-02 09:30", "11-02 12:00", "Ported the toggle."));
});

test("a drag during another block's save saves too, while the saving block refuses a new drag", async ({ page }) => {
  await page.evaluate(() => (window.punchcard.saveMs = 300));
  const serialize = await boxOf(blockOf(page, SERIALIZE, at("11-02 07:15")));
  await drag(page, serialize.x + serialize.width / 2, serialize.y + serialize.height / 2, await pixelsFor(page, SERIALIZE, 32));

  const held = await boxOf(blockOf(page, SERIALIZE, at("11-02 07:45")));
  expect(held.x).toBeCloseTo(await xAt(page, SERIALIZE, at("11-02 07:45")), 0);
  await drag(page, held.x + held.width / 2, held.y + held.height / 2, 40);

  const port = await boxOf(blockOf(page, PORT, at("11-02 09:30")));
  await drag(page, port.x + port.width - 2, port.y + port.height / 2, await pixelsFor(page, PORT, 17));

  await expect.poll(() => saves(page)).toEqual([
    { original: saved(SERIALIZE, "11-02 07:15", "11-02 08:55"), block: saved(SERIALIZE, "11-02 07:45", "11-02 09:25") },
    { original: saved(PORT, "11-02 09:30", "11-02 11:45", "Ported the toggle."), block: saved(PORT, "11-02 09:30", "11-02 12:00", "Ported the toggle.") },
  ]);
  await page.waitForTimeout(700);
  expect(await saves(page)).toHaveLength(2);
});

test("a held drag save that fails snaps its block back and keeps the error through a later save", async ({ page }) => {
  await page.evaluate(() => {
    window.punchcard.saveMs = 300;
    window.punchcard.refused = 0;
  });
  const serialize = await boxOf(blockOf(page, SERIALIZE, at("11-02 07:15")));
  await drag(page, serialize.x + serialize.width / 2, serialize.y + serialize.height / 2, await pixelsFor(page, SERIALIZE, 32));

  const port = await boxOf(blockOf(page, PORT, at("11-02 09:30")));
  await drag(page, port.x + port.width - 2, port.y + port.height / 2, await pixelsFor(page, PORT, 17));

  await expect.poll(async () => (await saves(page)).length).toBe(2);
  await page.waitForTimeout(400);
  await expect(page.getByRole("alert")).toHaveText("The disk is full.");
  const restored = await boxOf(blockOf(page, SERIALIZE, at("11-02 07:15")));
  expect(restored.x).toBeCloseTo(await xAt(page, SERIALIZE, at("11-02 07:15")), 0);
});

test("dragging the end edge snaps to 5 minutes and saves", async ({ page }) => {
  const block = blockOf(page, PORT, at("11-02 09:30"));
  const box = await boxOf(block);
  const y = box.y + box.height / 2;

  await drag(page, box.x + box.width - 2, y, await pixelsFor(page, PORT, 17));

  await expect.poll(() => saves(page)).toEqual([
    { original: saved(PORT, "11-02 09:30", "11-02 11:45", "Ported the toggle."), block: saved(PORT, "11-02 09:30", "11-02 12:00", "Ported the toggle.") },
  ]);
  const moved = await boxOf(blockOf(page, PORT, at("11-02 09:30")));
  expect(moved.x + moved.width).toBeCloseTo(await xAt(page, PORT, at("11-02 12:00")), 0);
});

test("dragging the body moves the block at the same length", async ({ page }) => {
  const box = await boxOf(blockOf(page, SERIALIZE, at("11-02 07:15")));

  await drag(page, box.x + box.width / 2, box.y + box.height / 2, await pixelsFor(page, SERIALIZE, 32));

  await expect.poll(() => saves(page)).toEqual([
    { original: saved(SERIALIZE, "11-02 07:15", "11-02 08:55"), block: saved(SERIALIZE, "11-02 07:45", "11-02 09:25") },
  ]);
  const moved = await boxOf(blockOf(page, SERIALIZE, at("11-02 07:45")));
  expect(moved.x).toBeCloseTo(await xAt(page, SERIALIZE, at("11-02 07:45")), 0);
  expect(moved.width).toBeCloseTo(box.width, 0);
});

test("Esc during a drag cancels it", async ({ page }) => {
  const block = blockOf(page, PORT, at("11-02 13:00"));
  const box = await boxOf(block);

  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width / 2 + 60, box.y + box.height / 2, { steps: 5 });
  await page.keyboard.press("Escape");
  await page.mouse.up();

  expect(await saves(page)).toEqual([]);
  expect((await boxOf(block)).x).toBeCloseTo(box.x, 0);
  await expect(page.locator("[data-popup]")).toHaveCount(0);
});

test("double-clicking empty space adds a block and opens its popup", async ({ page }) => {
  await page.mouse.dblclick(await xAt(page, SERIALIZE, at("11-02 13:07")), await rowY(page, SERIALIZE));

  await expect.poll(() => saves(page)).toEqual([{ original: null, block: saved(SERIALIZE, "11-02 13:05", "11-02 13:35") }]);
  await expect(page.locator("[data-popup] [data-field=start]")).toHaveValue("1305");
  await expect(page.locator("[data-popup] [data-field=end]")).toHaveValue("1335");
});

test("the popup refuses an invalid end in place and saves a valid one", async ({ page }) => {
  await blockOf(page, SERIALIZE, at("11-02 15:00")).click();
  const end = page.locator("[data-popup] [data-field=end]");

  await end.fill("1450");
  await end.press("Enter");
  await expect(page.locator("[data-popup] [role=alert]")).toHaveText("The end must be after the start.");
  await end.fill("9:6");
  await end.press("Enter");
  await expect(page.locator("[data-popup] [role=alert]")).toHaveText("Write the time as HHmm, such as 0930.");
  expect(await saves(page)).toEqual([]);

  await end.fill("1630");
  await end.press("Enter");
  await expect.poll(() => saves(page)).toEqual([
    {
      original: saved(SERIALIZE, "11-02 15:00", "11-02 17:05", "Hold now keyed by domain."),
      block: saved(SERIALIZE, "11-02 15:00", "11-02 16:30", "Hold now keyed by domain."),
    },
  ]);
  await expect(page.locator("[data-popup] [role=alert]")).toHaveCount(0);
});

test("the popup edits a block that crosses the day start", async ({ page }) => {
  for (let step = 0; step < 4; step += 1) {
    await page.getByRole("button", { name: "Previous" }).click();
  }
  await expect(page.locator("h1")).toHaveText("Thursday 29 October");
  await blockOf(page, WRITEUP, at("10-30 02:30")).click();
  const end = page.locator("[data-popup] [data-field=end]");

  await page.locator("[data-popup] [data-field=start]").press("Tab");
  await end.press("Tab");
  await expect(page.locator("[data-popup] [role=alert]")).toHaveCount(0);
  expect(await saves(page)).toEqual([]);

  await end.fill("0530");
  await end.press("Enter");

  await expect.poll(() => saves(page)).toEqual([
    { original: saved(WRITEUP, "10-30 02:30", "10-30 05:00"), block: saved(WRITEUP, "10-30 02:30", "10-30 05:30") },
  ]);
  await expect(page.locator("[data-popup] [role=alert]")).toHaveCount(0);
});

test("the popup edits the end of a block that crosses midnight", async ({ page }) => {
  for (let step = 0; step < 5; step += 1) {
    await page.getByRole("button", { name: "Previous" }).click();
  }
  await expect(page.locator("h1")).toHaveText("Wednesday 28 October");
  await blockOf(page, WRITEUP, at("10-28 19:00")).click();
  const end = page.locator("[data-popup] [data-field=end]");

  await end.fill("0300");
  await end.press("Enter");
  // A save lands before the reload, and the reload resets the end field, so the second edit waits for it.
  await expect(blockOf(page, WRITEUP, at("10-28 19:00"))).toHaveAccessibleName("PA5 Writeup, 19:00-03:00");
  await end.fill("2330");
  await end.press("Enter");

  await expect.poll(() => saves(page)).toEqual([
    { original: saved(WRITEUP, "10-28 19:00", "10-29 02:10"), block: saved(WRITEUP, "10-28 19:00", "10-29 03:00") },
    { original: saved(WRITEUP, "10-28 19:00", "10-29 03:00"), block: saved(WRITEUP, "10-28 19:00", "10-28 23:30") },
  ]);
  await expect(page.locator("[data-popup] [role=alert]")).toHaveCount(0);
});

test("the popup refuses an edit shorter than the host's minimum block", async ({ page }) => {
  await blockOf(page, SERIALIZE, at("11-02 15:00")).click();
  const end = page.locator("[data-popup] [data-field=end]");

  await end.fill("1502");
  await end.press("Enter");

  await expect(page.locator("[data-popup] [role=alert]")).toHaveText("A block must be at least 3 minutes long.");
  expect(await saves(page)).toEqual([]);
});

test("Enter then Tab on a time saves it once, while the first save is still in flight", async ({ page }) => {
  await page.evaluate(() => (window.punchcard.saveMs = 300));
  await blockOf(page, SERIALIZE, at("11-02 15:00")).click();
  const end = page.locator("[data-popup] [data-field=end]");

  await end.fill("1630");
  await end.press("Enter");
  await end.press("Tab");

  await expect.poll(async () => (await saves(page)).length).toBe(1);
  await page.waitForTimeout(700);
  expect(await saves(page)).toHaveLength(1);
  await expect(page.locator("[data-popup] [role=alert]")).toHaveCount(0);
});

test("a save landing while a time is being typed keeps the typed time", async ({ page }) => {
  await page.evaluate(() => (window.punchcard.saveMs = 300));
  await blockOf(page, PORT, at("11-02 13:00")).click();
  const start = page.locator("[data-popup] [data-field=start]");

  await page.locator("[data-popup] [data-field=entry]").fill("Wired the drag.");
  await start.click();
  await start.fill("1250");

  await expect.poll(async () => (await saves(page)).length).toBe(1);
  await page.waitForTimeout(700);
  await expect(start).toHaveValue("1250");
});

test("the popup sits below its block without covering it", async ({ page }) => {
  const block = blockOf(page, SERIALIZE, at("11-02 15:00"));
  await block.click();

  const popup = await boxOf(page.locator("[data-popup]"));
  const target = await boxOf(block);
  expect(popup.y).toBeGreaterThanOrEqual(target.y + target.height);
  expect(popup.x + popup.width).toBeLessThanOrEqual(1200);
});

test("moving a block to another task merges it with what it overlaps there", async ({ page }) => {
  await blockOf(page, ANCHOR, at("11-02 11:30")).click();
  await page.locator("[data-popup] [data-task]").click();
  await page.getByLabel("Move block to task").fill("port");
  await page.getByLabel("Move block to task").press("Enter");

  await expect.poll(() => saves(page)).toEqual([
    {
      original: saved(PORT, "11-02 09:30", "11-02 11:45", "Ported the toggle."),
      block: saved(PORT, "11-02 09:30", "11-02 12:25", "Ported the toggle.\nPlanning."),
    },
    { original: saved(ANCHOR, "11-02 11:30", "11-02 12:25", "Planning."), block: null },
  ]);
  await expect(page.locator(`[data-row="${ANCHOR}"]`)).toHaveCount(0);
  await expect(page.locator("[data-popup] [data-task]")).toContainText("Port Task Timer to Rust Stint");
});

test("undo reverts the last edit through save", async ({ page }) => {
  const box = await boxOf(blockOf(page, PORT, at("11-02 09:30")));
  await drag(page, box.x + box.width - 2, box.y + box.height / 2, await pixelsFor(page, PORT, 17));
  await expect.poll(async () => (await saves(page)).length).toBe(1);

  await page.keyboard.press("ControlOrMeta+z");

  await expect.poll(async () => (await saves(page))[1]).toEqual({
    original: saved(PORT, "11-02 09:30", "11-02 12:00", "Ported the toggle."),
    block: saved(PORT, "11-02 09:30", "11-02 11:45", "Ported the toggle."),
  });
  const restored = await boxOf(blockOf(page, PORT, at("11-02 09:30")));
  expect(restored.x + restored.width).toBeCloseTo(await xAt(page, PORT, at("11-02 11:45")), 0);
});

test("undo reverts a task move in reverse order", async ({ page }) => {
  await blockOf(page, ANCHOR, at("11-02 11:30")).click();
  await page.locator("[data-popup] [data-task]").click();
  await page.getByLabel("Move block to task").fill("port");
  await page.getByLabel("Move block to task").press("Enter");
  await expect.poll(async () => (await saves(page)).length).toBe(2);
  await page.keyboard.press("Escape");

  await page.keyboard.press("ControlOrMeta+z");

  await expect.poll(async () => (await saves(page)).slice(2)).toEqual([
    { original: null, block: saved(ANCHOR, "11-02 11:30", "11-02 12:25", "Planning.") },
    {
      original: saved(PORT, "11-02 09:30", "11-02 12:25", "Ported the toggle.\nPlanning."),
      block: saved(PORT, "11-02 09:30", "11-02 11:45", "Ported the toggle."),
    },
  ]);
  await expect(blockOf(page, ANCHOR, at("11-02 11:30"))).toBeVisible();
});

test("a task move whose second write fails keeps both copies, and undo reverts the first write", async ({ page }) => {
  await page.evaluate(() => (window.punchcard.refused = 1));
  await blockOf(page, ANCHOR, at("11-02 11:30")).click();
  await page.locator("[data-popup] [data-task]").click();
  await page.getByLabel("Move block to task").fill("port");
  await page.getByLabel("Move block to task").press("Enter");

  await expect(page.locator("[data-popup] [role=alert]")).toHaveText("The disk is full.");
  expect(await saves(page)).toHaveLength(2);
  await expect(blockOf(page, ANCHOR, at("11-02 11:30"))).toBeVisible();
  const merged = await boxOf(blockOf(page, PORT, at("11-02 09:30")));
  expect(merged.x + merged.width).toBeCloseTo(await xAt(page, PORT, at("11-02 12:25")), 0);
  await page.keyboard.press("Escape");

  await page.keyboard.press("ControlOrMeta+z");

  await expect.poll(async () => (await saves(page)).slice(2)).toEqual([
    {
      original: saved(PORT, "11-02 09:30", "11-02 12:25", "Ported the toggle.\nPlanning."),
      block: saved(PORT, "11-02 09:30", "11-02 11:45", "Ported the toggle."),
    },
  ]);
  await expect(blockOf(page, ANCHOR, at("11-02 11:30"))).toBeVisible();
});

test("typing in one log entry saves after a pause and undoes as one edit", async ({ page }) => {
  await blockOf(page, PORT, at("11-02 13:00")).click();
  const entry = page.locator("[data-popup] [data-field=entry]");

  await entry.pressSequentially("Wired");
  await expect.poll(() => saves(page), { timeout: 3000 }).toHaveLength(1);
  await entry.pressSequentially(" the drag.");
  await expect.poll(() => saves(page), { timeout: 3000 }).toHaveLength(2);
  expect((await saves(page))[1]?.block?.entry).toBe("Wired the drag.");
  await page.keyboard.press("Escape");

  await page.keyboard.press("ControlOrMeta+z");

  await expect.poll(async () => (await saves(page))[2]).toEqual({
    original: saved(PORT, "11-02 13:00", "11-02 14:43", "Wired the drag."),
    block: saved(PORT, "11-02 13:00", "11-02 14:43"),
  });
});

test("Delete asks to confirm before it removes the block", async ({ page }) => {
  await blockOf(page, PORT, at("11-02 13:00")).click();
  await page.getByRole("button", { name: "Delete block" }).click();
  expect(await saves(page)).toEqual([]);

  await page.getByRole("button", { name: "Delete", exact: true }).click();

  await expect.poll(() => saves(page)).toEqual([{ original: saved(PORT, "11-02 13:00", "11-02 14:43"), block: null }]);
  await expect(page.locator("[data-popup]")).toHaveCount(0);
});

test("Esc closes the popup, and the note links open the note", async ({ page }) => {
  await blockOf(page, PORT, at("11-02 13:00")).click();
  await page.getByRole("button", { name: "Open note" }).click();
  await page.keyboard.press("Escape");
  await expect(page.locator("[data-popup]")).toHaveCount(0);

  await page.getByRole("button", { name: "Serialize Hold per Domain", exact: true }).click();

  expect((await state(page)).opened).toEqual([PORT, SERIALIZE]);
});

test("the popup takes focus on open and gives it back to its block on close", async ({ page }) => {
  const block = blockOf(page, PORT, at("11-02 13:00"));
  await block.click();
  await expect(page.locator("[data-popup] [data-field=start]")).toBeFocused();

  await page.keyboard.press("Escape");

  await expect(page.locator("[data-popup]")).toHaveCount(0);
  await expect(block).toBeFocused();
});

test("a popup closed while focus is elsewhere leaves that focus alone", async ({ page }) => {
  await blockOf(page, PORT, at("11-02 13:00")).click();
  const label = page.getByRole("button", { name: "Serialize Hold per Domain", exact: true });
  await label.click();
  await expect(label).toBeFocused();

  await page.keyboard.press("ControlOrMeta+z");

  await expect(page.locator("[data-popup]")).toHaveCount(0);
  await expect(label).toBeFocused();
});

test("an undo whose write fails stays in history and can run again", async ({ page }) => {
  const box = await boxOf(blockOf(page, PORT, at("11-02 09:30")));
  await drag(page, box.x + box.width - 2, box.y + box.height / 2, await pixelsFor(page, PORT, 17));
  await expect.poll(async () => (await saves(page)).length).toBe(1);
  await page.evaluate(() => (window.punchcard.refused = 1));

  await page.keyboard.press("ControlOrMeta+z");
  await expect(page.getByRole("alert")).toHaveText("The disk is full.");
  await page.keyboard.press("ControlOrMeta+z");

  const revert = { original: saved(PORT, "11-02 09:30", "11-02 12:00", "Ported the toggle."), block: saved(PORT, "11-02 09:30", "11-02 11:45", "Ported the toggle.") };
  await expect.poll(async () => (await saves(page)).slice(1)).toEqual([revert, revert]);
  await expect(page.getByRole("alert")).toHaveCount(0);
  const restored = await boxOf(blockOf(page, PORT, at("11-02 09:30")));
  expect(restored.x + restored.width).toBeCloseTo(await xAt(page, PORT, at("11-02 11:45")), 0);
});

test("the week view has a row per day, labels wide blocks and asks for a new block's task", async ({ page }) => {
  await page.getByRole("button", { name: "week" }).click();
  await expect(page.locator("h1")).toHaveText("2 Nov - 8 Nov");
  await page.getByRole("button", { name: "Previous" }).click();
  await expect(page.locator("h1")).toHaveText("26 Oct - 1 Nov");
  await expect(page.locator("[data-track]")).toHaveCount(7);
  // PA5 Writeup's 02:30-05:00 block on the 30th crosses the day start, so all 2h 30m count toward the 29th.
  await expect(page.locator(`[data-row="26-10-29"] [data-hours]`)).toHaveText("2h 50m");
  await expect(page.locator(`[data-row="26-10-30"] [data-hours]`)).toHaveText("0h 00m");
  await expect(page.locator("[data-total]")).toHaveText("18h 55m");

  const writeup = page.locator(`[data-track="26-10-28"] [data-key="${WRITEUP}|${at("10-28 19:00")}"]`);
  await expect(writeup).toHaveText("PA5 Writeup");
  const short = page.locator(`[data-track="26-10-29"] [data-key="${SERIALIZE}|${at("10-29 10:00")}"]`);
  await expect(short).toHaveText("");
  await expect(short).toHaveAttribute("title", "Serialize Hold per Domain");

  const track = await boxOf(page.locator(`[data-track="26-10-31"]`));
  const x = track.x + ((at("10-31 09:00") - at("10-31 04:00")) / MINUTES_PER_DAY) * track.width;
  await page.mouse.dblclick(x, track.y + track.height / 2);
  await page.getByLabel("Move block to task").fill("review");
  await page.getByLabel("Move block to task").press("Enter");

  await expect.poll(() => saves(page)).toEqual([{ original: null, block: saved("Tasks/Review Gate.md", "10-31 09:00", "10-31 09:30") }]);
});

test("a block of a note missing from the picker list draws with that note's name and color", async ({ page }) => {
  const block = blockOf(page, COMPLETED.path, at("11-02 10:00"));
  await expect(block).toHaveAccessibleName("Archive Old Spans, 10:00-10:30");
  await expect(block).toHaveClass(/pc:bg-project-4/);

  await block.click();
  await page.locator("[data-popup] [data-task]").click();

  const options = page.getByRole("listbox", { name: "Tasks" }).getByRole("option");
  await expect(options).toHaveCount(NOTES.length);
  await expect(options.filter({ hasText: COMPLETED.name })).toHaveCount(0);
});

// The harness clock stands still, so a block started at now has no width to draw, and its row's button shows the start landed.
test("a task row's play button starts the task, and its stop button stops the running block", async ({ page }) => {
  await page.getByRole("button", { name: "Start Port Task Timer to Rust Stint", exact: true }).click();

  await expect(page.getByRole("button", { name: "Stop Port Task Timer to Rust Stint", exact: true })).toBeVisible();

  await page.getByRole("button", { name: "Stop Design Timeline Look", exact: true }).click();

  await expect(blockOf(page, DESIGN, at("11-02 17:25"))).toHaveAccessibleName("Design Timeline Look, 17:25-18:09");
  await expect(page.getByRole("button", { name: "Start Design Timeline Look", exact: true })).toBeVisible();
  expect((await state(page)).toggles).toEqual([PORT, DESIGN]);
});

test("a task started from a day its new block is not on shows as running in the Start task list", async ({ page }) => {
  await page.getByRole("button", { name: "Previous" }).click();
  await expect(page.locator("h1")).toHaveText("Sunday 1 November");
  const start = page.getByRole("button", { name: "Start task", exact: true });
  await start.click();
  await page.getByLabel("Task to start or stop").fill("review");
  await page.keyboard.press("Enter");
  await expect.poll(async () => (await state(page)).toggles).toEqual(["Tasks/Review Gate.md"]);

  await start.click();

  await expect(page.getByRole("option").filter({ hasText: "Review Gate" })).toContainText("Running");
});

test("a repeated or composing Enter in the Start task list chooses nothing", async ({ page }) => {
  await page.getByRole("button", { name: "Start task", exact: true }).click();
  const search = page.getByLabel("Task to start or stop");
  await search.fill("review");

  await search.dispatchEvent("keydown", { key: "Enter", repeat: true });
  await search.dispatchEvent("keydown", { key: "Enter", isComposing: true });

  await expect(search).toBeVisible();
  expect((await state(page)).toggles).toEqual([]);
});

test("week rows are days, so they have no play button, and the Start task row stays", async ({ page }) => {
  await page.getByRole("button", { name: "week" }).click();

  await expect(page.locator("[data-track]")).toHaveCount(7);
  await expect(page.locator("[data-toggle]")).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Start task", exact: true })).toBeVisible();
});

test("choosing a task under Start task toggles it, reloads the blocks and closes the list", async ({ page }) => {
  const start = page.getByRole("button", { name: "Start task", exact: true });
  await start.click();
  const search = page.getByLabel("Task to start or stop");
  await expect(search).toBeFocused();

  await search.fill("serialize");
  await search.press("Enter");

  await expect(page.getByRole("button", { name: "Stop Serialize Hold per Domain", exact: true })).toBeVisible();
  await expect(search).toHaveCount(0);
  await expect(start).toBeFocused();
  expect((await state(page)).toggles).toEqual([SERIALIZE]);

  await start.click();
  const option = page.getByRole("option").filter({ hasText: "Serialize Hold per Domain" });
  await expect(option).toContainText("Running");
});

test("a tiny block draws at its true width, has no edge handles, and its body moves it", async ({ page }) => {
  const tiny = blockOf(page, COMPLETED.path, at("11-02 10:40"));
  const box = await boxOf(tiny);
  expect(box.width).toBeLessThan(18);
  expect(box.x).toBeCloseTo(await xAt(page, COMPLETED.path, at("11-02 10:40")), 0);
  expect(box.x + box.width).toBeCloseTo(await xAt(page, COMPLETED.path, at("11-02 10:45")), 0);
  await expect(tiny.locator("[data-edge]")).toHaveCount(0);

  await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
  await expect(page.locator("[data-popup]")).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.locator("[data-popup]")).toHaveCount(0);

  // Pressed beside the block, inside its wider hit area.
  await drag(page, box.x + box.width + 3, box.y + box.height / 2, await pixelsFor(page, COMPLETED.path, 10));

  await expect.poll(() => saves(page)).toEqual([
    { original: saved(COMPLETED.path, "11-02 10:40", "11-02 10:45"), block: saved(COMPLETED.path, "11-02 10:50", "11-02 10:55") },
  ]);
});

test("dragging a tiny running block moves its start", async ({ page }) => {
  // Narrow enough that Design Timeline Look's 44 running minutes draw under 18 px.
  await page.setViewportSize({ width: 760, height: 800 });
  const running = blockOf(page, DESIGN, at("11-02 17:25"));
  const box = await boxOf(running);
  expect(box.width).toBeLessThan(18);
  expect(box.x + box.width).toBeCloseTo(await xAt(page, DESIGN, NOW), 0);
  await expect(running.locator("[data-edge]")).toHaveCount(0);

  await drag(page, box.x + box.width / 2, box.y + box.height / 2, -(await pixelsFor(page, DESIGN, 10)));

  await expect.poll(() => saves(page)).toEqual([
    { original: saved(DESIGN, "11-02 17:25", null), block: saved(DESIGN, "11-02 17:15", null) },
  ]);
});

function blockOf(page: Page, note: string, start: Minutes): Locator {
  return page.locator(`[data-track="${note}"] [data-key="${note}|${start}"]`);
}

async function boxOf(locator: Locator): Promise<Box> {
  // A save lands before the reload that redraws its block, so the block can be replaced while it is measured.
  let box: Box | null = null;
  await expect(async () => {
    box = await locator.boundingBox();
    expect(box).not.toBeNull();
  }).toPass();
  if (box === null) {
    throw new Error("The element is not drawn.");
  }
  return box;
}

async function xAt(page: Page, row: string, time: Minutes): Promise<number> {
  const track = await boxOf(page.locator(`[data-track="${row}"]`));
  return track.x + ((time - MONDAY) / MINUTES_PER_DAY) * track.width;
}

async function rowY(page: Page, row: string): Promise<number> {
  const track = await boxOf(page.locator(`[data-track="${row}"]`));
  return track.y + track.height / 2;
}

async function pixelsFor(page: Page, row: string, minutes: number): Promise<number> {
  const track = await boxOf(page.locator(`[data-track="${row}"]`));
  return (minutes / MINUTES_PER_DAY) * track.width;
}

async function drag(page: Page, x: number, y: number, shift: number): Promise<void> {
  await page.mouse.move(x, y);
  await page.mouse.down();
  await page.mouse.move(x + shift, y, { steps: 5 });
  await page.mouse.up();
}

function state(page: Page): Promise<HarnessState> {
  return page.evaluate(() => window.punchcard);
}

async function saves(page: Page): Promise<HarnessState["saves"]> {
  return (await state(page)).saves;
}

function saved(note: string, start: string, end: string | null, entry = ""): HarnessState["saves"][number]["block"] {
  const span: Span = { start: at(start), end: end === null ? null : at(end) };
  return { note, span: formatSpan(span), entry };
}
