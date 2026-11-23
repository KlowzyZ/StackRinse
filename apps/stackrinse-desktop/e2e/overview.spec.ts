import { expect, test } from "@playwright/test";
import { mkdirSync, writeFileSync } from "node:fs";
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
  if (process.env.STACKRINSE_SCREENSHOTS) {
    mkdirSync(process.env.STACKRINSE_SCREENSHOTS, { recursive: true });
    await page.screenshot({
      path: join(process.env.STACKRINSE_SCREENSHOTS, "dry-run-review.png"),
    });
  }
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
  let ready = false;
  await connectDesktop(page, fixture, {
    waitForCancel: true,
    cancelReady: () => ready,
  });
  await page.goto("/");
  await page.getByRole("button", { name: "Scan folders" }).click();
  await page.getByRole("button", { name: "History", exact: true }).click();
  const cancel = page.getByRole("button", { name: "Cancel scan" });
  await cancel.click();
  await expect(cancel).toBeEnabled();
  ready = true;
  await cancel.click();
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

test("dismissing the folder picker leaves discovery idle and preserves an existing scan", async ({
  page,
}) => {
  let finishPicker!: (picked: boolean) => void;
  const calls = await connectDesktop(page, fixture, {
    pickFolder: () =>
      new Promise((resolve) => {
        finishPicker = resolve;
      }),
  });
  await page.goto("/");
  await page.getByRole("button", { name: "Scan folders" }).click();
  await expect.poll(() => calls.includes("choose_root")).toBe(true);
  await expect(
    page.getByRole("button", { name: "Choosing folder…" }),
  ).toBeDisabled();
  await expect(page.getByRole("button", { name: "Cancel scan" })).toHaveCount(
    0,
  );
  finishPicker(false);
  await expect(
    page.getByRole("button", { name: "Scan folders" }),
  ).toBeEnabled();
  expect(calls).not.toContain("scan");

  await page.getByRole("button", { name: "Scan folders" }).click();
  await expect
    .poll(() => calls.filter((c) => c === "choose_root").length)
    .toBe(2);
  finishPicker(true);
  await expect(page.getByText("Inventory measured")).toBeVisible();
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  const add = page.getByRole("button", { name: "Add folder" });
  await add.click();
  await expect
    .poll(() => calls.filter((c) => c === "choose_root").length)
    .toBe(3);
  finishPicker(false);
  await expect(add).toBeEnabled();
  await page.getByRole("button", { name: "Overview", exact: true }).click();
  await expect(page.getByText("Inventory measured")).toBeVisible();
});

test("repository protection and partial sizes stay visible while inspecting a filtered list", async ({
  page,
}) => {
  const guarded = scanFixture((root) => {
    writeFileSync(join(root, "relay-engine", ".git"), "gitdir: ../unavailable");
    mkdirSync(join(root, "relay-engine", "target", ".git"));
  });
  try {
    await connectDesktop(page, guarded);
    await page.goto("/");
    await page.getByRole("button", { name: "Scan folders" }).click();
    await expect(page.locator(".instrumentFoot")).toContainText(
      "Partial inventory",
    );
    await page.getByRole("button", { name: "Explore", exact: true }).click();
    const search = page.getByRole("textbox", { name: "Search projects" });
    await search.fill("relay");
    await page.getByRole("button", { name: /relay-engine/ }).click();
    await expect(
      page.getByText("Repository protection", { exact: true }),
    ).toBeVisible();
    await expect(
      page.getByRole("button", {
        name: /protect project|remove protection preference/i,
      }),
    ).toHaveCount(0);
    await expect(
      page.getByRole("button", { name: "Select for review" }),
    ).toBeDisabled();
    if (process.env.STACKRINSE_SCREENSHOTS) {
      mkdirSync(process.env.STACKRINSE_SCREENSHOTS, { recursive: true });
      await page.screenshot({
        path: join(
          process.env.STACKRINSE_SCREENSHOTS,
          "repository-protection.png",
        ),
      });
    }
    await expect(page.locator(".candidateTitle .number")).toContainText(
      "observed",
    );
    await page
      .locator("main")
      .getByRole("button", { name: "Projects", exact: true })
      .click();
    await expect(search).toHaveValue("relay");
    await expect(page.locator(".projectRow")).toHaveCount(1);
  } finally {
    guarded.dispose();
  }
});

test("zero-byte categories and epoch manifest dates have explicit values", async ({
  page,
}) => {
  const project = fixture.report.projects.find(
    (p) => p.name === "relay-engine",
  )!;
  const report = {
    ...fixture.report,
    totals: { logical_bytes: 0, reviewable_bytes: 0, protected_bytes: 0 },
    projects: [
      {
        ...project,
        logical_bytes: 0,
        last_activity: 0,
        candidates: project.candidates.map((c) => ({ ...c, logical_bytes: 0 })),
      },
    ],
  };
  await connectDesktop(page, { ...fixture, report });
  await page.goto("/");
  await page.getByRole("button", { name: "Scan folders" }).click();
  await expect(page.locator(".dialCenter span")).toHaveText("category");
  await page.getByRole("button", { name: /^Build outputs/ }).click();
  await expect(page.locator(".dialCenter strong")).toHaveText("0 B");
  await expect(page.locator(".dialCenter span")).toHaveText("observed");
  await expect(page.locator(".activityRow")).toContainText(/1969|1970/);
});

test("page and filter artifact paths, then revise the dry-run selection", async ({
  page,
}) => {
  const several = scanFixture((root) => {
    for (let i = 0; i < 42; i++) {
      const pkg = join(
        root,
        "signal-notebook",
        "package-" + String(i).padStart(2, "0"),
      );
      mkdirSync(join(pkg, "__pycache__"), { recursive: true });
      writeFileSync(join(pkg, "unit.py"), "pass");
      writeFileSync(join(pkg, "__pycache__", "unit.cpython-312.pyc"), "123");
    }
  });
  try {
    const calls = await connectDesktop(page, several);
    await page.setViewportSize({ width: 640, height: 600 });
    await page.goto("/");
    await page.getByRole("button", { name: "Scan folders" }).click();
    await page
      .getByRole("button", { name: /^signal-notebook/ })
      .first()
      .click();
    await expect(page.locator(".candidate")).toHaveCount(20);
    await page.getByRole("button", { name: "Next candidates" }).focus();
    await page.keyboard.press("Enter");
    await expect(page.getByRole("status")).toContainText("21–40 of 44");
    const filter = page.getByRole("textbox", {
      name: "Filter candidate paths",
    });
    await filter.fill("package-4");
    await expect(page.locator(".candidate")).toHaveCount(2);
    await page
      .getByRole("button", { name: "Select for review" })
      .first()
      .click();
    await page.getByRole("button", { name: "Select for review" }).click();
    await page.getByRole("button", { name: "Review selection" }).click();
    await expect(page.locator(".planTotal strong")).toHaveText("6 B");
    await page
      .getByRole("button", { name: /Remove from review: .*package-40/ })
      .click();
    await expect(page.locator(".planTotal strong")).toHaveText("3 B");
    expect(calls.filter((c) => c === "dry_run")).toHaveLength(2);
    await expect(
      page.getByRole("button", { name: "Clean selected" }),
    ).toBeDisabled();
    await page
      .getByRole("button", { name: /Remove from review: .*package-41/ })
      .click();
    await expect(page.getByText("Your review is empty")).toBeVisible();
    await expect(
      page.getByRole("button", { name: "Browse projects" }),
    ).toBeFocused();
  } finally {
    several.dispose();
  }
});

test("a failed exclusion save preserves the draft for retry", async ({ page }) => {
  await connectDesktop(page, fixture, { rejectPreferences: true });
  await page.goto("/");
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  const draft = page.getByRole("textbox", { name: "Directory name to exclude" });
  await draft.fill("generated");
  await page.getByRole("button", { name: "Add exclusion" }).click();
  await expect(page.getByRole("alert")).toContainText("Settings are busy");
  await expect(draft).toHaveValue("generated");
});
