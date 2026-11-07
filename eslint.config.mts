import obsidianmd from "eslint-plugin-obsidianmd";
import svelte from "eslint-plugin-svelte";
import globals from "globals";
import { globalIgnores, defineConfig } from "eslint/config";
import tseslint from "typescript-eslint";

export default defineConfig(
  globalIgnores([
    "node_modules",
    "esbuild.config.mjs",
    "scripts",
    "src/wasm/pkg",
    "main.js",
    "dist",
    "test-results",
    "src-tauri",
    "package.json",
    "package-lock.json",
    "tsconfig.json",
    ".worktrees",
  ]),
  {
    languageOptions: {
      globals: { ...globals.browser },
      parserOptions: {
        projectService: { allowDefaultProject: ["manifest.json"] },
        tsconfigRootDir: import.meta.dirname,
        extraFileExtensions: [".json", ".svelte"],
      },
    },
  },
  ...obsidianmd.configs.recommended,
  ...svelte.configs.recommended.map((config) => ({ ...config, files: ["**/*.svelte"] })),
  {
    files: ["**/*.svelte"],
    languageOptions: { parserOptions: { parser: tseslint.parser } },
    plugins: { "@typescript-eslint": tseslint.plugin },
    // The core rule misreads parameter names in function types, so .svelte files swap it as the .ts configs do.
    rules: { "no-unused-vars": "off", "@typescript-eslint/no-unused-vars": "error" },
  },
  // tsc already checks names, and no-undef misreads names resolved inside a `declare module` block.
  { files: ["**/*.ts"], rules: { "no-undef": "off" } },
  // The Tauri webview has no Obsidian globals such as createEl.
  { files: ["src/app/**/*.ts"], rules: { "obsidianmd/prefer-create-el": "off" } },
  // Tests and the Vite config run in Node, never on Obsidian mobile.
  { files: ["tests/**/*.ts", "vite.config.ts"], rules: { "obsidianmd/no-nodejs-modules": "off" } }
);
