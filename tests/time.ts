import { parseTime, type Minutes } from "../src/wasm/core";

// Test labels read "MM-DD HH:MM", always in 2026.
export function at(label: string): Minutes {
  const [date = "", clock = ""] = label.split(" ");
  return parseTime(`26/${date.replace("-", "/")} ${clock}`);
}
