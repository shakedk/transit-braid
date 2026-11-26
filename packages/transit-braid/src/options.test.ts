import { describe, expect, it } from 'vitest';

import { InputError } from './errors.ts';
import {
  DEFAULT_LINE_WIDTH,
  DEFAULT_SPACING,
  resolveOptions,
  type SideBySideOptions,
} from './options.ts';

/** What `call` throws, or undefined when it returns. */
function thrown(call: () => unknown): unknown {
  try {
    call();
  } catch (error) {
    return error;
  }
  return undefined;
}

/** An InputError with code BAD_OPTION naming `option`, whose message says `message`. */
function badOption(message: string, option: string): Record<string, unknown> {
  return {
    name: 'InputError',
    code: 'BAD_OPTION',
    option,
    message: expect.stringContaining(message) as unknown,
  };
}

/** The option a message is about: the first one it names in backquotes, else the zoom. */
function optionIn(message: string): string {
  return /`(\w+)`/.exec(message)?.[1] ?? 'zoom';
}

describe('resolveOptions', () => {
  it('fills in defaults when only zoom is given', () => {
    expect(resolveOptions({ zoom: 13 })).toEqual({
      scale: { kind: 'zoom', zoom: 13 },
      lineWidth: DEFAULT_LINE_WIDTH,
      spacing: DEFAULT_SPACING,
    });
  });

  it('accepts metersPerPixel instead of zoom', () => {
    const resolved = resolveOptions({ metersPerPixel: 2.5, lineWidth: 6, spacing: 0 });
    expect(resolved.scale).toEqual({ kind: 'metersPerPixel', metersPerPixel: 2.5 });
    expect(resolved.lineWidth).toBe(6);
    expect(resolved.spacing).toBe(0);
  });

  it.each([
    [{}, 'exactly one of'],
    [{ zoom: 12, metersPerPixel: 3 }, 'exactly one of'],
    [{ zoom: 30 }, 'between 0 and 24'],
    [{ zoom: Number.NaN }, 'finite'],
    [{ metersPerPixel: 0 }, 'greater than 0'],
    [{ zoom: 12, lineWidth: -1 }, '`lineWidth` must be greater than 0'],
    [{ zoom: 12, spacing: -0.5 }, '`spacing` must be 0 or more'],
    [{ zoom: 12, spacing: Number.POSITIVE_INFINITY }, '`spacing` must be a finite number'],
    [{ zoom: -0.01 }, 'between 0 and 24'],
    [{ zoom: 24.01 }, 'between 0 and 24'],
    [{ zoom: Number.NaN }, '`zoom` must be a finite number'],
    [{ metersPerPixel: 0 }, '`metersPerPixel` must be greater than 0'],
    [{ metersPerPixel: Number.NaN }, '`metersPerPixel` must be a finite number'],
    [{ metersPerPixel: Number.POSITIVE_INFINITY }, '`metersPerPixel` must be a finite number'],
    [{ zoom: 12, lineWidth: Number.NaN }, '`lineWidth` must be a finite number'],
  ])('rejects %j', (options, message) => {
    const option =
      message === 'exactly one of'
        ? 'zoom' in options
          ? 'metersPerPixel'
          : 'zoom'
        : message === 'finite'
          ? 'zoom'
          : message === 'greater than 0'
            ? 'metersPerPixel'
            : optionIn(message);
    expect(thrown(() => resolveOptions(options))).toBeInstanceOf(InputError);
    expect(thrown(() => resolveOptions(options))).toMatchObject(badOption(message, option));
  });

  it('takes a lane cap as a whole number from 1, none when left out or null', () => {
    expect(resolveOptions({ zoom: 12 }).maxSideBySide).toBeUndefined();
    expect(resolveOptions({ zoom: 12, maxSideBySide: null }).maxSideBySide).toBeUndefined();
    expect(resolveOptions({ zoom: 12, maxSideBySide: undefined }).maxSideBySide).toBeUndefined();
    expect(resolveOptions({ zoom: 12, maxSideBySide: 1 }).maxSideBySide).toBe(1);
    expect(resolveOptions({ zoom: 12, maxSideBySide: 6 }).maxSideBySide).toBe(6);
    // Past what the WASM call takes (32 bits): the largest it takes, which drops nothing anyway.
    expect(resolveOptions({ zoom: 12, maxSideBySide: 2 ** 40 }).maxSideBySide).toBe(2 ** 32 - 1);
    expect(resolveOptions({ zoom: 12, maxSideBySide: 2 ** 32 - 1 }).maxSideBySide).toBe(
      2 ** 32 - 1,
    );
  });

  it.each([
    [{ zoom: 12, maxSideBySide: 0 }, '`maxSideBySide` must be a whole number, 1 or more'],
    [{ zoom: 12, maxSideBySide: -2 }, '`maxSideBySide` must be a whole number, 1 or more'],
    [{ zoom: 12, maxSideBySide: 2.5 }, '`maxSideBySide` must be a whole number, 1 or more'],
    [{ zoom: 12, maxSideBySide: 0.5 }, '`maxSideBySide` must be a whole number, 1 or more'],
    [{ zoom: 12, maxSideBySide: Number.NaN }, '`maxSideBySide` must be a finite number'],
    [
      { zoom: 12, maxSideBySide: Number.POSITIVE_INFINITY },
      '`maxSideBySide` must be a finite number',
    ],
    [{ zoom: 12, maxSideBySide: '6' }, '`maxSideBySide` must be a finite number'],
    [{ zoom: 12, maxSideBySide: [6] }, '`maxSideBySide` must be a finite number'],
    [{ zoom: 12, maxSideBySide: true }, '`maxSideBySide` must be a finite number'],
  ])('rejects the lane cap in %j', (options, message) => {
    // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- deliberately wrong input
    const call = (): unknown => resolveOptions(options as unknown as SideBySideOptions);
    expect(thrown(call)).toBeInstanceOf(InputError);
    expect(thrown(call)).toMatchObject(badOption(message, 'maxSideBySide'));
  });

  it('accepts zoom at both ends of the range', () => {
    expect(resolveOptions({ zoom: 0 }).scale).toEqual({ kind: 'zoom', zoom: 0 });
    expect(resolveOptions({ zoom: 24 }).scale).toEqual({ kind: 'zoom', zoom: 24 });
  });

  // JavaScript callers (and JSON) can pass anything, whatever the types say.
  it.each([
    [{ zoom: null }, '`zoom` must be a finite number'],
    [{ zoom: '12' }, '`zoom` must be a finite number'],
    [{ metersPerPixel: null }, '`metersPerPixel` must be a finite number'],
    [{ zoom: 12, spacing: '1' }, '`spacing` must be a finite number'],
    [{ zoom: 12, spacing: {} }, '`spacing` must be a finite number'],
  ])('rejects untyped input %j', (options, message) => {
    // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- deliberately wrong input
    const call = (): unknown => resolveOptions(options as unknown as SideBySideOptions);
    expect(thrown(call)).toBeInstanceOf(InputError);
    expect(thrown(call)).toMatchObject(badOption(message, optionIn(message)));
  });

  // Current behavior, recorded on purpose: `null` for an optional size means "use the default"
  // (`??`), while `zoom: null` counts as given and is rejected.
  it('treats null lineWidth and spacing as not given', () => {
    // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- deliberately wrong input
    const options = { zoom: 12, lineWidth: null, spacing: null } as unknown as SideBySideOptions;
    expect(resolveOptions(options)).toMatchObject({
      lineWidth: DEFAULT_LINE_WIDTH,
      spacing: DEFAULT_SPACING,
    });
  });

  it.each([null, undefined, 'zoom 14', 14])(
    'refuses options that are not an object (%j), naming `options`',
    (options) => {
      // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- deliberately wrong input
      const call = (): unknown => resolveOptions(options as unknown as SideBySideOptions);
      expect(thrown(call)).toBeInstanceOf(InputError);
      expect(thrown(call)).toMatchObject(badOption('`options` must be an object', 'options'));
    },
  );
});
