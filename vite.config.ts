import { svelte } from "@sveltejs/vite-plugin-svelte";
import tailwindcss from "@tailwindcss/vite";
import { resolve } from "node:path";
import { configDefaults, defineConfig } from "vitest/config";

export default defineConfig({
  plugins: [svelte(), tailwindcss()],
  // Keeps Rust compile errors visible in the `tauri dev` terminal.
  clearScreen: false,
  server: {
    // devUrl in src-tauri/tauri.conf.json names this port, so Vite must not move to another one.
    port: 1420,
    strictPort: true,
    host: "127.0.0.1",
    watch: { ignored: ["**/src-tauri/**", "**/.worktrees/**"] },
  },
  build: {
    rolldownOptions: {
      input: [resolve(import.meta.dirname, "picker.html"), resolve(import.meta.dirname, "timeline.html")],
    },
  },
  test: { exclude: [...configDefaults.exclude, "**/.worktrees/**"] },
});
