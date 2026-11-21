import { createRoot } from "react-dom/client";
import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  Activity,
  ArrowLeft,
  ArrowRight,
  ChevronRight,
  CircleAlert,
  ClipboardCheck,
  FolderOpen,
  History,
  LayoutDashboard,
  LockKeyhole,
  Monitor,
  ScanLine,
  Settings2,
  ShieldCheck,
  Square,
  Terminal,
  X,
} from "lucide-react";
import "@fontsource-variable/inter";
import "@fontsource/ibm-plex-mono/400.css";
import mark from "../src-tauri/icons/mark.svg";
import "./style.css";
import { Overview } from "./overview";
import { Explorer, ProjectDetails } from "./projects";
import { Preferences } from "./preferences";
import { Empty, RiskLabel } from "./shared";
import {
  basename,
  bytes,
  initialSettings,
  type Plan,
  type Project,
  type Report,
  type Settings,
} from "./model";

const navigation = [
  { name: "Overview", icon: LayoutDashboard },
  { name: "Projects", icon: FolderOpen },
  { name: "Cleanup review", icon: ClipboardCheck },
  { name: "History", icon: History },
  { name: "Settings", icon: Settings2 },
] as const;
type Screen = (typeof navigation)[number]["name"] | "Project details";
const titles: Record<Screen, string> = {
  Overview: "Storage overview",
  Projects: "Project explorer",
  "Cleanup review": "Cleanup review",
  History: "Operation history",
  Settings: "Workspace settings",
  "Project details": "Project details",
};
const subtitles: Partial<Record<Screen, string>> = {
  Overview: "Understand the build artifacts in your workspace.",
  Projects: "Explore projects, artifact sizes, and rebuild costs.",
  "Cleanup review": "Inspect the exact paths included in your dry run.",
  History: "Results of completed filesystem operations.",
  Settings: "Choose what StackRinse scans and how it appears.",
};
const native = "__TAURI_INTERNALS__" in window;

function App() {
  const [screen, setScreen] = useState<Screen>("Overview"),
    [detail, setDetail] = useState<Project | null>(null);
  const [settings, setSettings] = useState<Settings>(initialSettings),
    [report, setReport] = useState<Report | null>(null),
    [lastScan, setLastScan] = useState(""),
    [scanning, setScanning] = useState(false),
    [cancelling, setCancelling] = useState(false),
    [visited, setVisited] = useState(0);
  const [error, setError] = useState(""),
    [saving, setSaving] = useState(false),
    [selected, setSelected] = useState<Set<string>>(new Set()),
    [plan, setPlan] = useState<Plan | null>(null);
  const heading = useRef<HTMLHeadingElement>(null),
    scanLock = useRef(false),
    settingLock = useRef(false);
  const busy = scanning || saving;
  useEffect(() => {
    if (!native) return;
    let live = true;
    invoke<Settings>("get_settings")
      .then((s) => {
        if (live) setSettings(s);
      })
      .catch((e) => setError(String(e)));
    const registration = listen<{ visited: number }>("scan-progress", (e) => {
      if (live) setVisited(e.payload.visited);
    }).catch((e) => {
      if (live) setError(String(e));
      return () => {};
    });
    return () => {
      live = false;
      void registration.then((f) => f());
    };
  }, []);
  useEffect(() => {
    const media = window.matchMedia("(prefers-color-scheme: light)");
    const apply = () => {
      document.documentElement.dataset.theme =
        settings.appearance === "system"
          ? media.matches
            ? "light"
            : "dark"
          : settings.appearance;
    };
    apply();
    media.addEventListener("change", apply);
    return () => media.removeEventListener("change", apply);
  }, [settings.appearance]);
  const requireDesktop = () => {
    if (native) return true;
    setError(
      "Open StackRinse in the desktop app to access local folders and preferences.",
    );
    return false;
  };
  const navigate = (s: Screen) => {
    setScreen(s);
    requestAnimationFrame(() => {
      heading.current?.focus();
      document.querySelector("main")?.scrollTo(0, 0);
    });
  };
  const open = (p: Project) => {
    setDetail(p);
    navigate("Project details");
  };
  const clearInventory = () => {
    setReport(null);
    setLastScan("");
    setSelected(new Set());
    setPlan(null);
    setDetail(null);
    if (screen === "Project details") navigate("Projects");
  };
  async function update(
    command: string,
    args?: Record<string, unknown>,
    reset = false,
  ) {
    if (scanLock.current || settingLock.current || !requireDesktop()) return;
    settingLock.current = true;
    setSaving(true);
    setError("");
    try {
      const next = await invoke<Settings>(command, args);
      setSettings(next);
      if (reset) clearInventory();
    } catch (e) {
      setError(String(e));
    } finally {
      settingLock.current = false;
      setSaving(false);
    }
  }
  async function scan() {
    if (scanLock.current || settingLock.current || !requireDesktop()) return;
    scanLock.current = true;
    setError("");
    setScanning(true);
    setVisited(0);
    setCancelling(false);
    try {
      if (!settings.roots.length) {
        const s = await invoke<Settings>("choose_root");
        setSettings(s);
        if (!s.roots.length) return;
      }
      setPlan(null);
      setSelected(new Set());
      const result = await invoke<Report>("scan");
      setReport(result);
      setLastScan(
        new Date().toLocaleTimeString(undefined, {
          hour: "2-digit",
          minute: "2-digit",
        }),
      );
      setDetail(null);
      navigate("Overview");
    } catch (e) {
      clearInventory();
      setError(String(e));
    } finally {
      scanLock.current = false;
      setScanning(false);
      setCancelling(false);
    }
  }
  async function cancel() {
    setCancelling(true);
    try {
      await invoke("cancel_scan");
    } catch (e) {
      setError(String(e));
      setCancelling(false);
    }
  }
  function toggle(path: string) {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
    setPlan(null);
  }
  async function review() {
    navigate("Cleanup review");
    setPlan(null);
    if (!selected.size || !native || settingLock.current || scanLock.current)
      return;
    settingLock.current = true;
    setSaving(true);
    try {
      setPlan(await invoke<Plan>("dry_run", { paths: [...selected] }));
    } catch (e) {
      setError(String(e));
    } finally {
      settingLock.current = false;
      setSaving(false);
    }
  }
  const change = (s: Screen) =>
    s === "Cleanup review" ? void review() : navigate(s);
  const active = screen === "Project details" ? "Projects" : screen;
  return (
    <div className="shell">
      <aside>
        <div className="brand">
          <img src={mark} alt="" />
          <span>StackRinse</span>
        </div>
        <span className="navLabel">Workspace</span>
        <nav aria-label="Workspace">
          {navigation.slice(0, 4).map(({ name, icon: Icon }) => (
            <button
              key={name}
              title={name}
              aria-current={active === name ? "page" : undefined}
              onClick={() => change(name)}
            >
              <Icon />
              {<span>{name}</span>}
              {name === "Cleanup review" && !!selected.size && (
                <small>{selected.size}</small>
              )}
            </button>
          ))}
        </nav>
        <div className="railBottom">
          <nav aria-label="Preferences">
            <button
              title="Settings"
              aria-current={active === "Settings" ? "page" : undefined}
              onClick={() => change("Settings")}
            >
              <Settings2 />
              <span>Settings</span>
            </button>
          </nav>
          <div className="byline">
            <img src={mark} alt="" />
            <span>by KLOWZY</span>
          </div>
        </div>
      </aside>
      <div className="workbench">
        <header className="topbar">
          <div>
            <Monitor size={14} />
            <span>Workspace</span>
            <ChevronRight size={12} />
            <b>{active}</b>
          </div>
          <span className="local">
            <i />
            Local session
          </span>
        </header>
        <main>
          <div className="canvas">
            <div className="pageHead">
              <div>
                {screen === "Project details" && (
                  <button
                    className="linkBtn back"
                    onClick={() => navigate("Projects")}
                  >
                    <ArrowLeft size={14} />
                    Projects
                  </button>
                )}
                <h1 tabIndex={-1} ref={heading}>
                  {screen === "Project details" ? detail?.name : titles[screen]}
                </h1>
                <p>
                  {screen === "Project details"
                    ? detail?.path
                    : subtitles[screen]}
                </p>
              </div>
              {["Overview", "Projects"].includes(screen) &&
                (scanning ? (
                  <button disabled={cancelling} onClick={() => void cancel()}>
                    <Square size={14} />
                    {cancelling ? "Cancelling…" : "Cancel scan"}
                  </button>
                ) : (
                  <button
                    className="primary"
                    disabled={busy}
                    onClick={() => void scan()}
                  >
                    <ScanLine size={17} />
                    Scan folders
                  </button>
                ))}
            </div>
            {error && (
              <div className="alert" role="alert">
                <CircleAlert size={16} />
                <span>{error}</span>
                <button aria-label="Dismiss error" onClick={() => setError("")}>
                  <X size={14} />
                </button>
              </div>
            )}
            {scanning && (
              <div className="scanStatus" role="status">
                <Activity size={16} />
                Scanning workspace{" "}
                <span>{visited.toLocaleString()} directories visited</span>
              </div>
            )}
            {report?.cancelled && !scanning && (
              <div className="alert" role="status">
                <CircleAlert size={16} />
                Scan cancelled. Run a complete scan before selecting candidates.
              </div>
            )}
            {screen === "Overview" && (
              <Overview
                report={report}
                roots={settings.roots.length}
                open={open}
                explore={() => navigate("Projects")}
                lastScan={lastScan}
              />
            )}
            {screen === "Projects" && (
              <Explorer projects={report?.projects || []} open={open} />
            )}
            {screen === "Project details" && detail && (
              <ProjectDetails
                p={detail}
                protectedNow={
                  detail.protected ||
                  settings.protectedProjects.includes(detail.path)
                }
                protect={() =>
                  void update(
                    "set_project_protection",
                    {
                      path: detail.path,
                      protected: !settings.protectedProjects.includes(
                        detail.path,
                      ),
                    },
                    true,
                  )
                }
                locked={busy || !!report?.cancelled}
                selected={selected}
                toggle={toggle}
                review={() => void review()}
              />
            )}
            {screen === "Cleanup review" && (
              <section className="panel">
                <header className="sectionHead">
                  <div>
                    <ClipboardCheck />
                    Dry-run plan
                  </div>
                  <span className="pill">
                    <LockKeyhole size={12} />
                    Preview only
                  </span>
                </header>
                {saving ? (
                  <div role="status" className="prefHint">
                    Preparing preview…
                  </div>
                ) : plan?.candidates.length ? (
                  <>
                    <div className="planTotal">
                      <strong>{bytes(plan.logical_bytes)}</strong>
                      <span>
                        logical size · {plan.candidates.length} selected
                      </span>
                    </div>
                    {plan.candidates.map((c) => (
                      <article className="candidate" key={c.path}>
                        <div className="candidateTitle">
                          <div className="rowIdentity">
                            <strong>{basename(c.path)}</strong>
                            <small>{c.path}</small>
                          </div>
                          <b className="number">{bytes(c.logical_bytes)}</b>
                        </div>
                        <RiskLabel risk={c.classification} />
                        <p>{c.reason}</p>
                        <p>
                          <b>Rebuild</b> {c.rebuild}
                        </p>
                      </article>
                    ))}
                    <p className="prefHint">
                      Dry run complete. All files remain at their original
                      locations. Verified filesystem cleanup is unavailable in
                      this build.
                    </p>
                  </>
                ) : (
                  <Empty
                    Icon={ClipboardCheck}
                    title={
                      selected.size
                        ? "No eligible candidates"
                        : "Your review is empty"
                    }
                  >
                    Select a candidate in a project to preview its cleanup plan.
                  </Empty>
                )}
                <footer className="reviewFoot">
                  <button
                    className="linkBtn"
                    onClick={() => navigate("Projects")}
                  >
                    Browse projects <ArrowRight size={14} />
                  </button>
                  <button disabled>
                    <LockKeyhole size={14} />
                    Clean selected
                  </button>
                </footer>
              </section>
            )}
            {screen === "History" && (
              <section className="panel">
                <header className="sectionHead">
                  <div>
                    <History />
                    Operation log
                  </div>
                </header>
                <Empty Icon={History} title="No cleanup operations">
                  This read-only build previews plans. No files have been moved
                  or removed.
                </Empty>
              </section>
            )}
            {screen === "Settings" && (
              <Preferences
                settings={settings}
                busy={busy}
                choose={() => void update("choose_root", undefined, true)}
                remove={(root) => void update("remove_root", { root }, true)}
                unprotect={(path) =>
                  void update(
                    "set_project_protection",
                    { path, protected: false },
                    true,
                  )
                }
                save={(appearance, excludedNames) =>
                  void update(
                    "set_preferences",
                    { appearance, excludedNames },
                    excludedNames !== settings.excludedNames,
                  )
                }
              />
            )}
          </div>
        </main>
        <footer className="statusbar">
          <span>
            <ShieldCheck size={13} />
            Read-only mode
          </span>
          <span>
            <Terminal size={13} />
            StackRinse 0.1.0
          </span>
        </footer>
      </div>
    </div>
  );
}
createRoot(document.getElementById("root")!).render(<App />);
