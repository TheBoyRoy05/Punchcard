import esbuild from "esbuild";
import process from "process";
import { cp, mkdir } from "node:fs/promises";
import { builtinModules } from "node:module";

const prod = process.argv[2] === "production";

const shared = {
  bundle: true,
  target: "es2021",
  logLevel: "info",
  sourcemap: prod ? false : "inline",
  treeShaking: true,
  minify: prod,
};

const plugin = await esbuild.context({
  ...shared,
  entryPoints: ["src/obsidian/main.ts"],
  external: [
    "obsidian",
    "electron",
    "@codemirror/autocomplete",
    "@codemirror/collab",
    "@codemirror/commands",
    "@codemirror/language",
    "@codemirror/lint",
    "@codemirror/search",
    "@codemirror/state",
    "@codemirror/view",
    "@lezer/common",
    "@lezer/highlight",
    "@lezer/lr",
    ...builtinModules,
  ],
  format: "cjs",
  outfile: "main.js",
});

const app = await esbuild.context({
  ...shared,
  entryPoints: ["src/app/timeline.ts", "src/app/picker.ts"],
  format: "esm",
  outdir: "dist",
});

await mkdir("dist", { recursive: true });
for (const file of ["timeline.html", "picker.html"]) {
  await cp(`src/app/${file}`, `dist/${file}`);
}

if (prod) {
  await Promise.all([plugin.rebuild(), app.rebuild()]);
  process.exit(0);
} else {
  await Promise.all([plugin.watch(), app.watch()]);
}
