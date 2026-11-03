import { execFileSync } from "node:child_process";
import { readFile, writeFile } from "node:fs/promises";
import { resolve } from "node:path";

const OUT_DIR = "src/wasm/pkg";
const GLUE = `${OUT_DIR}/punchcard_wasm.js`;
// The async init's default fetch path, which makes Vite emit the wasm a second time and esbuild CJS warn about import.meta.
const DEFAULT_FETCH = "module_or_path = new URL('punchcard_wasm_bg.wasm', import.meta.url);";
const INSTALL_WASM_PACK = "cargo install wasm-pack --version 0.15.1 --locked";

try {
  execFileSync(
    "wasm-pack",
    [
      "build",
      "src-tauri/wasm",
      "--target",
      "web",
      "--profile",
      "wasm",
      // wasm-pack resolves --out-dir against the crate, so the path goes in absolute.
      "--out-dir",
      resolve(OUT_DIR),
      "--no-pack",
      "--no-gitignore",
    ],
    { stdio: "inherit" }
  );
} catch (error) {
  // A failed build has already printed its own error, so only a missing wasm-pack needs a message.
  if (error.code === "ENOENT") {
    console.error(`wasm-pack is not on PATH; run \`${INSTALL_WASM_PACK}\`. See Toolchain in README.md.`);
    process.exit(1);
  }
  process.exit(error.status ?? 1);
}

const glue = await readFile(GLUE, "utf8");
if (!glue.includes(DEFAULT_FETCH)) {
  throw new Error(`${GLUE} no longer holds the default fetch path; check the wasm-bindgen version`);
}
await writeFile(GLUE, glue.replace(DEFAULT_FETCH, 'throw new Error("Load the Punchcard core with initSync");'));

// One plain module carries the bytes, so esbuild, Vite and vitest all load it with no wasm loader configured.
const bytes = await readFile(`${OUT_DIR}/punchcard_wasm_bg.wasm`);
await writeFile(`${OUT_DIR}/bytes.ts`, `export default "${bytes.toString("base64")}";\n`);
