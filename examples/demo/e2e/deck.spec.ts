// transit-braid-deck's BraidLayer in a real Chromium: it draws each route in its own slot
// with the route's color, keeps the last good result when a call fails, and recomputes after a
// zoom. A screenshot is kept as a test artifact.
import { expect, type Page, test } from '@playwright/test';

/** The page's pixel color at (x, y) of the deck.gl canvas, read from a screenshot. */
async function colorsAcross(page: Page, x: number, top: number, bottom: number): Promise<string[]> {
  const shot = await page.locator('canvas').first().screenshot();
  return page.evaluate(
    async ({ png, column, from, to }) => {
      const img = new Image();
      img.src = `data:image/png;base64,${png}`;
      await img.decode();
      const c = document.createElement('canvas');
      c.width = img.width;
      c.height = img.height;
      const g = c.getContext('2d');
      if (!g) return [];
      g.drawImage(img, 0, 0);
      const out: string[] = [];
      for (let y = from; y <= to; y++) {
        const [r = 0, gr = 0, b = 0] = g.getImageData(column, y, 1, 1).data;
        // Name the route colors; anything else is background or blending.
        const name =
          r > 200 && gr < 60 && b < 80
            ? 'red'
            : r < 60 && gr > 80 && gr < 150 && b > 170
              ? 'blue'
              : r < 60 && gr > 130 && b < 120
                ? 'green'
                : '';
        if (name && out.at(-1) !== name) out.push(name);
      }
      return out;
    },
    { png: shot.toString('base64'), column: x, from: top, to: bottom },
  );
}

async function open(page: Page): Promise<string[]> {
  const errors: string[] = [];
  page.on('pageerror', (e) => errors.push(e.message));
  await page.goto('/deck.html');
  await expect.poll(() => page.evaluate(() => window.deckDebug?.lines() ?? 0)).toBe(3);
  return errors;
}

test('BraidLayer draws the three routes side by side, each in its own color', async ({
  page,
}, info) => {
  const errors = await open(page);
  const box = await page.locator('canvas').first().boundingBox();
  if (!box) throw new Error('no canvas');
  // West of the green route's turn, all three share the street: a vertical strip through the
  // middle of the window crosses three colored lines, one slot each.
  const x = Math.round(box.width * 0.3);
  const mid = Math.round(box.height / 2);
  await expect.poll(() => colorsAcross(page, x, mid - 30, mid + 30)).toHaveLength(3);
  const across = await colorsAcross(page, x, mid - 30, mid + 30);
  expect(across.toSorted()).toEqual(['blue', 'green', 'red']);
  // East of the turn, only red and blue are left on the street.
  const east = await colorsAcross(page, Math.round(box.width * 0.8), mid - 30, mid + 30);
  expect(east.toSorted()).toEqual(['blue', 'red']);
  await info.attach('braid-layer', { body: await page.screenshot(), contentType: 'image/png' });
  expect(errors).toEqual([]);
});

test('a refused call keeps the last good lines and reports the error', async ({ page }) => {
  const errors = await open(page);
  await page.evaluate(() => {
    const d = window.deckDebug;
    if (!d) return;
    d.setRoutes({
      type: 'FeatureCollection',
      features: [
        {
          type: 'Feature',
          id: 'broken',
          properties: null,
          geometry: {
            type: 'LineString',
            coordinates: [
              [34.78, 95],
              [34.79, 95],
            ],
          },
        },
      ],
    });
  });
  await expect.poll(() => page.evaluate(() => window.deckDebug?.errors.length ?? 0)).toBe(1);
  const message = await page.evaluate(() => window.deckDebug?.errors[0] ?? '');
  expect(message).toContain('BraidLayer');
  expect(message).toContain('broken');
  expect(await page.evaluate(() => window.deckDebug?.lines())).toBe(3); // the last good result
  expect(errors).toEqual([]);
});

test('a lane cap leaves routes out whole and onDropped names them, once per change', async ({
  page,
}) => {
  const errors = await open(page);
  const dropped = (): Promise<unknown> => page.evaluate(() => window.deckDebug?.dropped());
  const drawn = (): Promise<unknown> => page.evaluate(() => window.deckDebug?.drawnIds());
  // The first result reports its (empty) list.
  await expect.poll(dropped).toEqual([[]]);
  // Cap 2: the west half carries red, blue and green, so green (third) is left out.
  await page.evaluate(() => window.deckDebug?.setCap(2));
  await expect.poll(drawn).toEqual(['red', 'blue']);
  await expect.poll(dropped).toEqual([[], ['green']]);
  // A zoom with the same cap leaves out the same route: no new report.
  await page.evaluate(() => window.deckDebug?.zoomTo(17.5));
  await expect.poll(() => page.evaluate(() => window.deckDebug?.zoomShown())).toBe(17.5);
  expect(await drawn()).toEqual(['red', 'blue']);
  expect(await dropped()).toEqual([[], ['green']]);
  // No cap: every route again, reported once.
  await page.evaluate(() => window.deckDebug?.setCap(null));
  await expect.poll(drawn).toEqual(['red', 'blue', 'green']);
  await expect.poll(dropped).toEqual([[], ['green'], []]);
  // A bad cap is refused like any bad option: the last good lines stay, the error is reported.
  await page.evaluate(() => window.deckDebug?.setCap(0));
  await expect.poll(() => page.evaluate(() => window.deckDebug?.errors.length ?? 0)).toBe(1);
  expect(await page.evaluate(() => window.deckDebug?.errors[0] ?? '')).toContain('maxSideBySide');
  expect(await drawn()).toEqual(['red', 'blue', 'green']);
  expect(errors).toEqual([]);
});

test('after a zoom settles, the lines are computed again for the new zoom', async ({ page }) => {
  const errors = await open(page);
  expect(await page.evaluate(() => window.deckDebug?.zoomShown())).toBe(16);
  await page.evaluate(() => window.deckDebug?.zoomTo(17.5));
  await expect.poll(() => page.evaluate(() => window.deckDebug?.zoomShown())).toBe(17.5);
  expect(await page.evaluate(() => window.deckDebug?.lines())).toBe(3);
  expect(errors).toEqual([]);
});

/** Where route `id`'s line is on the page, `t` of the way along its first segment. */
async function lane(page: Page, id: string, t: number): Promise<{ x: number; y: number }> {
  const box = await page.locator('canvas').first().boundingBox();
  const at = await page.evaluate(([r, f]) => window.deckDebug?.screenOf(r, f) ?? null, [
    id,
    t,
  ] as const);
  if (!box || !at) throw new Error(`no line for ${id}`);
  return { x: box.x + at.x, y: box.y + at.y };
}

/** The deck.gl canvas's `[r, g, b]` at each page point, read from one screenshot. */
async function pixels(page: Page, points: { x: number; y: number }[]): Promise<number[][]> {
  const canvas = page.locator('canvas').first();
  const box = await canvas.boundingBox();
  if (!box) throw new Error('no canvas');
  const png = (await canvas.screenshot()).toString('base64');
  const at = points.map((p) => ({ x: Math.round(p.x - box.x), y: Math.round(p.y - box.y) }));
  return page.evaluate(
    async ({ image, spots }) => {
      const img = new Image();
      img.src = `data:image/png;base64,${image}`;
      await img.decode();
      const c = document.createElement('canvas');
      c.width = img.width;
      c.height = img.height;
      const g = c.getContext('2d');
      if (!g) return [];
      g.drawImage(img, 0, 0);
      return spots.map(({ x, y }) => Array.from(g.getImageData(x, y, 1, 1).data.slice(0, 3)));
    },
    { image: png, spots: at },
  );
}

/** The demo's highlight color, `[255, 200, 0]`, give or take blending. */
function isYellow([r = 0, g = 0, b = 0]: number[]): boolean {
  return r > 200 && g > 150 && b < 100;
}

/** Hovers route `id`'s lane (its tooltip names it), then clicks it (the click picks it). */
async function hoverAndClick(page: Page, id: string): Promise<void> {
  const at = await lane(page, id, 0.3);
  await page.mouse.move(at.x, at.y);
  await expect(page.locator('.deck-tooltip')).toHaveText(`Route ${id}`);
  await page.mouse.click(at.x, at.y);
  await expect.poll(() => page.evaluate(() => window.deckDebug?.picked())).toBe(id);
}

test('hovering a line names its route and highlights it; clicking picks it', async ({
  page,
}, info) => {
  const errors = await open(page);
  await hoverAndClick(page, 'red');
  await hoverAndClick(page, 'blue');
  await hoverAndClick(page, 'green');
  // The hovered lane is drawn in the highlight color (yellow) along its length; its neighbors are
  // not. Sampled away from the cursor, where deck.gl's tooltip box covers the canvas.
  const blue = await lane(page, 'blue', 0.3);
  await page.mouse.move(blue.x, blue.y);
  await expect(page.locator('.deck-tooltip')).toHaveText('Route blue');
  const far = await Promise.all(['blue', 'red', 'green'].map((id) => lane(page, id, 0.7)));
  await expect
    .poll(async () => (await pixels(page, far)).map(isYellow))
    .toEqual([true, false, false]);
  const box = await page.locator('canvas').first().boundingBox();
  if (!box) throw new Error('no canvas');
  await info.attach('hover-blue', { body: await page.screenshot(), contentType: 'image/png' });
  // Empty map: no tooltip and no highlight, and a click there picks nothing.
  await page.mouse.move(box.x + 20, box.y + box.height - 20);
  await expect(page.locator('.deck-tooltip')).toBeHidden();
  await expect
    .poll(async () => (await pixels(page, far)).map(isYellow))
    .toEqual([false, false, false]);
  await page.mouse.click(box.x + 20, box.y + box.height - 20);
  await expect.poll(() => page.evaluate(() => window.deckDebug?.picked())).toBeNull();
  expect(errors).toEqual([]);
});
