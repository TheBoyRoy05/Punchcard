<script lang="ts">
  import { matches } from "../match";
  import type { TaskNote } from "./host";
  import { projectColor } from "./palette";

  // The caller places the list and sizes it; the list fills that box.
  interface Props {
    notes: TaskNote[];
    current: string;
    label: string;
    onPick: (note: TaskNote) => void;
    onClose: () => void;
  }

  let { notes, current, label, onPick, onClose }: Props = $props();
  const listId = $props.id();

  let query = $state("");
  let selected = $state(0);
  let input: HTMLInputElement | undefined = $state();

  const shown = $derived(notes.filter((note) => matches(query, note.name)));

  $effect(() => input?.focus());

  function choose(note: TaskNote | undefined): void {
    if (note === undefined || note.path === current) {
      onClose();
    } else {
      onPick(note);
    }
  }

  // Esc stops here, so it closes the list without also closing the popup.
  function keydown(event: KeyboardEvent): void {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      const step = event.key === "ArrowDown" ? 1 : -1;
      selected = Math.min(Math.max(selected + step, 0), Math.max(shown.length - 1, 0));
      event.preventDefault();
    } else if (event.key === "Enter") {
      // A held Enter would choose again from whatever list its repeats land on, and an IME's Enter only confirms text.
      if (!event.repeat && !event.isComposing) {
        choose(shown[selected]);
      }
      event.preventDefault();
    } else if (event.key === "Escape") {
      onClose();
      event.stopPropagation();
      event.preventDefault();
    }
  }
</script>

<div
  class="pc:box-border pc:flex pc:h-full pc:flex-col pc:rounded-md pc:border pc:border-solid pc:border-rule pc:bg-card pc:p-2 pc:shadow-[0_8px_24px_rgb(0_0_0/.45)]"
>
  <input
    bind:this={input}
    bind:value={query}
    oninput={() => (selected = 0)}
    onkeydown={keydown}
    class="pc:box-border pc:w-full pc:rounded pc:border pc:border-solid pc:border-now pc:bg-surface pc:px-2 pc:py-1 pc:font-ui pc:text-row pc:text-ink pc:outline-none"
    aria-label={label}
    aria-controls={listId}
    aria-activedescendant={shown.length > 0 ? `${listId}-${selected}` : undefined}
    placeholder="Task"
    autocomplete="off"
    spellcheck="false"
  />
  <ul
    id={listId}
    role="listbox"
    aria-label="Tasks"
    class="pc:m-0 pc:flex pc:min-h-0 pc:flex-1 pc:list-none pc:flex-col pc:gap-0.5 pc:overflow-y-auto pc:p-0 pc:pt-2"
  >
    {#each shown as note, index (note.path)}
      <!-- The search field keeps focus and takes the keys, naming the selected option through aria-activedescendant. -->
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <li
        id="{listId}-{index}"
        role="option"
        aria-selected={index === selected}
        class={[
          "pc:flex pc:cursor-default pc:items-center pc:gap-2 pc:rounded pc:px-2 pc:py-1",
          index === selected && "pc:bg-hover",
        ]}
        onmousedown={(event) => event.preventDefault()}
        onclick={() => choose(note)}
      >
        <span class={["pc:size-2 pc:shrink-0 pc:rounded-sm", projectColor(note.color)]}></span>
        <span class={["pc:min-w-0 pc:flex-1 pc:truncate", note.path === current && "pc:text-faint"]}>{note.name}</span>
        {#if note.running}
          <span class="pc:text-label pc:text-now">Running</span>
        {/if}
        {#if note.path === current}
          <span class="pc:text-label pc:text-faint">Current</span>
        {/if}
      </li>
    {:else}
      <li class="pc:px-2 pc:py-1 pc:text-faint">No matching task</li>
    {/each}
  </ul>
</div>
