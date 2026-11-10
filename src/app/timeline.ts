import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { mount } from "svelte";

import type { TimelineHost } from "../core/timeline/host";
import Timeline from "../core/timeline/Timeline.svelte";
import { wallClock } from "../wasm/core";
import type { Opening, StintLoadedRange } from "./stint";

// BlockPopup's MARGIN_PX, so a popup that fits below its block keeps the margin it checks for.
const POPUP_MARGIN_PX = 8;

declare global {
  interface Window {
    stint: Opening;
  }
}

const opening = window.stint;
const urls = new Map(opening.notes.map((note) => [note.path, note.url]));

const host: TimelineHost = {
  load: async (firstDay, lastDay) => {
    const loaded = await invoke<StintLoadedRange>("load", { firstDay, lastDay });
    for (const note of loaded.notes) {
      urls.set(note.path, note.url);
    }
    return loaded;
  },
  notes: opening.notes,
  dayStartHour: opening.dayStartHour,
  blockMinutes: opening.blockMinutes,
  blockMinMinutes: opening.blockMinMinutes,
  now: () => wallClock(new Date()),
  save: async (original, block) => {
    try {
      await invoke("save", { original, block });
      return { ok: true };
    } catch (error) {
      return failed(error);
    }
  },
  toggle: async (path) => {
    try {
      return { ok: true, running: await invoke<boolean>("toggle", { note: path }) };
    } catch (error) {
      return failed(error);
    }
  },
  // stint opens Obsidian links itself, and the timeline closes as Obsidian takes focus.
  openNote: (path) => {
    const url = urls.get(path);
    if (url !== undefined) {
      window.location.href = url;
    }
  },
};

const target = document.getElementById("timeline") ?? document.body;
// eslint's type checker reads no .svelte file, so it sees mount's exports as `any` unless typed here.
const timeline = mount(Timeline, {
  target,
  props: { ...host, initialRange: opening.range },
}) as { refresh(): void; settle(): Promise<string> };
const appWindow = getCurrentWindow();
// Only the first refused close stays open, so a save that keeps failing never keeps the window open for good.
let closeRefused = false;
let fitted = 0;

window.addEventListener("focus", () => timeline.refresh());

// The rows resize the content, but the block popup is drawn over it, so its opening and moves are watched separately.
new ResizeObserver(fit).observe(target);
new MutationObserver(fit).observe(target, { childList: true, subtree: true, attributeFilter: ["style"] });

// stint closes the window on a click elsewhere. With this listener Tauri leaves the close to the page, which destroys the window once the saves land.
void appWindow.onCloseRequested(async (event) => {
  const error = await timeline.settle();
  if (error !== "" && !closeRefused) {
    closeRefused = true;
    event.preventDefault();
  }
});

// Registered after the timeline's own listener, which marks the Esc it uses to cancel a drag or close a popup.
window.addEventListener("keydown", (event) => {
  if (event.key === "Escape" && !event.defaultPrevented) {
    void appWindow.close();
  }
});

// stint caps the height to the screen, and past the cap the page scrolls.
function fit(): void {
  const top = target.getBoundingClientRect().top;
  const popup = target.querySelector("[data-popup]")?.getBoundingClientRect();
  const height = Math.ceil(Math.max(target.offsetHeight, popup === undefined ? 0 : popup.bottom - top + POPUP_MARGIN_PX));
  // A drag restyles its block on every move, which leaves the height alone.
  if (height === fitted) {
    return;
  }

  fitted = height;
  invoke("fit", { height }).catch((error: unknown) => {
    // Forgetting the height lets the next layout change try again.
    fitted = 0;
    console.error(`stint: window not resized: ${String(error)}`);
  });
}

// A command that returns Err rejects with its message string.
function failed(error: unknown): { ok: false; error: string } {
  return { ok: false, error: String(error) };
}
