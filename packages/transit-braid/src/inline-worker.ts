// The inline entry's worker (`transit-braid/inline`): the WASM bytes are inside this script as
// base64, so nothing is fetched and no bundler has to find a .wasm file.

/// <reference lib="webworker" />

import { initSync } from '../wasm/transit_braid_wasm.js';
import wasmDataUrl from '../wasm/transit_braid_wasm_bg.wasm?inline';
import { serve, webWorkerPort } from './serve.ts';

/** The bytes of a `data:...;base64,` URL. */
function bytesOf(dataUrl: string): Uint8Array<ArrayBuffer> {
  const text = atob(dataUrl.slice(dataUrl.indexOf(',') + 1));
  const bytes = new Uint8Array(text.length);
  for (let i = 0; i < text.length; i++) bytes[i] = text.charCodeAt(i);
  return bytes;
}

// Compiled on the worker's own thread (allowed there at any size); a failure answers every
// request with the error instead of hanging.
serve(
  Promise.resolve().then(() => initSync({ module: bytesOf(wasmDataUrl) })),
  webWorkerPort(),
);
