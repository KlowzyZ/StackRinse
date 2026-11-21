import { expect, test } from '@playwright/test';
import { mkdirSync } from 'node:fs';
import { join } from 'node:path';
import { scanFixture } from './workspace';
import { connectDesktop } from './desktop-bridge';

let fixture: ReturnType<typeof scanFixture>;
test.beforeAll(() => {
  test.setTimeout(240_000);
  fixture = scanFixture();
  expect(fixture.report.projects).toHaveLength(4);
});
test.afterAll(() => fixture?.dispose());

test('scan results resize without horizontal overflow', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  page.on('console', message => {
    if (message.type() === 'error') errors.push(message.text());
  });
  const calls = await connectDesktop(page, fixture);
  await page.goto('/');
  await expect(page).toHaveTitle(/StackRinse/);
  await expect(page.getByRole('heading', { name: /Storage overview/i })).toBeVisible();
  await page.getByRole('button', { name: /Scan folders/i }).first().click();
  await expect(page.getByText('atlas-console', { exact: true }).first()).toBeVisible();
  expect(calls).toContain('scan');
  await page.evaluate(() => document.fonts.ready);

  for (const [width, height] of [[1280, 720], [1440, 900], [1920, 1080], [800, 640]]) {
    await page.setViewportSize({ width, height });
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(width);
    await expect(page.getByRole('heading', { name: /Storage overview/i })).toBeVisible();
    if (process.env.STACKRINSE_SCREENSHOTS && [1440, 800].includes(width)) {
      mkdirSync(process.env.STACKRINSE_SCREENSHOTS, { recursive: true });
      await page.screenshot({ path: join(process.env.STACKRINSE_SCREENSHOTS, 'overview-' + width + '.png') });
    }
  }
  expect(errors).toEqual([]);
});

test('category proportions respond to keyboard selection', async ({ page }) => {
  await connectDesktop(page, fixture);
  await page.goto('/');
  await page.getByRole('button', { name: /Scan folders/i }).first().click();
  const legendItem = page.getByRole('button', { name: /^Dependencies /i });
  await legendItem.focus();
  await page.keyboard.press('Enter');
  await expect(legendItem).toHaveAttribute('aria-pressed', 'true');
  const candidates = fixture.report.projects.flatMap(project => project.candidates);
  const bytes = (category?: string) => candidates
    .filter(c => !category || c.category === category)
    .reduce((total, candidate) => total + candidate.logical_bytes, 0);
  await expect(page.getByText(Math.round(bytes('Dependencies') / bytes() * 100) + '%', { exact: true })).toBeVisible();
  await page.keyboard.press('Space');
  await expect(legendItem).toHaveAttribute('aria-pressed', 'false');
});
