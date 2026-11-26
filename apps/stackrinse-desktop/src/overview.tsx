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

function Distribution({
  entries,
  scanned,
}: {
  entries: [string, number][];
  scanned: boolean;
}) {
  const [active, setActive] = useState<string | null>(null);
  const total = entries.reduce((n, e) => n + e[1], 0);
  const selected = entries.find((e) => e[0] === active);
  const share =
    selected && total > 0 ? Math.round((selected[1] / total) * 100) : null;
  const dialValue = !scanned
    ? "—"
    : !selected
      ? entries.length
      : share === null
        ? bytes(selected[1])
        : share + "%";
  const dialUnit = !selected
    ? entries.length === 1
      ? "category"
      : "categories"
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
              (share === null ? "" : ", " + share + "% of inventory")
            : scanned
              ? "Artifact category distribution in logical bytes"
              : "Storage map awaiting scan"
        }
      >
        <svg viewBox="0 0 200 200" aria-hidden="true">
          {Array.from({ length: 72 }, (_, i) => (
            <line
              key={i}
              x1="100"
              y1={i % 6 ? 9 : 6}
              x2="100"
              y2="13"
              transform={"rotate(" + i * 5 + " 100 100)"}
              className={i % 6 ? "minorTick" : "majorTick"}
            />
          ))}
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
          <circle cx="100" cy="100" r="61" className="guide" />
        </svg>
        <div className="dialCenter">
          <Layers3 size={20} />
          <strong>{dialValue}</strong>
          <span>{dialUnit}</span>
        </div>
      </div>
      <div className="legend">
        <h3>Artifact breakdown</h3>
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
          <p>
            {scanned
              ? "No artifact categories measured."
              : "Category sizes appear after a workspace scan."}
          </p>
        )}
      </div>
    </div>
  );
}
export function Overview({
  report,
  roots,
  open,
  explore,
  lastScan,
  busy,
  savedScan,
}: {
  report: Report | null;
  roots: number;
  open: (p: Project) => void;
  explore: () => void;
  lastScan: string;
  busy: boolean;
  savedScan: ReactNode;
}) {
  const data = useMemo(() => {
    const projects = report?.projects || [],
      groups = new Map<string, number>();
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
        groups.set(c.category, (groups.get(c.category) || 0) + c.logical_bytes);
      }
    return {
      count,
      counts,
      partialSize,
      sizes,
      entries: [...groups.entries()].sort((a, b) => b[1] - a[1]),
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
  return (
    <>
      <section className="instrument">
        <header className="sectionHead">
          <div>
            <ScanLine />
            {report ? "Current session inventory" : "Storage map"}
          </div>
          <span className="mono muted">Logical file size</span>
        </header>
        <div className="instrumentBody">
          <div className="measurement">
            <span className="eyelabel">Candidate inventory</span>
            <div className="bigNumber">
              {report ? bytes(report.totals.logical_bytes) : "—"}
              {!report && <span> bytes</span>}
            </div>
            {report ? (
              <>
                <dl className="scopeTotals">
                  <div>
                    <dt>Reviewable</dt>
                    <dd>{bytes(report.totals.reviewable_bytes)}</dd>
                  </div>
                  <div>
                    <dt>Protected</dt>
                    <dd>{bytes(report.totals.protected_bytes)}</dd>
                  </div>
                </dl>
                {partial && (
                  <p>Observed logical bytes from a partial inventory.</p>
                )}
              </>
            ) : (
              <p>Scan a folder to measure its build artifacts.</p>
            )}
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
          <Distribution entries={data.entries} scanned={!!report} />
        </div>
        <footer className="instrumentFoot">
          <span
            className={
              "signal " + (partial ? "partial" : report ? "measured" : "")
            }
          />
          {report
            ? partial
              ? "Partial inventory"
              : "Inventory measured"
            : "Awaiting first scan"}
          <span className="footRight">
            {roots} scan {roots === 1 ? "root" : "roots"}
          </span>
        </footer>
        {report && !report.cancelled && <ExportControl key={report.completedAtMs} source={{ kind: "fresh" }} disabled={busy} />}
      </section>
      {savedScan}
      {!!report?.warnings.length && (
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
