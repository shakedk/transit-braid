// The one cross-browser smoke test: Firefox and WebKit get exactly this, nothing more. In each
// browser: load the worker and its WASM, make one sideBySide() call, get GeoJSON. Chromium runs
// the full suite (*.browser.test.ts) instead.
import { expect, it } from 'vitest';

import { sideBySide } from './side-by-side.ts';

it('loads the worker and draws two routes side by side', async () => {
  const street = [
    [34.78, 32.07],
    [34.785, 32.07],
  ];
  const out = await sideBySide(
    {
      type: 'FeatureCollection',
      features: ['A', 'B'].map((id) => ({
        type: 'Feature' as const,
        id,
        properties: null,
        geometry: { type: 'LineString' as const, coordinates: street },
      })),
    },
    { zoom: 16 },
  );
  expect(out.lines.features.map((f) => f.id)).toEqual(['A', 'B']);
  const [a, b] = out.lines.features.map((f) => f.geometry.coordinates[0]?.[1] ?? Number.NaN);
  // One street, two lanes: the lines moved apart, one to each side of it.
  expect((a ?? 0) - 32.07).toBeCloseTo(-((b ?? 0) - 32.07), 9);
  expect(Math.abs((a ?? 0) - (b ?? 0))).toBeGreaterThan(0);
});
