// Loads the WebAssembly from disk in Node (the Node entry and its worker): the .wasm next to this
// package's source, compiled on the thread that calls this.

import { readFile } from 'node:fs/promises';

import { initSync } from '../wasm/transit_braid_wasm.js';

/** Reads and compiles the WASM; does nothing more once it is loaded. */
export async function loadWasm(): Promise<void> {
  const bytes = await readFile(new URL('../wasm/transit_braid_wasm_bg.wasm', import.meta.url));
  initSync({ module: bytes });
}
