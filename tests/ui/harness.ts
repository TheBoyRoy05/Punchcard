import { mount } from "svelte";

import type { TimelineHost } from "../../src/core/timeline/host";
import Timeline from "../../src/core/timeline/Timeline.svelte";
import { dayWindow, formatSpan, overlaps, type Block } from "../../src/wasm/core";
import { BLOCK_MIN_MINUTES, BLOCK_MINUTES, BLOCKS, COMPLETED, DAY_START_HOUR, NOTES, NOW, type SavedBlock } from "./fixture";

const store = [...BLOCKS];
window.punchcard = { saves: [], opened: [], refused: null, saveMs: 0 };

// Like the Tauri host, a save is refused unless the note still holds the original heading.
const host: TimelineHost = {
  load: (firstDay, lastDay) => {
    const range = { start: dayWindow(firstDay, DAY_START_HOUR).start, end: dayWindow(lastDay, DAY_START_HOUR).end };
    const blocks = store.filter((block) => overlaps(block.span, range));
    const notes = [...NOTES, COMPLETED].filter((note) => blocks.some((block) => block.note === note.path));
    return Promise.resolve({ blocks, notes });
  },
  notes: NOTES,
  dayStartHour: DAY_START_HOUR,
  blockMinutes: BLOCK_MINUTES,
  blockMinMinutes: BLOCK_MIN_MINUTES,
  now: () => NOW,
  save: async (original, block) => {
    window.punchcard.saves.push({ original: original && saved(original), block: block && saved(block) });
    await new Promise((resolve) => window.setTimeout(resolve, window.punchcard.saveMs));
    if (window.punchcard.saves.length - 1 === window.punchcard.refused) {
      return { ok: false, error: "The disk is full." };
    }

    const index = original === null ? -1 : store.findIndex((stored) => sameHeading(stored, original));
    if (original !== null && index === -1) {
      return { ok: false, error: "The log changed since it was loaded." };
    }

    if (original !== null) {
      store.splice(index, 1);
    }
    if (block !== null) {
      store.push(block);
    }
    return { ok: true };
  },
  openNote: (path) => window.punchcard.opened.push(path),
};

mount(Timeline, { target: document.getElementById("timeline") ?? document.body, props: host });

function sameHeading(a: Block, b: Block): boolean {
  return a.note === b.note && a.span.start === b.span.start && a.span.end === b.span.end;
}

function saved({ note, span, entry }: Block): SavedBlock {
  return { note, span: formatSpan(span), entry };
}
