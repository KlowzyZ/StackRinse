import { useMemo, useState } from "react";
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
  let offset = 0;
  return (
    <div className="distribution">
      <div
        className="dial"
        role="img"
        aria-label={
          scanned
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
          <strong>
            {scanned
              ? selected && total
                ? Math.round((selected[1] / total) * 100) + "%"
                : entries.length
              : "—"}
          </strong>
          <span>{selected ? "of inventory" : "categories"}</span>
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
            </button>
          ))
        ) : (
          <p>Category sizes appear after a workspace scan.</p>
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
}: {
  report: Report | null;
  roots: number;
  open: (p: Project) => void;
  explore: () => void;
  lastScan: string;
}) {
  const data = useMemo(() => {
    const projects = report?.projects || [],
      groups = new Map<string, number>();
    const counts: Record<Risk, number> = {
      rebuildable: 0,
      review_required: 0,
      protected: 0,
    };
    let size = 0,
      count = 0;
    for (const p of projects)
      for (const c of p.candidates) {
        size += c.logical_bytes;
        count++;
        counts[c.classification]++;
        groups.set(c.category, (groups.get(c.category) || 0) + c.logical_bytes);
      }
    return {
      size,
      count,
      counts,
      entries: [...groups.entries()].sort((a, b) => b[1] - a[1]),
      top: [...projects]
        .sort((a, b) => b.logical_bytes - a.logical_bytes)
        .slice(0, 5),
      recent: [...projects]
        .filter((p) => p.last_activity)
        .sort((a, b) => b.last_activity! - a.last_activity!)
        .slice(0, 3),
    };
  }, [report]);
  return (
    <>
      <section className="instrument">
        <header className="sectionHead">
          <div>
            <ScanLine />
            Storage map
          </div>
          <span className="mono muted">Logical file size</span>
        </header>
        <div className="instrumentBody">
          <div className="measurement">
            <span className="eyelabel">Candidate inventory</span>
            <div className="bigNumber">
              {report ? bytes(data.size) : "—"}
              {!report && <span> bytes</span>}
            </div>
            <p>
              {report
                ? "Measured across recognized artifact directories."
                : "Scan a folder to measure its build artifacts."}
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
          <Distribution entries={data.entries} scanned={!!report} />
        </div>
        <footer className="instrumentFoot">
          <span className="signal" />
          {report
            ? report.cancelled
              ? "Partial inventory"
              : "Inventory measured"
            : "Awaiting first scan"}
          <span className="footRight">
            {roots} scan {roots === 1 ? "root" : "roots"}
          </span>
        </footer>
      </section>
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
            <Empty title="No projects discovered">
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
          </header>
          <div className="riskRows">
            {(Object.keys(data.counts) as Risk[]).map((r) => (
              <div key={r}>
                <RiskLabel risk={r} />
                <b>{report ? data.counts[r] : "—"}</b>
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
              Recent project activity
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
      {!!report?.warnings.length && (
        <details className="panel issues">
          <summary>
            <CircleHelp size={16} />
            {report.warnings.length} scan warning(s)
          </summary>
          {report.warnings.map((w, i) => (
            <p key={i}>{w}</p>
          ))}
        </details>
      )}
    </>
  );
}
