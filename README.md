# Punchcard

Track time on Obsidian notes as blocks on a timeline, from inside Obsidian or from a desktop window.

## Toolchain

The block rules live once, in Rust (`src-tauri/core`). `stint` links them natively, and the Obsidian plugin and the desktop pages load them as WebAssembly, so building the JavaScript side needs `wasm-pack`:

```sh
cargo install wasm-pack --version 0.15.1 --locked
```

`wasm-pack` adds the `wasm32-unknown-unknown` target through rustup, fetches `wasm-opt`, and fetches the `wasm-bindgen` CLI version that `src-tauri/Cargo.lock` names. `npm run build` and `npm test` rebuild the wasm first.
