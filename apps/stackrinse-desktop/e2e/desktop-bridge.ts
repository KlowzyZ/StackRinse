import type { Page } from "@playwright/test";
import type { scanFixture } from "./workspace";
import type { HistoryDetail } from "../src/history-api";
import type { Report } from "../src/model";

// Test-only adapter for the Tauri IPC transport. The page receives the actual
// CLI report. The production bundle never imports this module.
export async function connectDesktop(
  page: Page,
  fixture: ReturnType<typeof scanFixture>,
  options: {
    waitForCancel?: boolean;
    rejectPreferences?: boolean;
    cancelReady?: () => boolean;
    pickFolder?: () => Promise<boolean>;
    scanReport?: () => Report;
    historyError?: () => boolean;
    exportReport?: (args: Record<string, unknown>) => boolean;
  } = {},
) {
  let settings = {
    roots: [] as string[],
    appearance: "dark",
    excludedNames: [] as string[],
    protectedProjects: [] as string[],
  };
  const commands: string[] = [];
  let history: HistoryDetail[] = [];
  let retention = 50;
  let nextId = 1;
  function record(report: Report, duration: number) {
    const candidates = report.projects.flatMap((project) => project.candidates);
    const incomplete = candidates.filter(
      (candidate) => !candidate.size_complete,
    ).length;
    const completedAtMs = Date.now();
    const categories: Record<string, number> = {};
    const ecosystems: Record<string, number> = {};
    for (const project of report.projects) {
      for (const candidate of project.candidates) {
        categories[candidate.category] =
          (categories[candidate.category] ?? 0) + candidate.logical_bytes;
        const ecosystem = candidate.ecosystem;
        ecosystems[ecosystem] =
          (ecosystems[ecosystem] ?? 0) + candidate.logical_bytes;
      }
    }
    history.unshift({
      entry: {
        id: nextId++,
        completed_at_ms: completedAtMs,
        duration_ms: duration,
        discovery_rule_version: report.discovery_rule_version,
        scope_key: JSON.stringify([
          report.discovery_rule_version,
          settings.roots,
          settings.excludedNames,
          settings.protectedProjects,
        ]),
        root_hint: settings.roots[0] ?? "",
        root_count: settings.roots.length,
        project_count: report.projects.length,
        candidate_count: candidates.length,
        warning_count: report.warnings.length,
        incomplete_candidate_count: incomplete,
        directories: report.directories,
        complete: !incomplete && !report.warnings.length,
        totals: report.totals,
      },
      roots: [...settings.roots],
      exclusions: [...settings.excludedNames],
      warnings: report.warnings,
      categories,
      ecosystems,
      project_details_available: true,
      largest_projects: report.projects
        .map((project) => ({
          ...project,
          candidate_count: project.candidates.length,
        }))
        .sort((a, b) => b.logical_bytes - a.logical_bytes),
    });
    history = history.slice(0, retention);
    return { ...report, completedAtMs };
  }
  let finishScan: (() => void) | undefined;
  await page.exposeFunction(
    "stackrinseTestInvoke",
    async (command: string, args: Record<string, unknown>) => {
      commands.push(command);
      switch (command) {
        case "get_settings":
          break;
        case "choose_root":
          if (!options.pickFolder || (await options.pickFolder()))
            settings.roots = [fixture.root];
          break;
        case "remove_root":
          settings.roots = settings.roots.filter((root) => root !== args.root);
          break;
        case "set_preferences":
          if (options.rejectPreferences)
            throw Error("Settings are busy. Try again.");
          settings = {
            ...settings,
            appearance: String(args.appearance),
            excludedNames: args.excludedNames as string[],
          };
          break;
        case "set_project_protection":
          settings.protectedProjects = settings.protectedProjects.filter(
            (path) => path !== args.path,
          );
          if (args.protected)
            settings.protectedProjects.push(String(args.path));
          break;
        case "scan": {
          if (options.waitForCancel) {
            return new Promise((resolve) => {
              finishScan = () =>
                resolve({
                  ...fixture.report,
                  totals: { ...fixture.report.totals, reviewable_bytes: 0 },
                  cancelled: true,
                });
            });
          }
          const started = performance.now();
          return record(
            options.scanReport?.() ?? fixture.report,
            Math.round(performance.now() - started),
          );
        }
        case "history_list":
          if (options.historyError?.())
            throw Error("Scan history is unavailable");
          return { retention, entries: history.map((item) => item.entry) };
        case "history_detail":
          return history.find((item) => item.entry.id === args.id);
        case "history_remove":
          history = history.filter((item) => item.entry.id !== args.id);
          return;
        case "history_clear":
          history = [];
          return;
        case "history_set_retention":
          retention = Number(args.count);
          history = history.slice(0, retention);
          return;
        case "history_compare": {
          const before = history.find(
            (item) => item.entry.id === args.beforeId,
          )!;
          const after = history.find((item) => item.entry.id === args.afterId)!;
          const buckets = (
            left: Record<string, number>,
            right: Record<string, number>,
          ) =>
            [...new Set([...Object.keys(left), ...Object.keys(right)])].map(
              (name) => ({
                name,
                before: left[name] ?? 0,
                after: right[name] ?? 0,
              }),
            );
          const changes = (kind: "growth" | "discovered" | "missing") => {
            const left = new Map(
              before.largest_projects.map((p) => [p.path, p.logical_bytes]),
            );
            const right = new Map(
              after.largest_projects.map((p) => [p.path, p.logical_bytes]),
            );
            const projects = [
              ...new Set([...left.keys(), ...right.keys()]),
            ].flatMap((path) => {
              const prior = left.get(path) ?? null,
                later = right.get(path) ?? null;
              const matches =
                kind === "growth"
                  ? prior !== null && later !== null && later > prior
                  : kind === "discovered"
                    ? prior === null
                    : later === null;
              return matches ? [{ path, before: prior, after: later }] : [];
            });
            return { count: projects.length, projects };
          };
          return {
            logical_bytes_delta:
              after.entry.totals.logical_bytes -
              before.entry.totals.logical_bytes,
            ecosystems: buckets(before.ecosystems, after.ecosystems),
            categories: buckets(before.categories, after.categories),
            growth: changes("growth"),
            discovered: changes("discovered"),
            no_longer_observed: changes("missing"),
          };
        }
        case "export_inventory":
          return options.exportReport?.(args) ?? false;
        case "cancel_scan":
          if (!finishScan || (options.cancelReady && !options.cancelReady()))
            return false;
          finishScan();
          return true;
        case "dry_run": {
          const paths = new Set(args.paths as string[]);
          const candidates = fixture.plan.candidates.filter((candidate) =>
            paths.has(candidate.path),
          );
          return {
            candidates,
            logical_bytes: candidates.reduce(
              (sum, candidate) => sum + candidate.logical_bytes,
              0,
            ),
          };
        }
        case "plugin:event|listen":
          return 1;
        case "plugin:event|unlisten":
          return;
        default:
          throw Error("Unexpected desktop command " + command);
      }
      return settings;
    },
  );
  await page.addInitScript(() => {
    let handle = 0;
    Object.assign(window, {
      __TAURI_INTERNALS__: {
        invoke: (command: string, args = {}) =>
          (
            window as unknown as {
              stackrinseTestInvoke: (
                cmd: string,
                args: object,
              ) => Promise<unknown>;
            }
          ).stackrinseTestInvoke(command, args),
        transformCallback: () => ++handle,
        unregisterCallback() {},
      },
      __TAURI_EVENT_PLUGIN_INTERNALS__: { unregisterListener() {} },
    });
  });
  return commands;
}
