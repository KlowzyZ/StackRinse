import { expect, test } from "@playwright/test";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { connectDesktop } from "./desktop-bridge";
import { scanFixture } from "./workspace";

test("shows the measured ecosystem and separates historical rule versions", async ({
  page,
}) => {
  test.setTimeout(120_000);
  const fixture = scanFixture();
  const markers = fileURLToPath(
    new URL(
      "../../../crates/stackrinse-core/tests/fixtures/ecosystem-markers/",
      import.meta.url,
    ),
  );
  const put = (path: string, data: string | Buffer) => {
    const destination = join(fixture.root, path);
    mkdirSync(dirname(destination), { recursive: true });
    writeFileSync(destination, data);
  };
  const nav = (name: string) => page.getByRole("button", { name, exact: true });
  let legacy = true;
  try {
    const calls = await connectDesktop(page, fixture, {
      scanReport: () =>
        legacy
          ? { ...fixture.report, discovery_rule_version: 1 }
          : fixture.rescan(),
    });
    await page.goto("/");
    await nav("Scan folders").click();
    await expect(page.getByText("Inventory measured")).toBeVisible();
    legacy = false;

    put("pub-client/pubspec.yaml", "name: storage_sample\n");
    for (const name of ["package_config.json", "package_graph.json"]) {
      put("pub-client/.dart_tool/" + name, readFileSync(join(markers, name)));
    }
    await nav("Scan folders").click();
    await expect(
      page.locator(".projectRow").filter({ hasText: "pub-client" }),
    ).toBeVisible();
    await page.getByLabel("Group storage map").selectOption("ecosystem");
    const dart = page.getByRole("button", { name: /^Dart / });
    await dart.focus();
    await page.keyboard.press("Enter");
    await expect(dart).toHaveAttribute("aria-pressed", "true");
    await expect(
      page.getByRole("img", { name: /^Dart, .*less than 1% of inventory/ }),
    ).toBeVisible();
    await expect(page.locator(".dialCenter strong")).toHaveText("<1%");
    if (process.env.STACKRINSE_SCREENSHOTS) {
      mkdirSync(process.env.STACKRINSE_SCREENSHOTS, { recursive: true });
      await page.evaluate(() => document.fonts.ready);
      await page.screenshot({
        path: join(
          process.env.STACKRINSE_SCREENSHOTS,
          "ecosystem-overview.png",
        ),
      });
    }

    await nav("Projects").click();
    await page.getByLabel("Technology filter").selectOption("Dart");
    await expect(page.locator(".projectRow")).toHaveCount(1);
    await page.locator(".projectRow").click();
    const candidate = page
      .locator(".candidate")
      .filter({ hasText: ".dart_tool" });
    await expect(candidate).toContainText("Dart · Tooling metadata");
    await expect(candidate).toContainText(
      "package_config.json and package_graph.json",
    );
    await expect(candidate).toContainText("Review required");
    await expect(candidate).toContainText("dart pub get");

    await nav("History").click();
    await expect(page.getByText(/No earlier complete scan used/)).toBeVisible();
    await expect(page.getByLabel("Compare with")).toHaveCount(0);
    expect(calls).not.toContain("history_compare");

    await page.getByText("Scope and warnings", { exact: true }).click();
    await expect(page.getByText(/Discovery rules v2/)).toBeVisible();
    await page.getByText("Storage breakdown", { exact: true }).click();
    await expect(
      page
        .getByRole("table", { name: "Artifact ecosystems" })
        .getByRole("rowheader", { name: "Dart" }),
    ).toBeVisible();
    expect(calls).not.toContain("dry_run");
  } finally {
    fixture.dispose();
  }
});
