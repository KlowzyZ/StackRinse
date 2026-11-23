import { useState } from "react";
import {
  FolderPlus,
  Folder,
  Minus,
  Monitor,
  Moon,
  Sun,
  Shield,
  SlidersHorizontal,
} from "lucide-react";
import type { Settings } from "./model";

export function Preferences({
  settings,
  choose,
  remove,
  unprotect,
  save,
  busy,
}: {
  settings: Settings;
  choose: () => void;
  remove: (s: string) => void;
  unprotect: (s: string) => void;
  save: (appearance: Settings["appearance"], names: string[]) => void;
  busy: boolean;
}) {
  const [draft, setDraft] = useState("");
  return (
    <div className="preferences">
      <section className="panel">
        <header className="sectionHead">
          <div>
            <Folder />
            Scan roots
          </div>
          <button disabled={busy} onClick={choose}>
            <FolderPlus size={14} />
            Add folder
          </button>
        </header>
        <p className="prefHint">
          Discovery only runs inside folders you choose.
        </p>
        {settings.roots.map((s) => (
          <div className="prefRow" key={s}>
            <code>{s}</code>
            <button
              aria-label={"Remove root " + s}
              disabled={busy}
              onClick={() => remove(s)}
            >
              <Minus size={14} />
            </button>
          </div>
        ))}
        {!settings.roots.length && (
          <p className="prefHint">No folders selected.</p>
        )}
      </section>
      <section className="panel">
        <header className="sectionHead">
          <div>
            <SlidersHorizontal />
            Appearance
          </div>
        </header>
        <div className="themeChoices">
          {(["dark", "light", "system"] as const).map((s, i) => {
            const Icon = [Moon, Sun, Monitor][i]!;
            return (
              <button
                aria-pressed={settings.appearance === s}
                disabled={busy}
                key={s}
                onClick={() => save(s, settings.excludedNames)}
              >
                <Icon size={15} />
                {s}
              </button>
            );
          })}
        </div>
      </section>
      <section className="panel">
        <header className="sectionHead">
          <div>
            <Shield />
            Exclusions & protection
          </div>
        </header>
        <p className="prefHint">
          Skip directories by exact name on your next scan.
        </p>
        <form
          className="exclude"
          onSubmit={(e) => {
            e.preventDefault();
            const name = draft.trim();
            if (name && !settings.excludedNames.includes(name)) {
              save(settings.appearance, [...settings.excludedNames, name]);
              setDraft("");
            }
          }}
        >
          <input
            aria-label="Directory name to exclude"
            placeholder="Directory name"
            required
            pattern={"[^/\\\\]+"}
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
          />
          <button disabled={busy || !draft.trim()}>Add exclusion</button>
        </form>
        {settings.excludedNames.map((s) => (
          <div key={s} className="prefRow">
            <code>{s}</code>
            <button
              disabled={busy}
              aria-label={"Remove exclusion " + s}
              onClick={() =>
                save(
                  settings.appearance,
                  settings.excludedNames.filter((v) => v !== s),
                )
              }
            >
              <Minus size={14} />
            </button>
          </div>
        ))}
        <p className="prefHint">
          <b>Protected projects</b>
        </p>
        {settings.protectedProjects.length ? (
          settings.protectedProjects.map((s) => (
            <div key={s} className="prefRow">
              <code>{s}</code>
              <button disabled={busy} onClick={() => unprotect(s)}>
                Unprotect
              </button>
            </div>
          ))
        ) : (
          <p className="prefHint">Protect a project from its detail view.</p>
        )}
      </section>
      <section className="panel">
        <header className="sectionHead">
          <div>About StackRinse</div>
        </header>
        <div className="prefRow">
          <span>Clean builds. Keep code.</span>
          <span className="mono">0.1.0 · MIT · by KLOWZY</span>
        </div>
        <p className="prefHint">
          Cleanup is limited to dry-run review in this build.
        </p>
      </section>
    </div>
  );
}
