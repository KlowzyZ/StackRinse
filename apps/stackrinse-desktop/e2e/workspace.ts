import type { Report, Plan } from "../src/model";
import { execFileSync } from "node:child_process";
import {
  mkdtempSync,
  mkdirSync,
  realpathSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";

// Each payload is a small, allocated file. All reported sizes come from the core.
export function scanFixture(prepare?: (root: string) => void) {
  const root = realpathSync(mkdtempSync(join(tmpdir(), "stackrinse-qa-")));
  const put = (name: string, data: string | number) => {
    const path = join(root, name);
    mkdirSync(dirname(path), { recursive: true });
    writeFileSync(
      path,
      typeof data === "number" ? Buffer.alloc(data, 0x41) : data,
    );
  };
  try {
    put(
      "atlas-console/package.json",
      '{"name":"atlas-console","private":true}',
    );
    put(
      "atlas-console/node_modules/.package-lock.json",
      '{"lockfileVersion":3}',
    );
    put("atlas-console/node_modules/ui-runtime/dist/runtime.js", 6_800_000);
    put(
      "relay-engine/Cargo.toml",
      '[package]\nname = "relay-engine"\nversion = "0.1.0"',
    );
    put("relay-engine/target/.rustc_info.json", "{}");
    put("relay-engine/target/debug/librelay.rlib", 4_100_000);
    put(
      "signal-notebook/pyproject.toml",
      '[project]\nname = "signal-notebook"\nversion = "0.1.0"',
    );
    put(
      "signal-notebook/.venv/pyvenv.cfg",
      "include-system-site-packages = false",
    );
    put("signal-notebook/.venv/lib/site-packages/analysis.dat", 2_400_000);
    put("signal-notebook/signal.py", "def analyze(): return 0");
    put("signal-notebook/__pycache__/signal.cpython-312.pyc", 164_000);
    put(
      "catalog-service/pom.xml",
      "<project><artifactId>catalog-service</artifactId></project>",
    );
    put("catalog-service/target/classes/org/example/Catalog.class", 920_000);
    prepare?.(root);
    const cli = (args: string[]) =>
      JSON.parse(
        execFileSync(
          "cargo",
          [
            "run",
            "--locked",
            "--quiet",
            "-p",
            "stackrinse-cli",
            "--",
            ...args,
            root,
            "--json",
          ],
          { encoding: "utf8" },
        ),
      );
    const report: Report = cli(["scan"]);
    const plan: Plan = cli(["clean", "--dry-run"]).plan;
    return {
      root,
      report,
      plan,
      rescan: (): Report => cli(["scan"]),
      dispose: () => rmSync(root, { recursive: true, force: true }),
    };
  } catch (error) {
    rmSync(root, { recursive: true, force: true });
    throw error;
  }
}
