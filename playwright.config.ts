import { defineConfig, devices } from "@playwright/test";

import { HARNESS } from "./tests/ui/fixture";

// A port of its own, apart from `tauri dev` on 1420, and never reused, so a server from another worktree is never tested.
const SERVER = "http://127.0.0.1:1421";

export default defineConfig({
  testDir: "tests/ui",
  forbidOnly: true,
  use: { ...devices["Desktop Chrome"], baseURL: SERVER, viewport: { width: 1200, height: 800 } },
  // Screenshots only write PNGs for a person to look at, so `test:ui` runs the `ui` project alone.
  projects: [
    { name: "ui", testMatch: "*.ui.ts" },
    { name: "screenshots", testMatch: "*.shot.ts" },
  ],
  // The server has no index page, so readiness is checked on the harness itself.
  webServer: { command: "npm run dev -- --port 1421", url: SERVER + HARNESS, reuseExistingServer: false },
});
