// Runs inside a worker: answers requests once the WebAssembly is ready. Shared by the normal
// worker (`worker.ts`, loads the .wasm from a URL), the inline one (`inline-worker.ts`, WASM bytes
// inside the script) and Node's (`node-worker.ts`, a worker_threads worker).

/// <reference lib="webworker" />

import {
  InternalError,
  internalError,
  type InternalErrorCode,
  Refused,
  type Refusal,
} from './errors.ts';
import type { WorkerRequest, WorkerResponse } from './protocol.ts';
import { closeSession, run } from './run.ts';

declare const self: DedicatedWorkerGlobalScope;

/** What `serve()` needs from the worker's side: a web worker's `self` or Node's `parentPort`. */
export interface ServerPort {
  /** Calls `listener` with each request. */
  onRequest(listener: (request: WorkerRequest) => void): void;
  /** Sends an answer, moving (not copying) the `transfer` buffers. */
  answer(response: WorkerResponse, transfer: ArrayBuffer[]): void;
}

/** This web worker's `self` as a `ServerPort`. */
export function webWorkerPort(): ServerPort {
  return {
    onRequest: (listener) => {
      self.addEventListener('message', (event: MessageEvent<WorkerRequest>) => {
        listener(event.data);
      });
    },
    answer: (response, transfer) => {
      self.postMessage(response, transfer);
    },
  };
}

/**
 * Answers every request on `port`; each waits for `ready` (the WASM loaded) first, so requests are
 * handled in the order they came: a session's close after its calls.
 */
export function serve(ready: Promise<unknown>, port: ServerPort): void {
  port.onRequest((request) => {
    void handle(ready, port, request);
  });
}

/** Handles one request; every kind waits for `ready` the same way, which keeps them in order. */
async function handle(
  ready: Promise<unknown>,
  port: ServerPort,
  request: WorkerRequest,
): Promise<void> {
  let response: WorkerResponse;
  try {
    await ready.catch((error: unknown) => {
      throw internalError('INTERNAL', `the engine could not load (${errorMessage(error)}).`);
    });
    if (request.type === 'closeSession') {
      closeSession(request.session); // no answer
      return;
    }
    response = { id: request.id, ok: true, ...run(request) };
  } catch (error) {
    // A close has no answer: a WASM that failed to load has no session to free.
    if (request.type === 'closeSession') return;
    response = { id: request.id, ok: false, ...failureOf(error) };
  }
  // Transfer the result buffers instead of copying them: zero-copy back to the main thread.
  const buffers = response.ok
    ? [response.coords, response.starts, response.bars, response.dropped]
    : [];
  const transfer = buffers.flatMap((array) =>
    array.buffer instanceof ArrayBuffer ? [array.buffer] : [],
  );
  port.answer(response, transfer);
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** A failed call's answer: the engine's refusal as data, or an internal error's code and text. */
function failureOf(
  error: unknown,
): { refusal: Refusal } | { internal: { code: InternalErrorCode; message: string } } {
  if (error instanceof Refused) return { refusal: error.refusal };
  const internal =
    error instanceof InternalError ? error : internalError('INTERNAL', errorMessage(error));
  return { internal: { code: internal.code, message: internal.message } };
}
