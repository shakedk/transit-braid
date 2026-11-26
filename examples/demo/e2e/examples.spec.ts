// The Examples page: a card per network, each opening the demo with that network.
import { expect, test } from '@playwright/test';

test('lists every network as a card, and a card opens it in the demo', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', (error) => errors.push(error.message));
  await page.goto('/examples.html');
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true');
  await expect(page.locator('#real .card')).toHaveCount(2);
  await expect(page.locator('#scenarios .card')).toHaveCount(12);
  // Each scenario shows its routes as given, and what a good drawing shows.
  await expect(page.locator('#scenarios .card svg.thumb')).toHaveCount(12);
  await expect(page.locator('#scenarios .card .look')).toHaveCount(12);
  const ySplit = page.locator('.card[data-network="y-split"]');
  await expect(ySplit).toHaveAttribute('href', './?network=y-split');
  await ySplit.click();
  await page.waitForFunction(
    () => window.braidDebug?.network === 'y-split' && /side by side/.test(window.braidDebug.status),
  );
  expect(await page.evaluate(() => window.braidDebug!.routes)).toEqual(['A', 'B', 'C']);
  expect(errors).toEqual([]);
});
