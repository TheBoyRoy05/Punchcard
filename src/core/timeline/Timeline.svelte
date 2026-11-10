<script lang="ts">
  import ChevronLeft from "@lucide/svelte/icons/chevron-left";
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import Play from "@lucide/svelte/icons/play";
  import Square from "@lucide/svelte/icons/square";
  import { onDestroy, tick, untrack } from "svelte";

  import { clip, dayWindow, editBlock, logicalDay, type Block, type ClosedSpan, type EditResult, type Minutes } from "../../wasm/core";
  import BlockPopup from "./BlockPopup.svelte";
  import { History, planChanges, sameBlock, type Change } from "./history";
  import type { LoadedRange, Range, TaskNote, TimelineHost } from "./host";
  import NotePicker from "./NotePicker.svelte";
  import {
    dragSpan,
    formatClock,
    formatDay,
    formatHours,
    loggedHours,
    percentOf,
    pixelsToMinutes,
    shiftDay,
    snap,
    weekOf,
    type DragMode,
  } from "./layout";
  import { projectColor } from "./palette";

  const NOW_REFRESH_MS = 30_000;
  const CLICK_SLOP_PX = 3;
  const MIN_LABEL_PX = 48;
  // Below this width a block has no room for two edge handles and a body, so a press has one meaning instead.
  const TINY_BLOCK_PX = 18;
  const NARROW_TRACK_PX = 480;
  const HOUR_LINES = Array.from({ length: 24 }, (_, hour) => hour);
  const HOUR_LABELS = Array.from({ length: 13 }, (_, index) => index * 2);

  interface Piece {
    block: Block;
    span: ClosedSpan;
    fromBefore: boolean;
    pastEnd: boolean;
  }

  interface Row {
    key: string;
    label: string;
    note: TaskNote | null;
    window: ClosedSpan;
    pieces: Piece[];
    hours: number;
    running: boolean;
  }

  interface Drag {
    block: Block;
    mode: DragMode;
    x: number;
    width: number;
    preview: Block;
    moved: boolean;
  }

  interface Landing {
    block: Block;
    preview: Block;
  }

  interface Popup {
    block: Block;
    opening: number;
    picking: boolean;
  }

  interface Outcome {
    error: string;
    saved: Block | undefined;
  }

  let {
    load,
    notes,
    dayStartHour,
    blockMinutes,
    blockMinMinutes,
    now: readNow,
    save,
    toggle,
    openNote,
    initialRange = "day",
  }: TimelineHost & { initialRange?: Range } = $props();

  let now = $state(untrack(() => readNow()));
  let range = $state<Range>(untrack(() => initialRange));
  let day = $state(untrack(() => logicalDay(readNow(), dayStartHour)));
  let blocks = $state<Block[]>([]);
  let rangeNotes = $state<TaskNote[]>([]);
  // Running states the toggles left, kept until a load reports the note again, since its new block may be outside the range.
  let toggled = $state.raw(new Map<string, boolean>());
  let notice = $state("");
  let trackWidth = $state(0);
  let drag = $state<Drag | null>(null);
  // Released drags whose save is still queued. Raw, because a deep proxy breaks the identity check that removes an entry.
  let landing = $state.raw<Landing[]>([]);
  let popup = $state<Popup | null>(null);
  let anchor = $state<HTMLElement | null>(null);
  let frame: HTMLElement | undefined = $state();
  let dayPicks = $state(0);
  let openings = 0;
  let loads = 0;
  let justDragged = false;
  let toggling = false;
  let queue: Promise<unknown> = Promise.resolve();
  const history = new History();

  const days = $derived(range === "day" ? [day] : weekOf(day, dayStartHour));
  const firstDay = $derived(days[0] ?? day);
  const lastDay = $derived(days[days.length - 1] ?? day);
  // The picker notes are included, so a block just moved to one resolves before the reload lands.
  const notesByPath = $derived(
    new Map(
      [...notes, ...rangeNotes].map((note) => {
        const running = toggled.get(note.path) ?? note.running;
        return [note.path, { ...note, running }];
      })
    )
  );
  // The host's list in its rank order, with each note's newest running state.
  const pickerNotes = $derived(notes.map((note) => notesByPath.get(note.path) ?? note));
  // A reload during a drag or its save replaces the block's object, so it is found by key.
  const drawn = $derived(blocks.map((block) => moving().find((moved) => keyOf(block) === keyOf(moved.block))?.preview ?? block));
  const rows = $derived(range === "day" ? noteRows() : dayRows());
  const total = $derived(loggedHours(drawn, days, dayStartHour, now));
  const title = $derived(
    range === "day"
      ? formatDay(day, dayStartHour, "full")
      : `${formatDay(firstDay, dayStartHour, "date")} - ${formatDay(lastDay, dayStartHour, "date")}`
  );
  const nowPercent = $derived(range === "day" ? percentOf(now, dayWindow(day, dayStartHour)) : null);
  const narrow = $derived(trackWidth < NARROW_TRACK_PX);

  $effect(() => {
    const timer = window.setInterval(() => (now = readNow()), NOW_REFRESH_MS);
    return () => window.clearInterval(timer);
  });

  // Queued, so a popup's entry flushed on a ‹ › click saves against the blocks it was typed over, not the next range's.
  $effect(() => refresh(firstDay, lastDay));

  // A host awaits this before it closes, so the open popup's unsaved entry and every queued save land first.
  // Returns the notice those saves left, "" when they all landed.
  export async function settle(): Promise<string> {
    notice = "";
    popup = null;
    await tick();
    await queued(() => Promise.resolve());
    return notice;
  }

  // A host calls this when the notes may have changed outside the timeline, such as when its window regains focus.
  export function refresh(first = firstDay, last = lastDay): void {
    void queued(() => reload(first, last)).then((error) => {
      if (error !== "") {
        notice = error;
      }
    });
  }

  onDestroy(stopTracking);

  async function reload(first = firstDay, last = lastDay): Promise<string> {
    loads += 1;
    const ticket = loads;
    let loaded: LoadedRange;
    try {
      loaded = await load(first, last);
    } catch (error) {
      return messageOf(error);
    }

    // Quick ‹ › clicks start several loads, and only the last one started matches the range shown.
    if (ticket !== loads) {
      return "";
    }
    blocks = loaded.blocks;
    rangeNotes = loaded.notes;
    const reported = new Set(loaded.notes.map((note) => note.path));
    toggled = new Map([...toggled].filter(([path]) => !reported.has(path)));

    // A popup whose block moved is re-pointed by the edit that moved it, so only a same-key block refreshes here.
    const open = popup;
    const shown = open === null ? undefined : blocks.find((block) => keyOf(block) === keyOf(open.block));
    if (open !== null && shown !== undefined) {
      open.block = shown;
    }
    await placeAnchor();
    return "";
  }

  function noteRows(): Row[] {
    const window = dayWindow(day, dayStartHour);
    const groups: Piece[][] = [];
    for (const piece of piecesIn(window)) {
      const group = groups.find(([first]) => first?.block.note === piece.block.note);
      if (group === undefined) {
        groups.push([piece]);
      } else {
        group.push(piece);
      }
    }

    return groups
      .map((pieces) => {
        const [first] = pieces;
        const path = first?.block.note ?? "";
        const note = noteOf(path);
        return {
          key: path,
          label: note.name,
          note,
          window,
          pieces,
          hours: loggedHours(
            drawn.filter((block) => block.note === path),
            [day],
            dayStartHour,
            now
          ),
          running: pieces.some((piece) => piece.block.span.end === null),
        };
      })
      .sort((a, b) => (a.note?.project ?? "").localeCompare(b.note?.project ?? "") || a.label.localeCompare(b.label));
  }

  function dayRows(): Row[] {
    return days.map((shown) => {
      const window = dayWindow(shown, dayStartHour);
      const pieces = piecesIn(window);
      return {
        key: shown,
        label: formatDay(shown, dayStartHour, "weekday"),
        note: null,
        window,
        pieces,
        hours: loggedHours(drawn, [shown], dayStartHour, now),
        running: pieces.some((piece) => piece.block.span.end === null),
      };
    });
  }

  function piecesIn(window: ClosedSpan): Piece[] {
    const shown = popup?.block.note === "" ? [...drawn, popup.block] : drawn;

    return shown.flatMap((block) => {
      const span = clip(block.span, window, now);
      if (span === null) {
        return [];
      }
      const fromBefore = block.span.start < window.start;
      const pastEnd = (block.span.end ?? now) > window.end;
      return [{ block, span, fromBefore, pastEnd }];
    });
  }

  function noteOf(path: string): TaskNote {
    return notesByPath.get(path) ?? { name: path, path, project: "", color: 0, running: false };
  }

  function blockAt(note: string, time: Minutes): Block | undefined {
    return blocks.find((block) => block.note === note && block.span.start <= time && time < (block.span.end ?? now));
  }

  function keyOf(block: Block): string {
    return `${block.note}|${block.span.start}`;
  }

  // A moved block keeps its key until its save lands, so its button, pointer capture and post-drag click survive a moved start.
  function pieceKey(piece: Piece): string {
    return keyOf(moving().find((moved) => moved.preview === piece.block)?.block ?? piece.block);
  }

  function moving(): Landing[] {
    return drag === null ? landing : [drag, ...landing];
  }

  function isLanding(block: Block): boolean {
    return landing.some((moved) => moved.preview === block);
  }

  function pieceStyle(piece: Piece, window: ClosedSpan): { left: number; width: number } {
    const left = percentOf(piece.span.start, window);
    return { left, width: percentOf(piece.span.end, window) - left };
  }

  // A block redrawn under a new key keeps the popup on its old spot until the edit re-points it.
  async function placeAnchor(): Promise<void> {
    await tick();
    const key = popup === null ? null : CSS.escape(keyOf(popup.block));
    anchor = key === null ? null : (frame?.querySelector<HTMLElement>(`[data-key="${key}"]`) ?? anchor);
  }

  // One save at a time, so every edit is checked against the blocks the saves before it left.
  function queued<T>(step: () => Promise<T>): Promise<T> {
    const run = queue.then(step);
    queue = run.catch(() => undefined);
    return run;
  }

  // Returns the writes that landed, so undo can still revert a task move that stopped halfway.
  async function send(changes: Change[]): Promise<{ error: string; done: Change[] }> {
    const done: Change[] = [];
    let error = "";
    try {
      for (const change of changes) {
        const { original, block } = change;
        const result = await save(original, block);
        if (!result.ok) {
          error = result.error;
          break;
        }
        done.push(change);
      }
    } catch (failure) {
      error = messageOf(failure);
    }

    const reloadError = await reload();
    return { error: error || reloadError, done };
  }

  async function apply(changes: Change[], group?: number): Promise<string> {
    const { error, done } = await send(changes);
    if (done.length > 0) {
      history.push(done, group);
    }
    return error;
  }

  async function saveEdit(result: EditResult, next: Block, group?: number): Promise<Outcome> {
    if (!result.ok) {
      return { error: result.error, saved: undefined };
    }

    const error = await apply(planChanges(blocks, result.blocks), group);
    return { error, saved: error === "" ? blockAt(next.note, next.span.start) : undefined };
  }

  // The block is looked up again, since a save earlier in the queue may have replaced it.
  async function commitEdit(original: Block, change: Partial<Block>, group?: number): Promise<Outcome> {
    const current = blocks.find((block) => keyOf(block) === keyOf(original)) ?? original;
    const next = { ...current, ...change };
    if (sameBlock(current, next)) {
      return { error: "", saved: current };
    }
    return saveEdit(editBlock(blocks, current, next, readNow(), blockMinMinutes), next, group);
  }

  // An add is an edit of a block already in the list, so it gets the same checks as any edit.
  async function commitAdd(block: Block): Promise<Outcome> {
    return saveEdit(editBlock([...blocks, block], block, block, readNow(), blockMinMinutes), block);
  }

  async function undo(): Promise<void> {
    popup = null;
    // Closing the popup queues its pending log entry, which must land before the undo pops history.
    await tick();

    void queued(async () => {
      const changes = history.peek();
      if (changes === undefined) {
        return;
      }

      // A failed write stays in history, so the user can undo it again.
      const { error, done } = await send(changes);
      history.drop(done.length);
      notice = error;
    });
  }

  function changeOpen(open: Popup, change: Partial<Block>, group?: number): Promise<string> {
    return queued(async () => {
      const draft = open.block.note === "";
      const { error, saved } = draft ? await commitAdd({ ...open.block, ...change }) : await commitEdit(open.block, change, group);
      if (saved !== undefined) {
        open.block = saved;
      }
      if (popup === open) {
        await placeAnchor();
      } else if (error !== "") {
        // A closed popup flushed its entry and has no field left to show why the save was refused.
        notice = error;
      }
      return error;
    });
  }

  function remove(open: Popup): Promise<string> {
    return queued(async () => {
      const error = await apply([{ original: open.block, block: null }]);
      if (error === "" && popup === open) {
        popup = null;
      }
      return error;
    });
  }

  // A second toggle while the first is in flight would stop the timer it just started.
  function startOrStop(note: TaskNote): void {
    if (toggling) {
      return;
    }

    toggling = true;
    void queued(async () => {
      let error = "";
      try {
        const result = await toggle(note.path);
        if (result.ok) {
          toggled = new Map([...toggled, [note.path, result.running]]);
        } else {
          error = result.error;
        }
      } catch (failure) {
        error = messageOf(failure);
      }

      const reloadError = await reload();
      notice = error || reloadError;
      toggling = false;
    });
  }

  // The day view's picker stays open, so a pick starts it over with an empty search.
  function pickInDay(note: TaskNote): void {
    // A pick during a toggle is ignored, so it keeps its search too.
    if (toggling) {
      return;
    }
    dayPicks += 1;
    startOrStop(note);
  }

  async function openPopup(block: Block, picking: boolean): Promise<void> {
    openings += 1;
    popup = { block, opening: openings, picking };
    anchor = null;
    await placeAnchor();
  }

  // Window listeners keep the drag alive wherever the pointer is released.
  // A tiny block has no handles: dragging a running one moves its start, its only movable edge, and dragging a closed one moves it.
  function startDrag(event: PointerEvent, piece: Piece, tiny: boolean): void {
    justDragged = false;
    const edge = (event.target as HTMLElement).dataset.edge as DragMode | undefined;
    const mode = edge ?? (tiny && piece.block.span.end === null && !piece.fromBefore ? "start" : "move");
    // A block whose drag is still saving waits, since its save would overwrite a second drag's.
    const blocked = drag !== null || isLanding(piece.block) || piece.block.note === "" || (mode === "move" && piece.block.span.end === null);
    if (event.button !== 0 || blocked) {
      return;
    }

    notice = "";
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
    drag = {
      block: piece.block,
      mode,
      x: event.clientX,
      width: trackWidth,
      preview: piece.block,
      moved: false,
    };

    window.addEventListener("pointermove", moveDrag);
    window.addEventListener("pointerup", endDrag);
    window.addEventListener("pointercancel", cancelDrag);
  }

  function moveDrag(event: PointerEvent): void {
    if (drag === null) {
      return;
    }

    const shift = event.clientX - drag.x;
    if (!drag.moved && Math.abs(shift) < CLICK_SLOP_PX) {
      return;
    }

    drag.moved = true;
    drag.preview = { ...drag.block, span: dragSpan(drag.block.span, drag.mode, pixelsToMinutes(shift, drag.width)) };
  }

  function endDrag(): void {
    stopTracking();
    if (drag === null) {
      return;
    }

    const { block, preview, moved } = drag;
    drag = null;
    if (!moved) {
      return;
    }

    // The preview stays drawn until the save lands, so the block does not jump back for a frame.
    justDragged = true;
    const released = { block, preview };
    landing = [...landing, released];
    void queued(async () => {
      try {
        // Another drag's save may land after this one, so only an error writes the notice.
        const { error } = await commitEdit(block, { span: preview.span });
        if (error !== "") {
          notice = error;
        }
      } finally {
        landing = landing.filter((moved) => moved !== released);
      }
    });
  }

  function cancelDrag(): void {
    stopTracking();
    drag = null;
  }

  function stopTracking(): void {
    window.removeEventListener("pointermove", moveDrag);
    window.removeEventListener("pointerup", endDrag);
    window.removeEventListener("pointercancel", cancelDrag);
  }

  // Only a pointer click (detail > 0) can end a drag, so Enter after a drag still opens the block.
  // A press on another block is left to that block, which opens its own popup or starts a drag.
  function closeOnPressOutside(event: PointerEvent): void {
    const target = event.target instanceof Element ? event.target : null;
    if (popup !== null && target?.closest("[data-popup], [data-key]") == null) {
      popup = null;
    }
  }

  function clickBlock(event: MouseEvent, block: Block): void {
    const endsDrag = justDragged && event.detail > 0;
    justDragged = false;
    // A block still saving its drag would open on a preview that a failed save leaves behind.
    if (!endsDrag && block.note !== "" && !isLanding(block)) {
      void openPopup(block, false);
    }
  }

  function addAt(event: MouseEvent, row: Row): void {
    if (event.target !== event.currentTarget) {
      return;
    }

    const track = event.currentTarget as HTMLElement;
    const start = snap(row.window.start + pixelsToMinutes(event.offsetX, track.clientWidth));
    const span = { start, end: start + blockMinutes };
    if (row.note === null) {
      void openPopup({ note: "", span, entry: "" }, true);
      return;
    }

    const block = { note: row.note.path, span, entry: "" };
    void queued(async () => {
      const { error, saved } = await commitAdd(block);
      notice = error;
      if (saved !== undefined) {
        await openPopup(saved, false);
      }
    });
  }

  function navigate(step: number): void {
    popup = null;
    day = shiftDay(day, range === "day" ? step : step * 7, dayStartHour);
  }

  function keydown(event: KeyboardEvent): void {
    if (event.key === "Escape" && drag !== null) {
      cancelDrag();
      justDragged = true;
      event.preventDefault();
      return;
    }

    if (event.key === "Escape" && popup !== null) {
      popup = null;
      event.preventDefault();
      return;
    }

    // Ctrl+Z on a Mac and Cmd+Z elsewhere mean nothing else, so either one undoes.
    // An empty task search has nothing of its own to undo, and the day view's search usually holds focus.
    const emptySearch = event.target instanceof HTMLInputElement && "search" in event.target.dataset && event.target.value === "";
    const typing = !emptySearch && (event.target instanceof HTMLInputElement || event.target instanceof HTMLTextAreaElement);
    const command = event.metaKey || event.ctrlKey;
    if (command && !event.shiftKey && event.key.toLowerCase() === "z" && !typing) {
      event.preventDefault();
      void undo();
    }
  }

  function messageOf(error: unknown): string {
    return error instanceof Error ? error.message : String(error);
  }
</script>

<svelte:window onkeydown={keydown} onpointerdown={closeOnPressOutside} />

<div class="pc:@container pc:font-ui pc:text-row pc:text-ink">
  <section bind:this={frame} class="pc:relative pc:mx-auto pc:w-full pc:max-w-[1100px] pc:box-border pc:p-4">
    <header class="pc:flex pc:items-baseline pc:gap-4 pc:pb-3">
      <h1 class="pc:m-0 pc:text-head pc:font-semibold">{title}</h1>
      <span class="pc:tabular-nums pc:text-muted" data-total>{formatHours(total)}</span>
      <nav class="pc:ml-auto pc:flex pc:items-center pc:gap-1 pc:text-label">
        <button
          class="pc:cursor-pointer pc:rounded pc:border-0 pc:bg-transparent pc:px-2 pc:py-1 pc:font-ui pc:text-label pc:text-muted pc:hover:bg-hover"
          aria-label="Previous"
          onclick={() => navigate(-1)}><ChevronLeft class="pc:block" size={14} /></button
        >
        <span class="pc:flex pc:overflow-hidden pc:rounded pc:border pc:border-solid pc:border-rule">
          {#each ["day", "week"] as const as option (option)}
            <button
              class={[
                "pc:cursor-pointer pc:border-0 pc:px-2 pc:py-1 pc:font-ui pc:text-label pc:capitalize",
                range === option ? "pc:bg-hover pc:text-ink" : "pc:bg-transparent pc:text-muted pc:hover:bg-hover",
              ]}
              aria-pressed={range === option}
              onclick={() => {
                popup = null;
                range = option;
              }}>{option}</button
            >
          {/each}
        </span>
        <button
          class="pc:cursor-pointer pc:rounded pc:border-0 pc:bg-transparent pc:px-2 pc:py-1 pc:font-ui pc:text-label pc:text-muted pc:hover:bg-hover"
          aria-label="Next"
          onclick={() => navigate(1)}><ChevronRight class="pc:block" size={14} /></button
        >
      </nav>
    </header>

    {#if notice !== ""}
      <p class="pc:m-0 pc:pb-2 pc:text-label pc:text-danger" role="alert">{notice}</p>
    {/if}

    <div class="pc:relative pc:grid pc:grid-cols-[16rem_1fr] pc:@max-[700px]:grid-cols-[12rem_1fr]">
      <div class="pc:border-0 pc:border-b pc:border-solid pc:border-rule"></div>
      <div
        class="pc:relative pc:h-6 pc:border-0 pc:border-b pc:border-solid pc:border-rule pc:text-label pc:text-faint pc:tabular-nums"
        bind:clientWidth={trackWidth}
        data-axis
      >
        {#each HOUR_LABELS as offset (offset)}
          {#if !narrow || offset % 4 === 0}
            <span
              class={[
                "pc:absolute pc:bottom-1",
                offset === 0 ? "pc:translate-x-0" : offset === 24 ? "pc:-translate-x-full" : "pc:-translate-x-1/2",
              ]}
              style:left="{(offset / 24) * 100}%">{String((dayStartHour + offset) % 24).padStart(2, "0")}</span
            >
          {/if}
        {/each}
        {#if nowPercent !== null && nowPercent >= 0 && nowPercent <= 100}
          <div
            class="pc:absolute pc:-bottom-[4px] pc:size-[7px] pc:-translate-x-1/2 pc:rounded-full pc:bg-now"
            style:left="{nowPercent}%"
          ></div>
        {/if}
      </div>

      {#each rows as row, index (row.key)}
        {#if range === "day" && row.note !== null && row.note.project !== rows[index - 1]?.note?.project}
          <div class="pc:col-span-2 pc:flex pc:items-center pc:gap-2 pc:pt-3 pc:pb-1 pc:text-label pc:text-muted">
            <span class={["pc:size-2 pc:rounded-sm", projectColor(row.note.color)]}></span>
            {row.note.project}
          </div>
        {/if}

        <div
          class={[
            "pc:flex pc:h-[34px] pc:min-w-0 pc:items-center pc:gap-2 pc:border-0 pc:border-b pc:border-solid pc:border-rule/60 pc:pr-3",
            range === "day" && "pc:pl-4 pc:@max-[700px]:pl-2",
          ]}
          data-row={row.key}
        >
          {#if row.note !== null}
            {@const note = row.note}
            <button
              class={[
                "pc:shrink-0 pc:cursor-pointer pc:rounded pc:border-0 pc:bg-transparent pc:p-0.5 pc:hover:bg-hover",
                note.running ? "pc:text-now" : "pc:text-faint pc:hover:text-ink",
              ]}
              aria-label={`${note.running ? "Stop" : "Start"} ${row.label}`}
              data-toggle
              onclick={() => startOrStop(note)}
            >
              {#if note.running}
                <Square class="pc:block" size={12} />
              {:else}
                <Play class="pc:block" size={12} />
              {/if}
            </button>
            <button
              class="pc:min-w-0 pc:flex-1 pc:cursor-pointer pc:truncate pc:border-0 pc:bg-transparent pc:p-0 pc:text-left pc:font-ui pc:text-row pc:text-ink pc:hover:underline"
              title={row.label}
              onclick={() => openNote(note.path)}>{row.label}</button
            >
          {:else}
            <span class="pc:flex-1 pc:truncate">{row.label}</span>
          {/if}
          <span class={["pc:tabular-nums", row.running ? "pc:text-now" : "pc:text-faint"]} data-hours
            >{formatHours(row.hours)}</span
          >
        </div>

        <div
          class="pc:relative pc:h-[34px] pc:border-0 pc:border-b pc:border-solid pc:border-rule/60"
          role="presentation"
          data-track={row.key}
          ondblclick={(event) => void addAt(event, row)}
        >
          {#each HOUR_LINES as hour (hour)}
            <div
              class="pc:pointer-events-none pc:absolute pc:inset-y-0 pc:w-px pc:bg-rule"
              style:left="{(hour / 24) * 100}%"
            ></div>
          {/each}

          {#if nowPercent !== null && nowPercent >= 0 && nowPercent <= 100}
            <div
              class="pc:pointer-events-none pc:absolute pc:inset-y-0 pc:z-[1] pc:w-px pc:bg-now"
              style:left="{nowPercent}%"
              data-now
            ></div>
          {/if}

          {#each row.pieces as piece (pieceKey(piece))}
            {@const place = pieceStyle(piece, row.window)}
            {@const running = piece.block.span.end === null}
            {@const draft = piece.block.note === ""}
            {@const note = noteOf(piece.block.note)}
            {@const pixels = (place.width / 100) * trackWidth}
            {@const tiny = pixels < TINY_BLOCK_PX}
            {@const label = range === "week" && pixels >= MIN_LABEL_PX ? note.name : ""}
            <!-- Padding sits only beside a label, since a box cannot draw narrower than its padding and would run past its end. -->
            <!-- A tiny block keeps its true width but takes presses over a wider centred area; `truncate` would clip that area. -->
            <button
              class={[
                "pc:absolute pc:inset-y-1.5 pc:box-border pc:min-w-[2px] pc:rounded-[3px] pc:border-0 pc:p-0 pc:text-left pc:font-ui pc:text-label pc:font-medium pc:leading-[22px] pc:whitespace-nowrap pc:text-surface pc:hover:brightness-110",
                label !== "" && "pc:truncate pc:px-1.5",
                tiny &&
                  "pc:before:absolute pc:before:inset-y-0 pc:before:left-1/2 pc:before:w-3 pc:before:-translate-x-1/2 pc:before:content-['']",
                tiny && running && !piece.fromBefore ? "pc:cursor-ew-resize" : "pc:cursor-grab",
                draft ? "pc:border pc:border-dashed pc:border-ink pc:bg-transparent" : projectColor(note.color),
                running &&
                  "pc:rounded-r-none pc:bg-[image:repeating-linear-gradient(135deg,transparent_0_5px,rgb(0_0_0/.18)_5px_10px)]",
                piece.fromBefore && "pc:rounded-l-none",
                piece.pastEnd && "pc:rounded-r-none",
                popup !== null && keyOf(popup.block) === keyOf(piece.block) && "pc:outline-2 pc:outline-offset-2 pc:outline-ink",
              ]}
              style:left="{place.left}%"
              style:width="{place.width}%"
              title={range === "week" ? note.name : `${formatClock(piece.span.start)}-${formatClock(piece.span.end)}`}
              aria-label={`${draft ? "New block" : note.name}, ${formatClock(piece.block.span.start)}-${running ? "running" : formatClock(piece.block.span.end ?? now)}`}
              data-key={keyOf(piece.block)}
              onpointerdown={(event) => startDrag(event, piece, tiny)}
              onclick={(event) => clickBlock(event, piece.block)}
            >
              {#if !draft && !tiny && !piece.fromBefore}
                <!-- Above a neighbouring tiny block's wider hit area, so a press on a handle always resizes. -->
                <span class="pc:absolute pc:inset-y-0 pc:left-0 pc:z-[1] pc:w-1.5 pc:cursor-ew-resize" data-edge="start"></span>
              {/if}
              {label}
              {#if !draft && !tiny && !running && !piece.pastEnd}
                <span class="pc:absolute pc:inset-y-0 pc:right-0 pc:z-[1] pc:w-1.5 pc:cursor-ew-resize" data-edge="end"></span>
              {/if}
            </button>
          {/each}
        </div>
      {/each}

      <!-- Day rows are indented under their project, and the picker lines up with their play buttons. -->
      {#if range === "day"}
        <div class="pc:col-span-2 pc:pt-2 pc:pl-2 pc:@max-[700px]:pl-0">
          {#key dayPicks}
            <div class="pc:h-64">
              <NotePicker notes={pickerNotes} current="" label="Task to start or stop" onPick={pickInDay} />
            </div>
          {/key}
        </div>
      {/if}

      <!-- Inside the grid, so the popup clamps to the timeline's content edges, not the section's padding. -->
      {#if popup !== null && anchor !== null}
        {#key popup.opening}
          {@const open = popup}
          <BlockPopup
            block={open.block}
            note={open.block.note === "" ? null : noteOf(open.block.note)}
            {anchor}
            notes={pickerNotes}
            {now}
            {dayStartHour}
            picking={open.picking}
            onEdit={(change, typing) => changeOpen(open, change, typing ? open.opening : undefined)}
            onDelete={() => remove(open)}
            {openNote}
            onClose={() => (popup = null)}
          />
        {/key}
      {/if}
    </div>
  </section>
</div>
