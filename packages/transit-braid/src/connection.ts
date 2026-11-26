// Main-thread side of a Braid worker: sends requests and turns answers into Promises. Shared by
// the normal entry (`client.ts`, worker from a URL), the inline one (`inline.ts`, from a Blob) and
// the Node one (`node.ts`, a worker_threads worker).

import { InternalError, internalError, Refused, terminated } from './errors.ts';
import type { SideBySideOptions } from './options.ts';
import type { PipelineAnswer, SideBySideCall, WorkerRequest, WorkerResponse } from './protocol.ts';
import type { RoutesInput, SideBySideResult } from './routes.ts';
import { type BraidSession, openSession } from './session.ts';
import { type SessionHost, sideBySideOn } from './side-by-side-on.ts';

/** A running Braid worker. Call `terminate()` when you no longer need it. */
export interface BraidWorker {
  /**
   * `sideBySide()` on this worker: the same input, output and errors. For an app that wants a
   * worker of its own, for example a Node server whose thread must not block.
   */
  sideBySide(routes: RoutesInput, options: SideBySideOptions): Promise<SideBySideResult>;
  /** `createSession()` on this worker: a session whose memory lives in this worker. */
  createSession(): BraidSession;
  /** Stops the worker. Pending calls reject, and so does every later call, its sessions' too. */
  terminate(): void;
}

/**
 * What `connect()` needs from a started worker: a browser one (`webPort`, `web-port.ts`) or Node's
 * (`nodePort`, `node-port.ts`).
 */
export interface WorkerPort {
  /** Sends a request to the worker. */
  send(request: WorkerRequest): void;
  /** Calls `listener` with each answer. */
  onAnswer(listener: (response: WorkerResponse) => void): void;
  /**
   * Calls `listener` if the worker itself fails: its script didn't load, it stopped, it threw
   * outside a call, or an answer couldn't be read.
   */
  onFailure(listener: (message: string) => void): void;
  /** Told when calls start or stop waiting (Node lets the process exit while the worker idles). */
  busy?(waiting: boolean): void;
  /** Stops the worker. */
  terminate(): void;
}

type Pending = { resolve: (value: PipelineAnswer) => void; reject: (error: Error) => void };

/** The worker's `sideBySide` request, without its id. */
export type { SideBySideRequest } from './side-by-side-on.ts';

/** The worker as `sideBySide()` uses it: the public calls plus the pipeline and sessions
 * (internal). */
export interface PipelineWorker extends BraidWorker, SessionHost {
  /** False once the worker can't take calls: it failed, or was terminated. */
  readonly alive: boolean;
}

/**
 * Talks to a started Braid worker (`worker.ts`, `inline-worker.ts` or `node-worker.ts`).
 *
 * If the worker fails (its script, the thread, or an answer), it is stopped: waiting calls reject
 * with an internal error, and later calls reject at once, as after `terminate()`. A crash
 * inside the WASM is not a failure of the worker: it restarts its WASM and keeps answering.
 */
export function connect(port: WorkerPort): PipelineWorker {
  const pending = new Map<number, Pending>();
  let nextId = 1;
  /** Once the worker takes no more calls: the error each later call gets. */
  let stopped: (() => Error) | undefined;

  /** Forgets call `id`; tells the port when nothing is waiting any more. */
  function settle(id: number): void {
    pending.delete(id);
    if (pending.size === 0) port.busy?.(false);
  }

  port.onAnswer((response) => {
    const call = pending.get(response.id);
    // Defensive: the worker only answers ids we sent.
    if (!call) return;
    settle(response.id);
    if (response.ok) {
      const { coords, starts, bars, dropped } = response;
      call.resolve({ coords, starts, bars, dropped });
    } else if ('refusal' in response) {
      call.reject(new Refused(response.refusal));
    } else {
      call.reject(new InternalError(response.internal.code, response.internal.message));
    }
  });

  port.onFailure((message) => {
    // Node also reports the exit that our own terminate() causes; a worker stops only once.
    if (stopped !== undefined) return;
    stopped = () =>
      internalError(
        'WORKER_FAILED',
        `the worker failed earlier (${message}) and was stopped. Start a new one with createBraidWorker().`,
      );
    port.terminate();
    rejectAll(internalError('WORKER_FAILED', `the worker failed (${message}) and was stopped.`));
  });

  function rejectAll(error: Error): void {
    const calls = [...pending.values()];
    pending.clear();
    port.busy?.(false);
    for (const call of calls) call.reject(error);
  }

  function send(request: Omit<SideBySideCall, 'id'>): Promise<PipelineAnswer> {
    if (stopped !== undefined) return Promise.reject(stopped());
    const id = nextId++;
    return new Promise<PipelineAnswer>((resolve, reject) => {
      if (pending.size === 0) port.busy?.(true);
      pending.set(id, { resolve, reject });
      try {
        port.send({ ...request, id });
      } catch (error) {
        // Not sent (no memory left to copy the request for the worker): nothing will answer
        // it, so it must not keep a Node process alive.
        settle(id);
        throw error;
      }
    });
  }

  const worker: PipelineWorker = {
    get alive() {
      return stopped === undefined;
    },
    sideBySide: async (routes, options) => sideBySideOn(() => worker, routes, options),
    createSession: () => openSession(() => worker),
    runPipeline: async (request) => send({ type: 'sideBySide', ...request }),
    closeSession(session) {
      // A stopped worker took its sessions with it; nothing to free, nothing to answer.
      if (stopped !== undefined) return;
      try {
        port.send({ id: nextId++, type: 'closeSession', session });
      } catch {
        // Not sent (no memory left to copy it): the session's memory goes with the worker.
      }
    },
    terminate() {
      stopped ??= terminated;
      port.terminate();
      rejectAll(terminated());
    },
  };
  return worker;
}

/**
 * The worker `sideBySide()` shares between calls: started by `start` on the first call, and again
 * on the next call after it failed, so one failed worker never blocks the calls after it.
 */
export function sharedWorker(start: () => PipelineWorker): () => PipelineWorker {
  let worker: PipelineWorker | undefined;
  return () => {
    if (!worker?.alive) worker = start();
    return worker;
  };
}
