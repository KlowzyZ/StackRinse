import { expect, test } from "@playwright/test";
import { mkdirSync } from "node:fs";
import { join } from "node:path";
import { scanFixture } from "./workspace";
import { connectDesktop } from "./desktop-bridge";

let fixture: ReturnType<typeof scanFixture>;
test.beforeAll(() => {
  test.setTimeout(240_000);
  fixture = scanFixture();
  expect(fixture.report.projects).toHaveLength(4);
});
test.afterAll(() => fixture?.dispose());

test("scan results resize without horizontal overflow", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(message.text());
  });
  const calls = await connectDesktop(page, fixture);
  await page.goto("/");
  await expect(page).toHaveTitle(/StackRinse/);
  await expect(
    page.getByRole("heading", { name: /Storage overview/i }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: /Scan folders/i })
    .first()
    .click();
  await expect(
    page.getByText("atlas-console", { exact: true }).first(),
  ).toBeVisible();
  expect(calls).toContain("scan");
  await page.evaluate(() => document.fonts.ready);

  for (const [width, height] of [
    [1280, 720],
    [1440, 900],
    [1920, 1080],
    [800, 640],
  ]) {
    await page.setViewportSize({ width, height });
    expect(
      await page.evaluate(() => document.documentElement.scrollWidth),
    ).toBeLessThanOrEqual(width);
    await expect(
      page.getByRole("heading", { name: /Storage overview/i }),
    ).toBeVisible();
    if (process.env.STACKRINSE_SCREENSHOTS && [1440, 800].includes(width)) {
      mkdirSync(process.env.STACKRINSE_SCREENSHOTS, { recursive: true });
      await page.screenshot({
        path: join(
          process.env.STACKRINSE_SCREENSHOTS,
          "overview-" + width + ".png",
        ),
      });
    }
  }
  expect(errors).toEqual([]);
});

test("category proportions respond to keyboard selection", async ({ page }) => {
  await connectDesktop(page, fixture);
  await page.goto("/");
  await page
    .getByRole("button", { name: /Scan folders/i })
    .first()
    .click();
  const legendItem = page.getByRole("button", { name: /^Dependencies /i });
  await legendItem.focus();
  expect(
    await page.evaluate(
      () => matchMedia("(prefers-reduced-motion: reduce)").matches,
    ),
  ).toBe(true);
  const timings = await legendItem.evaluate((el) =>
    [
      getComputedStyle(el).animationDuration,
      getComputedStyle(el).transitionDuration,
    ]
      .flatMap((list) => list.split(","))
      .map((duration) => {
        const ms = duration.trim().endsWith("ms");
        return parseFloat(duration) * (ms ? 1 : 1000);
      }),
  );
  expect(Math.max(...timings)).toBeLessThanOrEqual(1);
  await page.keyboard.press("Enter");
  await expect(legendItem).toHaveAttribute("aria-pressed", "true");
  const candidates = fixture.report.projects.flatMap(
    (project) => project.candidates,
  );
  const bytes = (category?: string) =>
    candidates
      .filter((c) => !category || c.category === category)
      .reduce((total, candidate) => total + candidate.logical_bytes, 0);
  await expect(
    page.getByText(Math.round((bytes("Dependencies") / bytes()) * 100) + "%", {
      exact: true,
    }),
  ).toBeVisible();
  await page.keyboard.press("Space");
  await expect(legendItem).toHaveAttribute("aria-pressed", "false");
});

test("filter projects, inspect evidence and review an explicit selection", async ({
  page,
}) => {
  const calls = await connectDesktop(page, fixture);
  await page.goto("/");
  await page
    .getByRole("button", { name: /Scan folders/i })
    .first()
    .click();
  await page.getByRole("button", { name: "Explore", exact: true }).click();
  const search = page.getByRole("textbox", { name: "Search projects" });
  await search.fill("relay");
  await expect(page.getByText("relay-engine", { exact: true })).toBeVisible();
  await expect(page.getByText("atlas-console", { exact: true })).toHaveCount(0);
  await page.getByLabel("Technology filter").selectOption("Python");
  await expect(page.getByText("No matching projects")).toBeVisible();
  await search.fill("");
  await page.getByLabel("Safety filter").selectOption("rebuildable");
  await page.getByRole("button", { name: /signal-notebook/ }).click();
  await expect(page.getByText("__pycache__", { exact: true })).toBeVisible();
  const candidate = page
    .locator(".candidate")
    .filter({ hasText: "__pycache__" });
  await candidate.getByRole("button", { name: "Select for review" }).click();
  await expect(
    candidate.getByRole("button", { name: "Selected" }),
  ).toHaveAttribute("aria-pressed", "true");
  await page.getByRole("button", { name: "Review selection" }).click();
  await expect.poll(() => calls.includes("dry_run")).toBe(true);
  await expect(
    page.getByRole("button", { name: "Clean selected" }),
  ).toBeDisabled();
  await expect(page.getByText("__pycache__", { exact: true })).toBeVisible();
});

test("appearance is restored and exclusions invalidate inventory", async ({
  page,
}) => {
  await connectDesktop(page, fixture);
  await page.goto("/");
  await page.getByRole("button", { name: "Scan folders" }).click();
  await expect(page.getByText("Inventory measured")).toBeVisible();
  const settings = page.getByRole("button", { name: "Settings", exact: true });
  await settings.click();
  await page.getByRole("button", { name: "light", exact: true }).click();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
  await page.getByRole("button", { name: "Overview", exact: true }).click();
  await expect(page.getByText("Inventory measured")).toBeVisible();
  if (process.env.STACKRINSE_SCREENSHOTS) {
    mkdirSync(process.env.STACKRINSE_SCREENSHOTS, { recursive: true });
    await page.screenshot({
      path: join(process.env.STACKRINSE_SCREENSHOTS, "overview-light.png"),
    });
  }
  await settings.click();
  await page
    .getByRole("textbox", { name: "Directory name to exclude" })
    .fill("sandbox");
  await page.getByRole("button", { name: "Add exclusion" }).click();
  await expect(page.getByText("sandbox", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Overview", exact: true }).click();
  await expect(page.getByText("Awaiting first scan")).toBeVisible();
  await page.reload();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
});

test("cancelling a scan makes the partial result read-only", async ({
  page,
}) => {
  await connectDesktop(page, fixture, { waitForCancel: true });
  await page.goto("/");
  await page.getByRole("button", { name: "Scan folders" }).click();
  await page.getByRole("button", { name: "Cancel scan" }).click();
  await expect(page.getByText(/Scan cancelled\./)).toBeVisible();
  await page
    .getByRole("button", { name: /^atlas-console/ })
    .first()
    .click();
  await expect(
    page.getByRole("button", { name: "Select for review" }),
  ).toBeDisabled();
});

test("web preview explains local access and can dismiss the error", async ({
  page,
}) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Scan folders" }).click();
  await expect(page.getByRole("alert")).toContainText(
    "Open StackRinse in the desktop app",
  );
  await page.getByRole("button", { name: "Dismiss error" }).click();
  await expect(page.getByRole("alert")).toHaveCount(0);
});
