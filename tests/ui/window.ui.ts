import { expect, test, type Page } from "@playwright/test";

import type { Block } from "../../src/wasm/core";
import { BLOCK_MIN_MINUTES, BLOCK_MINUTES, BLOCKS, DAY_START_HOUR, NOTES } from "./fixture";

// The real timeline page, with Tauri's IPC replaced by a recorder that answers the way stint does.
const PAGE = "/timeline.html";
const SAVE_MS = 300;
const CLOSE_REQUESTED = "tauri://close-requested";

interface Call {
  cmd: string;
  args: { block?: Block | null; event?: string; handler?: number } | undefined;
}

interface FakeTauri {
  calls: Call[];
  // What stint does on a click elsewhere: Tauri hands the close to the page's close-requested listener.
  requestClose(): Promise<unknown>;
  // The message every save rejects with, the way stint refuses one; null lets saves land.
  refusal: string | null;
}

declare global {
  interface Window {
    fakeTauri: FakeTauri;
  }
}

test.beforeEach(async ({ page }) => {
  await page.clock.setFixedTime(new Date(2026, 10, 2, 18, 9));
  const opening = {
    range: "day",
    notes: NOTES.map((note) => ({ ...note, url: `obsidian://open?path=${note.path}` })),
    dayStartHour: DAY_START_HOUR,
    blockMinutes: BLOCK_MINUTES,
    blockMinMinutes: BLOCK_MIN_MINUTES,
  };
  await page.addInitScript(fakeTauri, { opening, blocks: BLOCKS, saveMs: SAVE_MS, closeRequested: CLOSE_REQUESTED });
  await page.goto(PAGE);
  await expect(page.locator("[data-key]").first()).toBeVisible();
});

test("a click elsewhere right after typing a log entry saves it before the window closes", async ({ page }) => {
  await typeEntry(page);

  await page.evaluate(() => window.fakeTauri.requestClose());

  await expectSavedThenDestroyed(page);
});

test("Esc twice right after typing a log entry saves it before the window closes", async ({ page }) => {
  await typeEntry(page);

  await page.keyboard.press("Escape");
  await page.keyboard.press("Escape");

  await expectSavedThenDestroyed(page);
});

test("a click elsewhere after a refused save keeps the window open to show why, and the next click closes it", async ({ page }) => {
  await page.evaluate(() => (window.fakeTauri.refusal = "Port changed since it was loaded"));
  await typeEntry(page);

  await page.evaluate(() => window.fakeTauri.requestClose());

  await expect(page.getByRole("alert")).toHaveText("Port changed since it was loaded");
  expect(await commands(page)).not.toContain("plugin:window|destroy");

  await page.evaluate(() => window.fakeTauri.requestClose());

  expect(await commands(page)).toContain("plugin:window|destroy");
});

async function typeEntry(page: Page): Promise<void> {
  await page.getByRole("button", { name: "Knowledge Anchor, 11:30-12:25" }).click();
  await page.locator("[data-popup] [data-field=entry]").pressSequentially(" Wired");
}

async function expectSavedThenDestroyed(page: Page): Promise<void> {
  await expect.poll(() => commands(page)).toContain("plugin:window|destroy");
  const calls = await page.evaluate(() => window.fakeTauri.calls);
  const cmds = calls.map((call) => call.cmd);
  const save = calls.findIndex((call) => call.cmd === "save" && call.args?.block?.entry.endsWith(" Wired") === true);

  const landed = cmds.indexOf("save landed", save);

  expect(save).toBeGreaterThan(-1);
  expect(landed).toBeGreaterThan(save);
  expect(landed).toBeLessThan(cmds.indexOf("plugin:window|destroy"));
}

function commands(page: Page): Promise<string[]> {
  return page.evaluate(() => window.fakeTauri.calls.map((call) => call.cmd));
}

// Runs in the page before its scripts, so it can use nothing from this file's scope.
function fakeTauri({ opening, blocks, saveMs, closeRequested }: { opening: unknown; blocks: Block[]; saveMs: number; closeRequested: string }): void {
  const callbacks = new Map<number, (event: unknown) => unknown>();
  const listeners = new Map<string, number>();
  const calls: Call[] = [];
  const requestClose = async (): Promise<unknown> => {
    const handler = callbacks.get(listeners.get(closeRequested) ?? -1);
    return handler?.({ event: closeRequested, id: 0, payload: null });
  };
  const fake: FakeTauri = { calls, requestClose, refusal: null };

  const internals = {
    metadata: { currentWindow: { label: "timeline" }, currentWebview: { windowLabel: "timeline", label: "timeline" } },
    transformCallback: (callback: (event: unknown) => unknown): number => {
      callbacks.set(callbacks.size + 1, callback);
      return callbacks.size;
    },
    invoke: async (cmd: string, args: Call["args"]): Promise<unknown> => {
      // The page sends Svelte state proxies, which structuredClone refuses; Tauri sends JSON anyway.
      calls.push({ cmd, args: JSON.parse(JSON.stringify(args ?? null)) as Call["args"] });
      switch (cmd) {
        case "load":
          return { blocks, notes: (opening as { notes: unknown }).notes };
        case "save":
          await new Promise((resolve) => window.setTimeout(resolve, saveMs));
          if (fake.refusal !== null) {
            // eslint-disable-next-line @typescript-eslint/only-throw-error -- Tauri rejects a command that returned Err with its message string, not an Error.
            throw fake.refusal;
          }
          calls.push({ cmd: "save landed", args: undefined });
          return null;
        case "plugin:event|listen":
          listeners.set(args?.event ?? "", args?.handler ?? -1);
          return args?.handler;
        case "plugin:window|close":
          void requestClose();
          return null;
        default:
          return null;
      }
    },
  };
  Object.assign(window, { stint: opening, fakeTauri: fake, __TAURI_INTERNALS__: internals });
}
