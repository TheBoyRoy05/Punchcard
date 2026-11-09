import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { mount } from "svelte";

import type { TimelineHost } from "../core/timeline/host";
import Timeline from "../core/timeline/Timeline.svelte";
import { wallClock } from "../wasm/core";
import type { Opening, StintLoadedRange } from "./stint";

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
      return { ok: false, error: String(error) };
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

// eslint's type checker reads no .svelte file, so it sees mount's exports as `any` unless typed here.
const timeline = mount(Timeline, {
  target: document.getElementById("timeline") ?? document.body,
  props: { ...host, initialRange: opening.range },
}) as { refresh(): void; settle(): Promise<string> };
const appWindow = getCurrentWindow();
// Only the first refused close stays open, so a save that keeps failing never keeps the window open for good.
let closeRefused = false;

window.addEventListener("focus", () => timeline.refresh());

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
