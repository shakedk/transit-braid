// The demo: routes drawn side by side by sideBySide() and edited on the map. We don't compare
// against saved screenshots (rendering differs per OS/GPU); instead we ask deck.gl which line is
// drawn at a point we know lies on each line (its "probe").
import { expect, type Page, test } from '@playwright/test';

/** Opens the routes to edit on a plain background (no map tiles from the network: stable tests). */
async function open(page: Page): Promise<string[]> {
  const errors: string[] = [];
  page.on('pageerror', (error) => errors.push(error.message));
  await page.goto('/?basemap=none&network=sample');
  await page.waitForFunction(() => window.braidDebug?.ready === true);
  await expect(page.locator('#status')).toContainText('side by side');
  return errors;
}

/** Which line deck.gl draws at each line's probe point. Poll it: drawing takes a frame. */
async function hits(page: Page): Promise<Record<string, string | undefined>> {
  return page.evaluate(() => {
    const braid = window.braidDebug;
    if (!braid) return {};
    const result: Record<string, string | undefined> = {};
    for (const [lineId, lngLat] of Object.entries(braid.probes)) {
      const { x, y } = braid.map.project(lngLat);
      const picked = braid.overlay.pickObject({ x, y, radius: 2, layerIds: ['braid-lines'] }) as {
        object?: { lineId?: string };
      } | null;
      result[lineId] = picked?.object?.lineId;
    }
    return result;
  });
}

/** Screen distance in px between two lines' probe points. */
async function gap(page: Page, a: string, b: string): Promise<number> {
  return page.evaluate(
    ([idA, idB]: [string, string]) => {
      const braid = window.braidDebug;
      const pa = braid?.probes[idA];
      const pb = braid?.probes[idB];
      if (!braid || !pa || !pb) return Number.NaN;
      const p = braid.map.project(pa);
      const q = braid.map.project(pb);
      return Math.hypot(p.x - q.x, p.y - q.y);
    },
    [a, b] satisfies [string, string],
  );
}

const state = (
  page: Page,
): Promise<{ routes: string[]; runs: number; editing: string | null; drafting: number | null }> =>
  page.evaluate(() => {
    const d = window.braidDebug!;
    return { routes: d.routes, runs: d.runs, editing: d.editing, drafting: d.drafting };
  });

test('draws the sample routes side by side, one lane (6 + 2 px) apart', async ({ page }, info) => {
  const errors = await open(page);
  expect((await state(page)).routes).toEqual(['red', 'blue', 'yellow']);
  await expect.poll(() => hits(page)).toEqual({ red: 'red', blue: 'blue', yellow: 'yellow' });
  // The three share their first stretch, so their probes sit side by side across it.
  const lane = await gap(page, 'red', 'blue');
  expect(lane).toBeGreaterThan(7.5);
  expect(lane).toBeLessThan(8.5);
  await info.attach('side by side', { body: await page.screenshot(), contentType: 'image/png' });
  expect(errors).toEqual([]);
});

test('"Side by side" off shows the routes as given, on top of each other', async ({ page }) => {
  await open(page);
  await page.locator('#side-by-side').uncheck();
  await expect(page.locator('#status')).toContainText('as given');
  // Only the top one (drawn last) shows where they share a street.
  await expect.poll(() => hits(page)).toEqual({ red: 'yellow', blue: 'yellow', yellow: 'yellow' });
  expect(await gap(page, 'red', 'blue')).toBe(0);
  await page.locator('#side-by-side').check();
  await expect.poll(() => hits(page)).toEqual({ red: 'red', blue: 'blue', yellow: 'yellow' });
});

test('removing a route draws the others again without it', async ({ page }) => {
  await open(page);
  const before = (await state(page)).runs;
  await page.getByRole('button', { name: 'Remove blue' }).click();
  await expect.poll(async () => (await state(page)).routes).toEqual(['red', 'yellow']);
  await expect.poll(() => hits(page)).toEqual({ red: 'red', yellow: 'yellow' });
  expect((await state(page)).runs).toBeGreaterThan(before);
  await expect(page.locator('#routes li')).toHaveCount(2);
});

test('adding a route: click points on the map, then Finish', async ({ page }) => {
  const errors = await open(page);
  const mapBox = (await page.locator('#map').boundingBox())!;
  const at = (fx: number, fy: number) => ({
    x: mapBox.x + mapBox.width * fx,
    y: mapBox.y + mapBox.height * fy,
  });
  await page.locator('#add').click();
  await expect(page.locator('#finish')).toBeVisible();
  // One click is not a route yet.
  const p1 = at(0.7, 0.3);
  await page.mouse.click(p1.x, p1.y);
  await page.locator('#finish').click();
  await expect(page.locator('#status')).toHaveText('A route needs at least two different points.');
  expect((await state(page)).drafting).toBe(1);
  const p2 = at(0.75, 0.5);
  await page.mouse.click(p2.x, p2.y);
  const p3 = at(0.85, 0.6);
  await page.mouse.click(p3.x, p3.y);
  expect((await state(page)).drafting).toBe(3);
  await page.locator('#finish').click();
  await expect
    .poll(async () => (await state(page)).routes)
    .toEqual(['red', 'blue', 'yellow', 'route-1']);
  expect((await state(page)).drafting).toBeNull();
  await expect.poll(async () => (await hits(page))['route-1']).toBe('route-1');
  // Cancel leaves the routes alone.
  await page.locator('#add').click();
  await page.mouse.click(p1.x, p1.y);
  await page.locator('#cancel').click();
  expect((await state(page)).routes).toHaveLength(4);
  expect(errors).toEqual([]);
});

test('moving a route: drag one of its points and the drawn line moves', async ({ page }) => {
  const errors = await open(page);
  await page.locator('#routes .pick', { hasText: 'red' }).click();
  expect((await state(page)).editing).toBe('red');
  await expect(page.locator('.maplibregl-marker')).toHaveCount(6);
  const before = await page.evaluate(() => window.braidDebug!.probes['red']!);
  const runs = (await state(page)).runs;

  // Point 1 ends red's first segment (the shared stretch of Ibn Gabirol Street).
  const marker = page.locator('.maplibregl-marker[data-point="1"]');
  const box = (await marker.boundingBox())!;
  const from = { x: box.x + box.width / 2, y: box.y + box.height * 0.8 };
  await page.mouse.move(from.x, from.y);
  await page.mouse.down();
  await page.mouse.move(from.x + 60, from.y + 40, { steps: 8 });
  await page.mouse.up();

  // The drag calls sideBySide() again, and red's drawn line is somewhere else now.
  await expect.poll(async () => (await state(page)).runs).toBeGreaterThan(runs);
  await expect
    .poll(() => page.evaluate(() => window.braidDebug!.probes['red']!))
    .not.toEqual(before);
  // Red's first drawn segment may now end at a new node near its start, at the top edge of the
  // test window: center the map there (no zoom, so nothing is drawn again), then ask what is drawn.
  await page.evaluate(() => {
    const d = window.braidDebug!;
    d.map.jumpTo({ center: d.probes['red']! });
  });
  await expect.poll(async () => (await hits(page))['red']).toBe('red');
  // Picking it again stops editing.
  await page.locator('#routes .pick', { hasText: 'red' }).click();
  await expect(page.locator('.maplibregl-marker')).toHaveCount(0);
  expect(errors).toEqual([]);
});

test('rapid edits: only the newest change is computed and drawn', async ({ page }) => {
  const errors = await open(page);
  const counts = () =>
    page.evaluate(() => {
      const d = window.braidDebug!;
      return { runs: d.runs, calls: d.calls, shown: d.shown };
    });
  await expect.poll(async () => (await counts()).shown).toBe((await counts()).runs);
  const before = await counts();
  // Six moves of one point in a row, all before the first sideBySide() call can answer.
  await page.evaluate(() => {
    const d = window.braidDebug!;
    const route = d.routes[0]!;
    const { lng, lat } = d.map.getCenter();
    for (let k = 1; k <= 6; k += 1) d.move(route, 0, [lng + k * 0.0003, lat]);
  });
  await expect.poll(async () => (await counts()).shown).toBe(before.runs + 6);
  const after = await counts();
  expect(after.runs).toBe(before.runs + 6);
  // The first move's call, then one call for the newest move; the four in between never run.
  expect(after.calls - before.calls).toBeLessThanOrEqual(2);
  await expect(page.locator('#status')).toContainText('side by side');
  expect(errors).toEqual([]);
});

test('zooming draws again, still one lane apart', async ({ page }) => {
  await open(page);
  await page.evaluate(() => {
    window.braidDebug?.map.zoomTo(16, { duration: 0 });
  });
  await expect(page.locator('#status')).toContainText('zoom 16.0');
  const lane = await gap(page, 'red', 'blue');
  expect(lane).toBeGreaterThan(7.5);
  expect(lane).toBeLessThan(8.5);
});

test('Tel Aviv: 25 real bus routes side by side, with station bars and the data credit', async ({
  page,
}, info) => {
  const errors = await open(page);
  await page.locator('#network').selectOption('tel-aviv');
  await expect.poll(() => page.evaluate(() => window.braidDebug?.network)).toBe('tel-aviv');
  await expect.poll(async () => (await state(page)).routes.length).toBe(25);
  await expect(page.locator('#status')).toContainText('25 routes side by side');
  await expect(page.locator('#routes li')).toHaveCount(25);
  await expect(page.locator('#route-count')).toHaveText('25');
  await expect(page.locator('#network-credit')).toBeVisible();
  await expect(page.locator('#network-credit')).toContainText('Ministry of Transport');
  const drawn = await page.evaluate(() => ({
    bars: window.braidDebug!.bars,
    marks: window.braidDebug!.marks,
  }));
  expect(drawn.bars).toBeGreaterThan(100);
  expect(drawn.marks).toBeGreaterThan(25);
  await info.attach('tel aviv', { body: await page.screenshot(), contentType: 'image/png' });
  // Back to the routes to edit: no credit, no bars.
  await page.locator('#network').selectOption('sample');
  await expect.poll(async () => (await state(page)).routes).toEqual(['red', 'blue', 'yellow']);
  await expect(page.locator('#network-credit')).toBeHidden();
  await expect.poll(() => page.evaluate(() => window.braidDebug!.bars)).toBe(0);
  expect(errors).toEqual([]);
});

test('the lane cap leaves whole routes out where too many share a street', async ({ page }) => {
  await open(page);
  // All three share their first street: one lane there keeps red only.
  await page.locator('#cap').selectOption('1');
  await expect
    .poll(() => page.evaluate(() => window.braidDebug!.dropped))
    .toEqual(['blue', 'yellow']);
  await expect(page.locator('#cap-about')).toContainText('2 routes left out');
  await expect.poll(() => hits(page)).toEqual({ red: 'red' });
  await page.locator('#cap').selectOption('');
  await expect(page.locator('#cap-about')).toHaveText('No cap: every route is drawn.');
  await expect.poll(() => hits(page)).toEqual({ red: 'red', blue: 'blue', yellow: 'yellow' });
});

test('the zoom slider zooms the map, and the drawing follows', async ({ page }) => {
  await open(page);
  await page.locator('#zoom').fill('16');
  await expect(page.locator('#zoom-value')).toHaveText('16.0');
  await expect(page.locator('#status')).toContainText('zoom 16.0');
  await expect(page.locator('#map-chip')).toContainText('Side by side · zoom 16.0');
  const lane = await gap(page, 'red', 'blue');
  expect(lane).toBeGreaterThan(7.5);
  expect(lane).toBeLessThan(8.5);
});

test('?network= opens a scenario on a plain background, and the link follows the picker', async ({
  page,
}) => {
  const errors: string[] = [];
  page.on('pageerror', (error) => errors.push(error.message));
  await page.goto('/?basemap=none&network=stops');
  await page.waitForFunction(
    () => window.braidDebug?.network === 'stops' && /side by side/.test(window.braidDebug.status),
  );
  await expect(page.locator('#network')).toHaveValue('stops');
  await expect(page.locator('#network-look')).toContainText('What to look for');
  // A bar for each stop near a route: 7 of the 8 (the far one gets none). The same-name pair's
  // two bars sit at the same place, so they read as one.
  expect(await page.evaluate(() => window.braidDebug!.bars)).toBe(7);
  await page.locator('#network').selectOption('stress-grid');
  await expect.poll(async () => (await state(page)).routes.length).toBe(70);
  await expect(page.locator('#status')).toContainText('70 routes side by side');
  expect(new URL(page.url()).searchParams.get('network')).toBe('stress-grid');
  expect(errors).toEqual([]);
});

test('opens Tel Aviv by default, and for an unknown ?network=', async ({ page }) => {
  const opensTelAviv = async (url: string): Promise<void> => {
    await page.goto(url);
    await page.waitForFunction(
      () =>
        window.braidDebug?.network === 'tel-aviv' &&
        /25 routes side by side/.test(window.braidDebug.status),
    );
    await expect(page.locator('#network')).toHaveValue('tel-aviv');
  };
  await opensTelAviv('/?basemap=none');
  await opensTelAviv('/?basemap=none&network=nowhere');
});

test('Edit routes: a click on a line picks it, then its points can be dragged', async ({
  page,
}) => {
  const errors = await open(page);
  await expect(page.locator('#edit-help')).toBeHidden();
  // Without edit mode, a click on a line picks nothing.
  const red = await page.evaluate(() => {
    const d = window.braidDebug!;
    return d.map.project(d.probes['red']!);
  });
  const box = (await page.locator('#map').boundingBox())!;
  await page.mouse.click(box.x + red.x, box.y + red.y);
  expect((await state(page)).editing).toBeNull();
  await page.locator('#edit-mode').click();
  await expect(page.locator('#edit-mode')).toHaveAttribute('aria-pressed', 'true');
  await expect(page.locator('#edit-help')).toBeVisible();
  await page.mouse.click(box.x + red.x, box.y + red.y);
  await expect.poll(async () => (await state(page)).editing).toBe('red');
  await expect(page.locator('.maplibregl-marker').first()).toBeVisible();
  // Done editing lets the route go.
  await page.locator('#edit-mode').click();
  await expect(page.locator('#edit-mode')).toHaveAttribute('aria-pressed', 'false');
  expect((await state(page)).editing).toBeNull();
  await expect(page.locator('.maplibregl-marker')).toHaveCount(0);
  expect(errors).toEqual([]);
});

test('Undo takes edits back, newest first', async ({ page }) => {
  await open(page);
  await expect(page.locator('#undo')).toBeDisabled();
  await page.getByRole('button', { name: 'Remove blue' }).click();
  await page.getByRole('button', { name: 'Remove yellow' }).click();
  await expect.poll(async () => (await state(page)).routes).toEqual(['red']);
  await page.locator('#undo').click();
  await expect.poll(async () => (await state(page)).routes).toEqual(['red', 'yellow']);
  await page.locator('#undo').click();
  await expect.poll(async () => (await state(page)).routes).toEqual(['red', 'blue', 'yellow']);
  await expect(page.locator('#undo')).toBeDisabled();
  await expect.poll(() => hits(page)).toEqual({ red: 'red', blue: 'blue', yellow: 'yellow' });
});
