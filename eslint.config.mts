import obsidianmd from "eslint-plugin-obsidianmd";
import globals from "globals";
import { globalIgnores, defineConfig } from "eslint/config";

export default defineConfig(
  globalIgnores([
    "node_modules",
    "esbuild.config.mjs",
    "main.js",
    "dist",
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
        extraFileExtensions: [".json"],
      },
    },
  },
  ...obsidianmd.configs.recommended,
  // tsc already checks names, and no-undef misreads names resolved inside a `declare module` block.
  { files: ["**/*.ts"], rules: { "no-undef": "off" } },
  // The Tauri webview has no Obsidian globals such as createEl.
  { files: ["src/app/**/*.ts"], rules: { "obsidianmd/prefer-create-el": "off" } }
);
