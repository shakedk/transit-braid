import { describe, expect, it } from 'vitest';

import { colorOf, DEFAULT_COLOR } from './color.ts';

describe('colorOf', () => {
  it('reads #rrggbb and #rgb from properties.color', () => {
    expect(colorOf({ properties: { color: '#e4002b' } })).toEqual([228, 0, 43]);
    expect(colorOf({ properties: { color: '#0072CE' } })).toEqual([0, 114, 206]);
    expect(colorOf({ properties: { color: ' #f80 ' } })).toEqual([255, 136, 0]);
  });

  it('gives gray for a missing, null or malformed color', () => {
    for (const properties of [
      undefined,
      null,
      {},
      { color: null },
      { color: 42 },
      { color: 'red' },
      { color: '#12345' },
      { color: '#ggg' },
      { color: '' },
      'not an object',
    ]) {
      expect(colorOf({ properties })).toEqual(DEFAULT_COLOR);
    }
    expect(colorOf({})).toEqual(DEFAULT_COLOR);
  });
});
