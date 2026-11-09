<script lang="ts">
  import ArrowUpRight from "@lucide/svelte/icons/arrow-up-right";
  import ChevronDown from "@lucide/svelte/icons/chevron-down";
  import X from "@lucide/svelte/icons/x";
  import { onDestroy, onMount, untrack } from "svelte";

  import { logicalDay, workedHours, type Block, type Minutes } from "../../wasm/core";
  import type { TaskNote } from "./host";
  import { formatClockDigits, formatHours, parseClock, parseEnd } from "./layout";
  import NotePicker from "./NotePicker.svelte";
  import { projectColor } from "./palette";

  const GAP_PX = 6;
  const MARGIN_PX = 8;
  const ENTRY_SAVE_MS = 1000;

  type Field = "start" | "end" | "entry" | "task";

  interface Props {
    block: Block;
    // Null for a draft, which has no task yet.
    note: TaskNote | null;
    anchor: HTMLElement;
    notes: TaskNote[];
    now: Minutes;
    dayStartHour: number;
    picking: boolean;
    onEdit: (change: Partial<Block>, typing: boolean) => Promise<string>;
    onDelete: () => Promise<string>;
    openNote: (path: string) => void;
    onClose: () => void;
  }

  let { block, note, anchor, notes, now, dayStartHour, picking, onEdit, onDelete, openNote, onClose }: Props = $props();

  let card: HTMLDivElement | undefined = $state();
  let startInput: HTMLInputElement | undefined = $state();
  let taskButton: HTMLButtonElement | undefined = $state();
  let top = $state(0);
  let left = $state(0);
  let choosing = $state(untrack(() => picking));
  let entry = $state("");
  let error = $state("");
  let errorField = $state<Field | null>(null);
  let confirming = $state(false);
  let entryTimer: number | undefined;
  // `document.activeElement` is already `body` in onDestroy, so focus is tracked as it moves.
  let focusInside = false;
  // A pending log entry is flushed on destroy, when `block` may already be gone.
  let latest = untrack(() => block);

  const draft = $derived(block.note === "");
  const running = $derived(block.span.end === null);
  const hours = $derived(workedHours([{ start: block.span.start, end: block.span.end ?? now }]));
  // A save re-points `block` while the user may be typing a time, so each field resets only when its own value moved.
  const shownStart = $derived(block.span.start);
  const shownEnd = $derived(block.span.end);
  let startText = $derived(formatClockDigits(shownStart));
  let endText = $derived(shownEnd === null ? "" : formatClockDigits(shownEnd));

  $effect(() => {
    latest = block;
    if (entryTimer === undefined) {
      entry = block.entry;
    }
  });

  $effect(() => {
    void [anchor, block, choosing, error];
    place();
  });

  // The picker focuses its own search field, so the popup only takes focus when the picker is closed.
  onMount(() => {
    if (!choosing) {
      (draft ? taskButton : startInput)?.focus();
    }
  });

  onDestroy(() => {
    if (entryTimer !== undefined) {
      void saveEntry();
    }
    // Focus returns only from the popup, so a click on › or Day/Week keeps its own focus.
    if (focusInside && anchor.isConnected) {
      anchor.focus();
    }
  });

  function place(): void {
    if (card === undefined) {
      return;
    }

    const frame = (card.offsetParent ?? document.body).getBoundingClientRect();
    const target = anchor.getBoundingClientRect();
    const below = target.bottom + GAP_PX;
    const above = target.top - GAP_PX - card.offsetHeight;
    const fitsBelow = below + card.offsetHeight <= window.innerHeight - MARGIN_PX;
    const y = fitsBelow || above < MARGIN_PX ? below : above;
    const x = Math.max(frame.left + MARGIN_PX, Math.min(target.left, frame.right - card.offsetWidth - MARGIN_PX));

    top = y - frame.top;
    left = x - frame.left;
  }

  async function send(field: Field, change: Partial<Block>, typing = false): Promise<void> {
    const failure = await onEdit(change, typing);
    error = failure;
    errorField = failure === "" ? null : field;
  }

  // The core checks the span itself, so the popup only reads the text.
  function commitTimes(field: "start" | "end"): void {
    const start = parseClock(startText, logicalDay(latest.span.start, dayStartHour), dayStartHour);
    const keepsRunning = running && endText.trim() === "";
    const extent = { start: latest.span.start, end: latest.span.end ?? now };
    const end = keepsRunning || start === null ? null : parseEnd(endText, start, extent);
    if (start === null || (end === null && !keepsRunning)) {
      error = "Write the time as HHmm, such as 0930.";
      errorField = start === null ? "start" : "end";
      return;
    }

    void send(field, { span: { start, end } });
  }

  function typeEntry(): void {
    window.clearTimeout(entryTimer);
    entryTimer = window.setTimeout(() => void saveEntry(), ENTRY_SAVE_MS);
  }

  async function saveEntry(): Promise<void> {
    window.clearTimeout(entryTimer);
    entryTimer = undefined;
    if (entry !== latest.entry) {
      await send("entry", { entry }, true);
    }
  }

  async function pick(note: TaskNote): Promise<void> {
    choosing = false;
    await send("task", { note: note.path });
  }

  async function remove(): Promise<void> {
    confirming = false;
    error = await onDelete();
  }

  function enterCommits(event: KeyboardEvent, field: "start" | "end"): void {
    if (event.key === "Enter") {
      commitTimes(field);
    }
  }
</script>

<svelte:window onresize={place} />

<div
  bind:this={card}
  class="pc:absolute pc:z-10 pc:box-border pc:w-[19rem] pc:rounded-md pc:border pc:border-solid pc:border-rule pc:bg-card pc:p-3 pc:shadow-[0_8px_24px_rgb(0_0_0/.45)]"
  style:top="{top}px"
  style:left="{left}px"
  role="dialog"
  aria-label={note?.name ?? "New block"}
  onfocusin={() => (focusInside = true)}
  onfocusout={(event) => {
    // Removing the popup also fires focusout, with no relatedTarget, so only a real move counts.
    if (event.relatedTarget instanceof Node) {
      focusInside = card?.contains(event.relatedTarget) ?? false;
    }
  }}
  data-popup
>
  <div class="pc:flex pc:items-center pc:gap-2 pc:pb-3">
    <span class={["pc:size-2 pc:shrink-0 pc:rounded-sm", note === null ? "pc:bg-faint" : projectColor(note.color)]}></span>
    <button
      class={[
        "pc:-mx-1 pc:flex pc:min-w-0 pc:flex-1 pc:cursor-pointer pc:items-center pc:gap-1 pc:rounded pc:border-0 pc:px-1 pc:py-0.5 pc:text-left pc:font-ui pc:text-row pc:text-ink pc:hover:bg-hover",
        choosing ? "pc:bg-hover" : "pc:bg-transparent",
      ]}
      aria-haspopup="listbox"
      aria-expanded={choosing}
      bind:this={taskButton}
      data-task
      onclick={() => (choosing = !choosing)}
    >
      <span class={["pc:truncate", draft && "pc:text-faint"]}>{note?.name ?? "Choose a task"}</span>
      <ChevronDown class="pc:shrink-0 pc:text-faint" size={14} />
    </button>
    {#if !draft}
      <button
        class="pc:cursor-pointer pc:border-0 pc:bg-transparent pc:p-0 pc:font-ui pc:text-row pc:text-faint pc:hover:text-ink"
        title="Open note"
        aria-label="Open note"
        onclick={() => openNote(latest.note)}><ArrowUpRight class="pc:block" size={14} /></button
      >
    {/if}
    <button
      class="pc:cursor-pointer pc:border-0 pc:bg-transparent pc:p-0 pc:font-ui pc:text-row pc:text-faint pc:hover:text-ink"
      aria-label="Close"
      onclick={onClose}><X class="pc:block" size={14} /></button
    >
  </div>

  {#if choosing}
    <NotePicker {notes} current={block.note} onPick={(note) => void pick(note)} onClose={() => (choosing = false)} />
  {/if}

  <div class="pc:grid pc:grid-cols-2 pc:gap-2 pc:pb-2">
    <label class="pc:flex pc:flex-col pc:gap-1 pc:text-label pc:text-faint"
      >Start
      <input
        bind:this={startInput}
        bind:value={startText}
        disabled={draft}
        onblur={() => commitTimes("start")}
        onkeydown={(event) => enterCommits(event, "start")}
        class={[
          "pc:box-border pc:w-full pc:rounded pc:border pc:border-solid pc:bg-surface pc:px-2 pc:py-1 pc:font-ui pc:text-row pc:text-ink pc:tabular-nums pc:outline-none pc:focus:border-now",
          errorField === "start" ? "pc:border-danger" : "pc:border-rule",
        ]}
        data-field="start"
      />
    </label>
    <label class="pc:flex pc:flex-col pc:gap-1 pc:text-label pc:text-faint"
      >End
      <input
        bind:value={endText}
        disabled={draft}
        placeholder={running ? "Running" : ""}
        onblur={() => commitTimes("end")}
        onkeydown={(event) => enterCommits(event, "end")}
        class={[
          "pc:box-border pc:w-full pc:rounded pc:border pc:border-solid pc:bg-surface pc:px-2 pc:py-1 pc:font-ui pc:text-row pc:text-ink pc:tabular-nums pc:outline-none pc:focus:border-now",
          errorField === "end" ? "pc:border-danger" : "pc:border-rule",
        ]}
        data-field="end"
      />
    </label>
  </div>

  {#if error !== ""}
    <p class="pc:m-0 pc:pb-2 pc:text-label pc:text-danger" role="alert">{error}</p>
  {/if}

  <label class="pc:flex pc:flex-col pc:gap-1 pc:pb-3 pc:text-label pc:text-faint"
    >Log
    <textarea
      bind:value={entry}
      disabled={draft}
      rows="3"
      oninput={typeEntry}
      onblur={() => void saveEntry()}
      class="pc:box-border pc:w-full pc:resize-none pc:rounded pc:border pc:border-solid pc:border-rule pc:bg-surface pc:px-2 pc:py-1 pc:font-ui pc:text-row pc:text-ink pc:outline-none pc:focus:border-now"
      data-field="entry"
    ></textarea>
  </label>

  <div class="pc:flex pc:items-center pc:gap-2 pc:text-label">
    <span class="pc:tabular-nums pc:text-faint">{formatHours(hours)}</span>
    {#if draft}
      <span class="pc:ml-auto pc:text-faint">Not saved until a task is chosen</span>
    {:else if confirming}
      <span class="pc:ml-auto pc:text-muted">Delete this block?</span>
      <button
        class="pc:cursor-pointer pc:rounded pc:border-0 pc:bg-transparent pc:px-2 pc:py-1 pc:font-ui pc:text-label pc:text-muted pc:hover:bg-hover"
        onclick={() => (confirming = false)}>Keep</button
      >
      <button
        class="pc:cursor-pointer pc:rounded pc:border-0 pc:bg-transparent pc:px-2 pc:py-1 pc:font-ui pc:text-label pc:text-danger pc:hover:bg-danger/10"
        onclick={() => void remove()}>Delete</button
      >
    {:else}
      <button
        class="pc:ml-auto pc:cursor-pointer pc:rounded pc:border-0 pc:bg-transparent pc:px-2 pc:py-1 pc:font-ui pc:text-label pc:text-danger pc:hover:bg-danger/10"
        onclick={() => (confirming = true)}>Delete block</button
      >
    {/if}
  </div>
</div>
