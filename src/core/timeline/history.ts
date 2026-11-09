import type { Block } from "../../wasm/core";

// One host write: no original adds `block`, no block deletes `original`.
export interface Change {
  original: Block | null;
  block: Block | null;
}

interface Entry {
  changes: Change[];
  group: number | undefined;
}

export class History {
  #entries: Entry[] = [];

  // Edits sharing a group, such as typing in one log entry, undo as one.
  push(changes: Change[], group?: number): void {
    const last = this.#entries[this.#entries.length - 1];
    const [first] = last?.changes ?? [];
    const [latest] = changes;
    if (group !== undefined && last?.group === group && first !== undefined && latest !== undefined) {
      last.changes = [{ original: first.original, block: latest.block }];
      return;
    }

    this.#entries.push({ changes, group });
  }

  // Returns the writes that revert the last edit, each sending the current block as its original.
  peek(): Change[] | undefined {
    return this.#entries
      .at(-1)
      ?.changes.map(({ original, block }) => ({ original: block, block: original }))
      .reverse();
  }

  // Forgets the first `count` writes peek returned, once they land, so a retry sends only the rest.
  drop(count: number): void {
    const last = this.#entries.at(-1);
    if (last === undefined) {
      return;
    }

    last.changes.splice(last.changes.length - count, count);
    if (last.changes.length === 0) {
      this.#entries.pop();
    }
  }
}

// A block that changes task is written to its new note before it leaves the old one, so a failed second write leaves a duplicate, never a lost block.
// The core returns fresh copies of every block, so blocks compare by value, not identity.
export function planChanges(before: Block[], after: Block[]): Change[] {
  const removed = before.filter((block) => !after.some((kept) => sameBlock(kept, block)));
  const writes = after
    .filter((block) => !before.some((kept) => sameBlock(kept, block)))
    .map((block) => {
      const replaced = removed.findIndex((original) => original.note === block.note);
      const [original = null] = replaced === -1 ? [] : removed.splice(replaced, 1);
      return { original, block };
    });

  return [...writes, ...removed.map((original) => ({ original, block: null }))];
}

export function sameBlock(a: Block, b: Block): boolean {
  return a.note === b.note && a.entry === b.entry && a.span.start === b.span.start && a.span.end === b.span.end;
}
