import { expect, test, type Page } from "@playwright/test";
import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { connectDesktop } from "./desktop-bridge";
import { scanFixture } from "./workspace";

const pageButton = (page: Page, name: string) =>
  page.getByRole("button", { name, exact: true });

async function scan(page: Page) {
  await pageButton(page, "Scan folders").click();
  await expect(pageButton(page, "Scan folders")).toBeEnabled();
}

async function screenshot(page: Page, name: string) {
  const folder = process.env.STACKRINSE_SCREENSHOTS;
  if (!folder) return;
  mkdirSync(folder, { recursive: true });
  await page.evaluate(() => document.fonts.ready);
  await page.screenshot({ path: join(folder, name + ".png") });
}

test("browses real scan measurements, changes baselines and exports through the desktop bridge", async ({
  page,
}) => {
  const fixture = scanFixture();
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("console", (message) => {
    if (message.type() === "error" || message.type() === "warning")
      errors.push(message.text());
  });
  const exports: unknown[] = [];
  try {
    const calls = await connectDesktop(page, fixture, {
      scanReport: () => fixture.rescan(),
      exportReport: (args) => {
        exports.push(args);
        return args.format === "json";
      },
    });
    await page.goto("/");
    await expect(page).toHaveTitle("StackRinse");
    await scan(page);
    const relay = join(fixture.root, "relay-engine/target/debug/librelay.rlib");
    writeFileSync(relay, Buffer.alloc(16_000_000, 0x41));
    await scan(page);
    writeFileSync(relay, Buffer.alloc(7_100_000, 0x41));
    const newProject = join(fixture.root, "ledger-service");
    mkdirSync(join(newProject, "target"), { recursive: true });
    writeFileSync(join(newProject, "Cargo.toml"), "[package]\nname='ledger'");
    writeFileSync(join(newProject, "target", ".rustc_info.json"), "{}");
    writeFileSync(
      join(newProject, "target", "result"),
      Buffer.alloc(5_000_000, 0x41),
    );
    rmSync(join(fixture.root, "catalog-service"), { recursive: true });
    await scan(page);

    await expect(
      page.getByRole("region", { name: "Latest saved scan" }),
    ).toBeVisible();
    await screenshot(page, "sprint-4-overview");
    await page.getByText("Export inventory", { exact: true }).click();
    await expect(
      page.getByText(/Reports may contain local filesystem paths/),
    ).toBeVisible();
    await pageButton(page, "Save CSV…").click();
    await expect
      .poll(() => exports)
      .toEqual([{ source: { kind: "fresh" }, format: "csv" }]);
    await expect(page.getByText("CSV report saved.")).toHaveCount(0);

    const history = pageButton(page, "History");
    await history.focus();
    await page.keyboard.press("Enter");
    await expect(
      page.getByRole("heading", { name: "Scan history", exact: true }),
    ).toBeFocused();
    await expect(
      page.getByRole("list", { name: "Saved scans" }).getByRole("button"),
    ).toHaveCount(3);
    await expect(
      page.getByRole("region", { name: "Storage trend" }),
    ).toContainText("3 comparable scans");
    await expect(
      page.getByText("Logical size change", { exact: true }),
    ).toBeVisible();
    await expect(
      page.getByRole("region", { name: "Newly observed" }),
    ).toContainText("ledger-service");
    await expect(
      page.getByRole("region", { name: "No longer observed" }),
    ).toContainText("catalog-service");
    await screenshot(page, "sprint-4-history-dark");

    const select = page.getByLabel("Compare with");
    await expect(select.locator("option")).toHaveCount(2);
    await select.selectOption({ index: 1 });
    await expect(
      page.getByRole("region", { name: "Growing projects" }),
    ).toContainText("relay-engine");
    await page
      .getByText("Ecosystem and artifact changes", { exact: true })
      .click();
    await expect(page.getByRole("rowheader", { name: "Rust" })).toBeVisible();
    await page.getByText("Export inventory", { exact: true }).click();
    await pageButton(page, "Save JSON…").click();
    await expect(page.getByText("JSON report saved.")).toBeVisible();
    expect(exports[1]).toEqual({
      source: { kind: "history", id: 3 },
      format: "json",
    });
    expect(calls).not.toContain("dry_run");

    await pageButton(page, "Settings").click();
    await pageButton(page, "light").click();
    await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
    await history.click();
    await expect(
      page.getByText("Logical size change", { exact: true }),
    ).toBeVisible();
    await screenshot(page, "sprint-4-history-light");
    for (const [width, height] of [
      [800, 650],
      [390, 740],
    ]) {
      await page.setViewportSize({ width, height });
      expect(
        await page
          .locator("main")
          .evaluate((el) => el.scrollWidth <= el.clientWidth),
      ).toBe(true);
      if (width === 800) await screenshot(page, "sprint-4-history-800");
    }
    expect(errors).toEqual([]);
  } finally {
    fixture.dispose();
  }
});

test("filters incompatible snapshots and manages saved history with the keyboard", async ({
  page,
}) => {
  const fixture = scanFixture();
  let historyUnavailable = true;
  try {
    const calls = await connectDesktop(page, fixture, {
      scanReport: () => fixture.rescan(),
      historyError: () => historyUnavailable,
    });
    await page.goto("/");
    await pageButton(page, "History").click();
    await expect(
      page.getByText("History unavailable", { exact: true }),
    ).toBeVisible();
    historyUnavailable = false;
    await pageButton(page, "Try again").click();
    await expect(
      page.getByText("No saved scans yet", { exact: true }),
    ).toBeVisible();
    await pageButton(page, "Go to Overview").click();
    await scan(page);
    await pageButton(page, "Settings").click();
    await page.getByLabel("Directory name to exclude").fill("cache");
    await pageButton(page, "Add exclusion").click();
    await expect(page.getByText("cache", { exact: true })).toBeVisible();
    await pageButton(page, "Overview").click();
    await scan(page);
    // A symlink inside an otherwise recognized artifact leaves the measurement partial.
    const { symlinkSync } = await import("node:fs");
    symlinkSync(
      "librelay.rlib",
      join(fixture.root, "relay-engine/target/debug/linked"),
    );
    await scan(page);
    await pageButton(page, "History").click();
    await expect(
      page.getByText("Incomplete inventories are excluded from comparisons."),
    ).toBeVisible();
    await page.getByLabel("Scan completeness").selectOption("complete");
    await expect(page.getByText(/No earlier complete scan used/)).toBeVisible();
    expect(calls).not.toContain("history_compare");
    const search = page.getByRole("textbox", { name: /Search saved scans/ });
    await search.fill("no-matching-root");
    await expect(page.getByText("No matching scans")).toBeVisible();
    await search.fill("stackrinse-qa");
    const scans = page
      .getByRole("list", { name: "Saved scans" })
      .getByRole("button");
    await expect(scans).toHaveCount(2);
    await scans.nth(1).focus();
    await page.keyboard.press("Space");
    await expect(scans.nth(1)).toHaveAttribute("aria-pressed", "true");
    const remove = pageButton(page, "Remove saved scan");
    await remove.focus();
    await page.keyboard.press("Enter");
    const confirmation = page.getByRole("dialog", {
      name: "Remove this saved scan?",
    });
    await expect(
      confirmation.getByRole("button", { name: "Cancel" }),
    ).toBeFocused();
    await page.keyboard.press("Escape");
    await expect(confirmation).toHaveCount(0);
    await expect(remove).toBeFocused();
    await page.keyboard.press("Enter");
    await page.keyboard.press("Tab");
    await expect(
      confirmation.getByRole("button", { name: "Remove scan" }),
    ).toBeFocused();
    await page.keyboard.press("Enter");
    await expect(
      page.getByText("Saved scan removed.", { exact: true }),
    ).toBeVisible();
    await expect(
      page.getByRole("heading", { name: "Scan timeline" }),
    ).toBeFocused();

    await page.getByText("History settings", { exact: true }).click();
    await page.getByLabel("Keep latest").selectOption("10");
    await pageButton(page, "Apply limit").click();
    await expect(
      page.getByText("History limit updated.", { exact: true }),
    ).toBeVisible();
    await page.reload();
    await pageButton(page, "History").click();
    await expect(
      page.getByRole("list", { name: "Saved scans" }).getByRole("button"),
    ).toHaveCount(2);
    await page.getByText("History settings", { exact: true }).click();
    await expect(page.getByLabel("Keep latest")).toHaveValue("10");
    await pageButton(page, "Clear history…").click();
    await page
      .getByRole("dialog", { name: "Clear scan history?" })
      .getByRole("button", { name: "Clear history", exact: true })
      .click();
    await expect(
      page.getByText("No saved scans yet", { exact: true }),
    ).toBeVisible();
    expect(calls).not.toContain("dry_run");
  } finally {
    fixture.dispose();
  }
});
