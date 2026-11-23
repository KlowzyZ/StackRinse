import type { Page } from "@playwright/test";
import type { scanFixture } from "./workspace";

// Test-only adapter for the Tauri IPC transport. The page receives the actual
// CLI report. The production bundle never imports this module.
export async function connectDesktop(
  page: Page,
  fixture: ReturnType<typeof scanFixture>,
  options: {
    waitForCancel?: boolean;
    pickFolder?: () => Promise<boolean>;
  } = {},
) {
  let settings = {
    roots: [] as string[],
    appearance: "dark",
    excludedNames: [] as string[],
    protectedProjects: [] as string[],
  };
  const commands: string[] = [];
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
        case "scan":
          if (options.waitForCancel) {
            return new Promise((resolve) => {
              finishScan = () =>
                resolve({ ...fixture.report, cancelled: true });
            });
          }
          return fixture.report;
        case "cancel_scan":
          finishScan?.();
          return;
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
