import { useMemo, useState, type ReactNode } from "react";
import { ExportControl } from "./export-control";
import {
  ArrowRight,
  Box,
  CircleHelp,
  Clock3,
  FolderOpen,
  Layers3,
  LockKeyhole,
  ScanLine,
  Package,
  Hammer,
  Database,
  ShieldCheck,
} from "lucide-react";
import {
  activity,
  bytes,
  palette,
  type Project,
  type Report,
  type Risk,
} from "./model";
import { Empty, ProjectRow, RiskLabel } from "./shared";

type Breakdown = "category" | "ecosystem";

function Distribution({
  entries,
  breakdown,
}: {
  entries: [string, number][];
  breakdown: Breakdown;
}) {
  const [active, setActive] = useState<string | null>(null);
  const total = entries.reduce((n, e) => n + e[1], 0);
  const selected = entries.find((e) => e[0] === active);
  const share = selected && total > 0 ? (selected[1] / total) * 100 : null;
  const shareLabel =
    share === null
      ? null
      : share > 0 && share < 1
        ? "<1%"
        : Math.round(share) + "%";
  const dialValue = selected
    ? (shareLabel ?? bytes(selected[1]))
    : entries.length;
  const dialUnit = !selected
    ? entries.length === 1
      ? breakdown
      : breakdown === "category"
        ? "categories"
        : "ecosystems"
    : share === null
      ? "observed"
      : "of inventory";
  let offset = 0;
  return (
    <div className="distribution">
      <div
        className="dial"
        role="img"
        aria-label={
          selected
            ? selected[0] +
              ", " +
              bytes(selected[1]) +
              (shareLabel === null
                ? ""
                : ", " +
                  (shareLabel === "<1%" ? "less than 1%" : shareLabel) +
                  " of inventory")
            : "Artifact " + breakdown + " distribution in logical bytes"
        }
      >
        <svg viewBox="0 0 200 200" aria-hidden="true">
          <circle cx="100" cy="100" r="77" className="ringBase" />
          {entries.map(([name, n], i) => {
            const length = total ? (n / total) * 100 : 0;
            const start = offset;
            offset += length;
            return (
              <circle
                key={name}
                cx="100"
                cy="100"
                r="77"
                pathLength="100"
                fill="none"
                stroke={palette[i % palette.length]}
                strokeWidth={active === name ? 15 : 12}
                opacity={!selected || name === active ? 1 : 0.3}
                strokeDasharray={length + " " + (100 - length)}
                strokeDashoffset={-start}
                transform="rotate(-90 100 100)"
              />
            );
          })}
        </svg>
        <div className="dialCenter">
          <Layers3 size={20} />
          <strong>{dialValue}</strong>
          <span>{dialUnit}</span>
        </div>
      </div>
      <div className="legend">
        <h3>
          {breakdown === "category"
            ? "Artifact breakdown"
            : "Ecosystem breakdown"}
        </h3>
        {entries.length ? (
          entries.map(([name, n], i) => (
            <button
              key={name}
              aria-pressed={active === name}
              onClick={() => setActive(active === name ? null : name)}
            >
              <i style={{ background: palette[i % palette.length] }} />
              <span>{name}</span>
              <b>{bytes(n)}</b>
              <span className="categoryTrack" aria-hidden="true">
                <span
                  style={{
                    width: (total ? (100 * n) / total : 0) + "%",
                    background: palette[i % palette.length],
                  }}
                />
              </span>
            </button>
          ))
        ) : (
          <p>No artifacts measured.</p>
        )}
      </div>
    </div>
  );
}
export function Overview({
  report,
  roots,
  scanning,
  scanAction,
  open,
  explore,
  lastScan,
  busy,
  savedScan,
}: {
  report: Report | null;
  roots: number;
  scanning: boolean;
  scanAction: ReactNode;
  open: (p: Project) => void;
  explore: () => void;
  lastScan: string;
  busy: boolean;
  savedScan: ReactNode;
}) {
  const [breakdown, setBreakdown] = useState<Breakdown>("category");
  const data = useMemo(() => {
    const projects = report?.projects || [];
    const groups = {
      category: new Map<string, number>(),
      ecosystem: new Map<string, number>(),
    };
    const counts: Record<Risk, number> = {
      rebuildable: 0,
      review_required: 0,
      protected: 0,
    };
    const sizes = { ...counts };
    let count = 0,
      partialSize = false;
    for (const p of projects)
      for (const c of p.candidates) {
        count++;
        partialSize ||= !c.size_complete;
        counts[c.classification]++;
        sizes[c.classification] += c.logical_bytes;
        for (const kind of ["category", "ecosystem"] as const) {
          const group = groups[kind];
          group.set(c[kind], (group.get(c[kind]) || 0) + c.logical_bytes);
        }
      }
    return {
      count,
      counts,
      partialSize,
      sizes,
      entries: {
        category: [...groups.category].sort((a, b) => b[1] - a[1]),
        ecosystem: [...groups.ecosystem].sort((a, b) => b[1] - a[1]),
      },
      top: [...projects]
        .sort((a, b) => b.logical_bytes - a.logical_bytes)
        .slice(0, 5),
      recent: [...projects]
        .filter((p) => p.last_activity !== null)
        .sort((a, b) => b.last_activity! - a.last_activity!)
        .slice(0, 3),
    };
  }, [report]);
  const partial =
    data.partialSize || !!report?.cancelled || !!report?.warnings.length;
  if (!report) {
    return (
      <>
        <section className="panel scanIntro" aria-labelledby="scan-intro-title">
          <div className="introMain">
            <div className="introIcon">
              <FolderOpen size={25} aria-hidden="true" />
            </div>
            <h2 id="scan-intro-title">
              {scanning
                ? "Discovering your projects"
                : "Clean builds. Keep code."}
            </h2>
            <p>
              {scanning
                ? "Your inventory will appear here when the scan finishes."
                : "Choose a workspace folder to see where dependencies, build outputs, and development caches are taking up space."}
            </p>
            {!scanning && scanAction}
            <span className="introStatus">
              {scanning ? "Scan in progress" : "Awaiting first scan"}
            </span>
          </div>
          <div className="introScope">
            <h3>What a scan looks for</h3>
            <ul>
              <li>
                <Package size={17} aria-hidden="true" />
                <span>Dependencies</span>
                <code>node_modules</code>
              </li>
              <li>
                <Hammer size={17} aria-hidden="true" />
                <span>Build outputs</span>
                <code>target</code>
              </li>
              <li>
                <Database size={17} aria-hidden="true" />
                <span>Development caches</span>
                <code>.godot</code>
              </li>
            </ul>
            <p>
              <ShieldCheck size={17} aria-hidden="true" />
              <span>
                Scans run locally in the folders you choose. Cleanup review is a
                read-only preview.
              </span>
            </p>
          </div>
        </section>
        {savedScan}
      </>
    );
  }
  const { logical_bytes, reviewable_bytes, protected_bytes } = report.totals;
  const other = Math.max(0, logical_bytes - reviewable_bytes - protected_bytes);
  return (
    <>
      <section className="instrument">
        <header className="sectionHead">
          <div>
            <ScanLine />
            Current session inventory
          </div>
          <div className="breakdownControls">
            <select
              aria-label="Group storage map"
              value={breakdown}
              onChange={(event) =>
                setBreakdown(
                  event.target.value === "ecosystem" ? "ecosystem" : "category",
                )
              }
            >
              <option value="category">By artifact type</option>
              <option value="ecosystem">By ecosystem</option>
            </select>
          </div>
        </header>
        <div className="instrumentBody">
          <div className="measurement">
            <span className="eyelabel">Observed logical size</span>
            <div className="bigNumber">{bytes(logical_bytes)}</div>
            {logical_bytes > 0 && (
              <div className="scopeTrack" aria-hidden="true">
                <span style={{ flexGrow: reviewable_bytes }} />
                <span style={{ flexGrow: protected_bytes }} />
                <span style={{ flexGrow: other }} />
              </div>
            )}
            <dl className="scopeTotals">
              <div>
                <dt>Reviewable</dt>
                <dd>{bytes(reviewable_bytes)}</dd>
              </div>
              <div>
                <dt>Protected</dt>
                <dd>{bytes(protected_bytes)}</dd>
              </div>
              {other > 0 && (
                <div>
                  <dt>Outside review</dt>
                  <dd>{bytes(other)}</dd>
                </div>
              )}
            </dl>
            <p className="logicalNote">
              {partial ? "Partial inventory. " : ""}File lengths, separate from
              disk usage.
            </p>
            <dl className="metrics">
              {[
                [FolderOpen, "Projects", report?.projects.length],
                [Box, "Candidates", report ? data.count : undefined],
                [Clock3, "Last scan", lastScan || undefined],
              ].map(([icon, label, value]) => {
                const Icon = icon as typeof Box;
                return (
                  <div key={String(label)}>
                    <dt>
                      <Icon size={14} />
                      {String(label)}
                    </dt>
                    <dd>{value === undefined ? "—" : String(value)}</dd>
                  </div>
                );
              })}
            </dl>
          </div>
          <Distribution
            key={breakdown}
            entries={data.entries[breakdown]}
            breakdown={breakdown}
          />
        </div>
        <footer className="instrumentFoot">
          <span
            className={
              "signal " + (partial ? "partial" : report ? "measured" : "")
            }
          />
          <span>{partial ? "Partial inventory" : "Inventory measured"}</span>
          <span className="footRight">
            {roots} scan {roots === 1 ? "root" : "roots"}
          </span>
        </footer>
        {!report.cancelled && (
          <ExportControl
            key={report.completedAtMs}
            source={{ kind: "fresh" }}
            disabled={busy}
          />
        )}
      </section>
      {!!report.warnings.length && (
        <details className="panel issues">
          <summary>
            <CircleHelp size={16} />
            {report.warnings.length} scan{" "}
            {report.warnings.length === 1 ? "warning" : "warnings"}
          </summary>
          {report.warnings.map((w, i) => (
            <p key={i}>{w}</p>
          ))}
        </details>
      )}
      <div className="overviewGrid">
        <section className="panel">
          <header className="sectionHead">
            <div>
              <FolderOpen />
              Largest projects
            </div>
            <button className="linkBtn" onClick={explore}>
              Explore <ArrowRight size={14} />
            </button>
          </header>
          {data.top.length ? (
            data.top.map((p) => <ProjectRow key={p.path} p={p} open={open} />)
          ) : (
            <Empty title={report ? "No projects discovered" : "Ready to scan"}>
              {report
                ? "Try another workspace folder."
                : "Choose a scan root to discover your projects."}
            </Empty>
          )}
        </section>
        <section className="panel">
          <header className="sectionHead">
            <div>
              <ShieldCheck />
              Safety summary
            </div>
            <span className="eyelabel">Count · Logical size</span>
          </header>
          <div className="riskRows">
            {(Object.keys(data.counts) as Risk[]).map((r) => (
              <div key={r}>
                <RiskLabel risk={r} />
                <b>
                  {report ? `${data.counts[r]} · ${bytes(data.sizes[r])}` : "—"}
                </b>
              </div>
            ))}
          </div>
          <div className="safetyFoot">
            <LockKeyhole size={14} />
            Read-only discovery
          </div>
        </section>
      </div>
      {savedScan}
      {data.recent.length > 0 && (
        <section className="panel">
          <header className="sectionHead">
            <div>
              <Clock3 />
              Recent manifest changes
            </div>
            <span className="muted">Manifest modified</span>
          </header>
          {data.recent.map((p) => (
            <button
              key={p.path}
              className="activityRow"
              onClick={() => open(p)}
            >
              <span>{p.name}</span>
              <span className="number">{activity(p)}</span>
            </button>
          ))}
        </section>
      )}
    </>
  );
}
