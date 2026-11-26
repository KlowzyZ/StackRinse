import { test, expect, type Page } from "@playwright/test";
import { connectDesktop } from "./desktop-bridge";
import { scanFixture } from "./workspace";
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { execFileSync } from "node:child_process";
const out = process.env.STACKRINSE_CAPTURE ?? "/tmp/stackrinse-before";
for (const theme of ["dark", "light"])
  test("capture " + theme, async ({ page }) => {
    test.skip(
      !process.env.STACKRINSE_CAPTURE,
      "Set STACKRINSE_CAPTURE to a screenshot output directory",
    );
    const fixture = scanFixture();
    let scan = 0;
    const runtimeErrors: string[] = [];
    page.on("pageerror", (error) => runtimeErrors.push(error.message));
    let delayed = false;
    let failed = false;
    await connectDesktop(page, fixture, {
      pickFolder: async () => {
        if (delayed) await new Promise((r) => setTimeout(r, 1200));
        return true;
      },
      scanReport: () => {
        if (failed)
          throw Error("The selected scan root is no longer available.");
        return scan++ === 0 ? fixture.report : fixture.rescan();
      },
    });
    mkdirSync(out, { recursive: true });
    const btn = (name: string | RegExp) =>
      page.getByRole("button", { name, exact: typeof name === "string" });
    const snap = async (name: string, target: Page = page) => {
      await target.evaluate(() => document.fonts.ready);
      await target.screenshot({
        path: join(out, theme + "-" + name + ".png"),
        animations: "disabled",
      });
    };
    try {
      await page.goto("/");
      await btn("Settings").click();
      await btn(theme).click();
      await expect(page.locator("html")).toHaveAttribute("data-theme", theme);
      await snap("10-settings");
      await btn("Overview").click();
      await snap("01-first-launch");
      delayed = true;
      await btn("Scan folders").click();
      await expect(btn("Choosing folder…")).toBeVisible();
      await snap("02-folder-picker");
      await expect(page.locator(".instrumentFoot")).toContainText(
        "Inventory measured",
      );
      await snap("03-overview");
      await page.getByLabel("Group storage map").selectOption("ecosystem");
      await snap("04-ecosystems");
      await page.getByText("Export inventory", { exact: true }).click();
      await snap("05-export");
      await btn("Explore").click();
      await snap("06-projects");
      await btn(/^atlas-console/).click();
      await snap("07-project-detail");
      await btn("Select for review").first().click();
      await btn("Review selection").click();
      await expect(page.locator(".planTotal strong")).toBeVisible();
      await snap("08-review");
      await btn("Overview").click();
      writeFileSync(
        join(
          fixture.root,
          "atlas-console/node_modules/ui-runtime/dist/extra.js",
        ),
        Buffer.alloc(3_500_000),
      );
      await btn("Scan folders").click();
      await expect(page.locator(".instrumentFoot")).toContainText(
        "Inventory measured",
      );
      await btn("History").click();
      await expect(
        page.getByText("Logical size change", { exact: true }),
      ).toBeVisible();
      await snap("09-history");
      await page
        .getByText("Ecosystem and artifact changes", { exact: true })
        .click();
      await page
        .getByText("Ecosystem and artifact changes", { exact: true })
        .scrollIntoViewIfNeeded();
      await snap("11-comparison");
      await page.setViewportSize({ width: 800, height: 650 });
      await btn("Overview").click();
      await snap("12-compact");
      failed = true;
      await btn("Scan folders").click();
      await expect(page.getByRole("alert")).toBeVisible();
      await snap("13-error");
      expect(runtimeErrors).toEqual([]);
    } finally {
      fixture.dispose();
    }
    const more = await page.context().newPage();
    const slow = scanFixture((root) => {
      writeFileSync(
        join(root, "relay-engine", ".git"),
        "gitdir: ../unavailable",
      );
      mkdirSync(join(root, "relay-engine", "target", ".git"));
      mkdirSync(join(root, "empty"));
    });
    const root = join(slow.root, "empty");
    const emptyReport = JSON.parse(
      execFileSync(
        "cargo",
        [
          "run",
          "--locked",
          "--quiet",
          "-p",
          "stackrinse-cli",
          "--",
          "scan",
          root,
          "--json",
        ],
        { encoding: "utf8" },
      ),
    );
    const button = (name: string | RegExp) =>
      more.getByRole("button", { name, exact: typeof name === "string" });
    const appearance = async () => {
      await more.goto("/");
      await button("Settings").click();
      await button(theme).click();
      await expect(more.locator("html")).toHaveAttribute("data-theme", theme);
      await button("Overview").click();
    };
    try {
      await connectDesktop(more, slow, { waitForCancel: true });
      await appearance();
      await button("Scan folders").click();
      await expect(button("Cancel scan")).toBeVisible();
      await snap("14-progress", more);
      await button("Cancel scan").click();
      await expect(more.getByText(/Scan cancelled\./)).toBeVisible();
      await snap("15-partial", more);
      await more.close();

      const scenarios = await page.context().newPage();
      let empty = false;
      await connectDesktop(scenarios, slow, {
        scanReport: () =>
          empty
            ? emptyReport
            : {
                ...slow.report,
                warnings: [
                  "A fixture directory could not be read. Observed sizes may be incomplete.",
                ],
              },
      });
      const action = (name: string | RegExp) =>
        scenarios.getByRole("button", {
          name,
          exact: typeof name === "string",
        });
      try {
        await scenarios.goto("/");
        await action("Settings").click();
        await action(theme).click();
        await expect(scenarios.locator("html")).toHaveAttribute(
          "data-theme",
          theme,
        );
        await action("Overview").click();
        await action("Scan folders").click();
        const warning = scenarios.getByText("1 scan warning", { exact: true });
        await warning.click();
        await snap("16-warning", scenarios);
        await scenarios
          .locator(".projectRow")
          .filter({ hasText: "relay-engine" })
          .click();
        await expect(
          scenarios.getByText("Repository protection", { exact: true }),
        ).toBeVisible();
        await snap("17-protected", scenarios);
        await action("Overview").click();
        empty = true;
        await action("Scan folders").click();
        await expect(
          scenarios.getByText("No projects discovered", { exact: true }),
        ).toBeVisible();
        await snap("18-empty-inventory", scenarios);
      } finally {
        await scenarios.close();
      }
    } finally {
      await more.close();
      slow.dispose();
    }
  });
