// The public surface: every entry exports the same names (public.ts plus its own three calls), and
// transit-braid's errors pass `instanceof` across two copies of the package. The inline entry is
// built by Vite, so its runtime names are checked in its e2e test (inline.spec.ts); here its
// types are.
import { describe, expect, expectTypeOf, it } from 'vitest';

import * as errors from './errors.ts';
import * as main from './index.ts';
import type * as inline from './inline.ts';
import * as node from './node.ts';

/** Everything an entry exports at runtime, sorted. Types are checked by the compiler below. */
const NAMES = [
  'BraidError',
  'DEFAULT_LINE_WIDTH',
  'DEFAULT_SPACING',
  'InputError',
  'InternalError',
  'createBraidWorker',
  'createSession',
  'sideBySide',
];

/** A second copy of `errors.ts`: the same file as another module, as a second bundle would be. */
async function copyOfErrors(): Promise<typeof errors> {
  const url = './errors.ts?second-copy';
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the same file, another instance
  return (await import(/* @vite-ignore */ url)) as typeof errors;
}

describe('the public surface', () => {
  it('is the same list of names in transit-braid and transit-braid/node, and nothing internal', () => {
    expect(Object.keys(main).toSorted()).toEqual(NAMES);
    expect(Object.keys(node).toSorted()).toEqual(NAMES);
  });

  it('has the same names and types in every entry, the inline one too', () => {
    expectTypeOf<keyof typeof inline>().toEqualTypeOf<keyof typeof main>();
    expectTypeOf<keyof typeof node>().toEqualTypeOf<keyof typeof main>();
    expectTypeOf<inline.BraidWorker>().toEqualTypeOf<main.BraidWorker>();
    expectTypeOf<node.SideBySideOptions>().toEqualTypeOf<main.SideBySideOptions>();
    expectTypeOf<inline.SideBySideResult>().toEqualTypeOf<main.SideBySideResult>();
    // The worker's only calls are sideBySide() and terminate().
    expectTypeOf<keyof main.BraidWorker>().toEqualTypeOf<
      'createSession' | 'sideBySide' | 'terminate'
    >();
    expectTypeOf<inline.BraidSession>().toEqualTypeOf<main.BraidSession>();
    expectTypeOf<keyof main.BraidSession>().toEqualTypeOf<'close' | 'sideBySide'>();
  });
});

describe('errors across two copies of the package', () => {
  it('pass instanceof for their own class and its kinds, from either copy', async () => {
    const copy = await copyOfErrors();
    expect(copy.InputError).not.toBe(errors.InputError);

    const input = new copy.InputError('BAD_COORDINATE', 'bad point', { routeId: 'A' });
    const internal = new copy.InternalError('ENGINE_CRASHED', 'crashed');
    const stopped = new copy.BraidError('TERMINATED', 'Braid worker was terminated.');
    expect(input).toBeInstanceOf(errors.InputError);
    expect(input).toBeInstanceOf(errors.BraidError);
    expect(input).not.toBeInstanceOf(errors.InternalError);
    expect(internal).toBeInstanceOf(errors.InternalError);
    expect(internal).toBeInstanceOf(errors.BraidError);
    expect(internal).not.toBeInstanceOf(errors.InputError);
    expect(stopped).toBeInstanceOf(errors.BraidError);
    expect(stopped).not.toBeInstanceOf(errors.InputError);
    expect(stopped).not.toBeInstanceOf(errors.InternalError);
    // And the other way round, and within one copy.
    expect(new errors.InputError('BAD_OPTION', 'x')).toBeInstanceOf(copy.InputError);
    expect(new errors.InputError('BAD_OPTION', 'x')).toBeInstanceOf(errors.InputError);
    expect(new errors.InternalError('INTERNAL', 'x')).toBeInstanceOf(Error);
  });

  it('keep their name, message, code and details, and nothing else passes for them', async () => {
    const copy = await copyOfErrors();
    const error = new copy.InputError('BAD_OPTION', '`zoom` must be between 0 and 24.', {
      option: 'zoom',
      routeId: undefined,
    });
    expect(error.name).toBe('InputError');
    expect(error.message).toBe('`zoom` must be between 0 and 24.');
    expect(String(error)).toBe('InputError: `zoom` must be between 0 and 24.');
    expect(error).toMatchObject({ code: 'BAD_OPTION', option: 'zoom' });
    expect(error).not.toHaveProperty('routeId');
    // The code and details are plain data: they survive JSON, as in a log line.
    expect(JSON.parse(JSON.stringify(error))).toEqual({
      name: 'InputError',
      code: 'BAD_OPTION',
      option: 'zoom',
    });
    for (const other of [
      new Error('x'),
      new TypeError('x'),
      { name: 'InputError', code: 'BAD_OPTION', message: 'x' },
      null,
      undefined,
      'InputError',
      42,
    ]) {
      expect(other).not.toBeInstanceOf(errors.BraidError);
      expect(other).not.toBeInstanceOf(errors.InputError);
      expect(other).not.toBeInstanceOf(errors.InternalError);
    }
  });
});
