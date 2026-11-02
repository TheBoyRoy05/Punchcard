import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

import { matches } from "./match";

interface Task {
  name: string;
  running: boolean;
}

const query = document.getElementById("query") as HTMLInputElement;
const list = document.getElementById("tasks") as HTMLUListElement;
const error = document.getElementById("error") as HTMLParagraphElement;

let tasks: Task[] = [];
let shown: Task[] = [];
let selected = 0;
let toggling = false;

function render(): void {
  shown = tasks.filter((task) => matches(query.value, task.name));
  selected = Math.min(selected, Math.max(shown.length - 1, 0));

  const items = shown.map((task, index) => {
    const item = document.createElement("li");
    item.textContent = task.name;
    item.classList.toggle("running", task.running);
    item.classList.toggle("selected", index === selected);
    // Keeps focus in the query box, so typing still filters after a click.
    item.addEventListener("mousedown", (event) => event.preventDefault());
    item.addEventListener("click", () => void toggle(task));
    return item;
  });
  list.replaceChildren(...items);
  list.children[selected]?.scrollIntoView({ block: "nearest" });
}

function showError(message: unknown): void {
  error.textContent = String(message);
  error.hidden = false;
}

async function load(): Promise<void> {
  try {
    tasks = await invoke<Task[]>("tasks");
    error.hidden = true;
  } catch (message) {
    showError(message);
  }
  render();
}

// A second toggle while the first is in flight would stop the timer it just started.
async function toggle(task: Task): Promise<void> {
  if (toggling) {
    return;
  }

  toggling = true;
  try {
    await invoke("toggle", { name: task.name });
    query.value = "";
    selected = 0;
    await load();
  } catch (message) {
    showError(message);
  } finally {
    toggling = false;
  }
}

query.addEventListener("input", () => {
  selected = 0;
  render();
});

document.addEventListener("keydown", (event) => {
  if (event.key === "Escape") {
    void getCurrentWindow().close();
  } else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
    event.preventDefault();
    const step = event.key === "ArrowDown" ? 1 : -1;
    selected = Math.min(Math.max(selected + step, 0), Math.max(shown.length - 1, 0));
    render();
  } else if (event.key === "Enter" && !event.repeat && !event.isComposing) {
    const task = shown[selected];
    if (task !== undefined) {
      void toggle(task);
    }
  }
});

void load();
