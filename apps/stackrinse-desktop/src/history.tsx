import { invoke } from "@tauri-apps/api/core";
import {
  ArrowLeft,
  ArrowRight,
  CircleAlert,
  Clock3,
  FolderOpen,
  GitCompare,
  History as HistoryIcon,
  Settings2,
  Trash2,
  TrendingUp,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { ExportControl } from "./export-control";
import {
  compatibleScans,
  formatChartDate,
  formatScanDate,
  signedBytes,
  type Comparison,
  type HistoryDetail,
  type HistoryEntry,
  type HistoryIndex,
  type ProjectChanges,
} from "./history-api";
import { basename, bytes } from "./model";
import { Empty } from "./shared";

const PAGE_SIZE = 7;

function Completeness({ entry }: { entry: HistoryEntry }) {
  return (
    <span className={"historyStatus " + (entry.complete ? "" : "partial")}>
      <span aria-hidden="true" />
      {entry.complete ? "Complete" : "Incomplete"}
    </span>
  );
}

export function SavedOverview({
  entries,
  open,
}: {
  entries: HistoryEntry[];
  open: () => void;
}) {
  const latest = entries[0];
  if (!latest) return null;
  const before = compatibleScans(entries, latest)[0];
  return (
    <section className="panel savedOverview" aria-label="Latest saved scan">
      <Clock3 size={18} aria-hidden="true" />
      <div>
        <h2>Latest saved scan</h2>
        <p>
          {formatScanDate(latest.completed_at_ms)} ·{" "}
          {latest.complete ? "Complete" : "Incomplete"}
        </p>
      </div>
      <div className="savedOverviewAmount">
        <strong>{bytes(latest.totals.logical_bytes)}</strong>
        <span>
          {before
            ? signedBytes(
                latest.totals.logical_bytes - before.totals.logical_bytes,
              ) + " logical change since prior comparable scan"
            : "Observed logical size"}
        </span>
      </div>
      <button className="linkBtn" onClick={open}>
        View history <ArrowRight size={14} />
      </button>
    </section>
  );
}

function StorageTrend({
  selected,
  entries,
}: {
  selected: HistoryEntry;
  entries: HistoryEntry[];
}) {
  const sameScope = selected.complete
    ? entries.filter(
        (entry) => entry.complete && entry.scope_key === selected.scope_key,
      )
    : [];
  // Keep the selected observation visible, even when inspecting older history.
  const selectedOffset = sameScope.findIndex(
    (entry) => entry.id === selected.id,
  );
  const start = Math.max(0, selectedOffset - 5);
  const samples = sameScope.slice(start, start + 12).reverse();
  const first = samples[0],
    last = samples.at(-1);
  const max = Math.max(
    1,
    ...samples.map((entry) => entry.totals.logical_bytes),
  );
  const scale = max <= 2 ? 2 : Math.ceil(max / 2) * 2;
  const points = samples.map((entry, i) => ({
    x: 7 + (586 * i) / Math.max(1, samples.length - 1),
    y: 111 - (104 * entry.totals.logical_bytes) / scale,
    entry,
  }));
  const line = points.map(({ x, y }) => x + "," + y).join(" ");
  const root = basename(selected.root_hint);
  return (
    <section className="panel historyTrend" aria-labelledby="trend-title">
      <header className="sectionHead">
        <div>
          <TrendingUp />
          <h2 id="trend-title">Storage trend</h2>
        </div>
        <span className="mono muted">Observed logical size</span>
      </header>
      {first && last && samples.length > 1 ? (
        <div className="trendBody">
          <div className="trendReadout">
            <span className="eyelabel">Selected scan</span>
            <strong>{bytes(selected.totals.logical_bytes)}</strong>
            <span className="trendScope" title={selected.root_hint}>
              <FolderOpen size={14} /> {root}
              {selected.root_count > 1 ? " +" + (selected.root_count - 1) : ""}
            </span>
            <p>
              {samples.length} comparable scans ·{" "}
              {signedBytes(
                last.totals.logical_bytes - first.totals.logical_bytes,
              )}{" "}
              across this range
            </p>
          </div>
          <figure className="trendFigure">
            <div className="trendPlot" aria-hidden="true">
              <div className="trendScale">
                <span>{bytes(scale)}</span>
                <span>{bytes(scale / 2)}</span>
                <span>0 B</span>
              </div>
              <svg viewBox="0 0 600 118" preserveAspectRatio="none">
                <path className="trendGrid" d="M0 7H600 M0 59H600 M0 111H600" />
                <polygon
                  className="trendFill"
                  points={"7,111 " + line + " 593,111"}
                />
                <polyline className="trendLine" points={line} />
                {points.map(({ x, y, entry }) => (
                  <ellipse
                    key={entry.id}
                    cx={x}
                    cy={y}
                    rx={entry.id === selected.id ? 4 : 2.5}
                    ry={entry.id === selected.id ? 4 : 2.5}
                    className={
                      entry.id === selected.id ? "trendSelected" : "trendPoint"
                    }
                  />
                ))}
              </svg>
            </div>
            <figcaption>
              <span aria-hidden="true">
                {formatChartDate(first.completed_at_ms)}
              </span>
              <details>
                <summary>Chart values</summary>
                <table className="trendValues">
                  <thead>
                    <tr>
                      <th scope="col">Scan</th>
                      <th scope="col">Logical size</th>
                    </tr>
                  </thead>
                  <tbody>
                    {samples.map((entry) => (
                      <tr key={entry.id}>
                        <td>
                          {formatScanDate(entry.completed_at_ms)}
                          {entry.id === selected.id && " (selected)"}
                        </td>
                        <td>{bytes(entry.totals.logical_bytes)}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </details>
              <span aria-hidden="true">
                {formatChartDate(last.completed_at_ms)}
              </span>
            </figcaption>
          </figure>
        </div>
      ) : (
        <p className="historyHint">
          {selected.complete
            ? "Run another complete scan with the same folders, exclusions and protection settings to see a trend."
            : "This inventory is incomplete. Select a complete scan to see its trend."}
        </p>
      )}
    </section>
  );
}

function BreakdownTable({
  label,
  values,
}: {
  label: string;
  values: Comparison["categories"];
}) {
  return (
    <div className="historyBreakdown">
      <h3>{label}</h3>
      {values.length ? (
        <table>
          <thead>
            <tr>
              <th scope="col">Name</th>
              <th scope="col">Baseline</th>
              <th scope="col">Selected</th>
              <th scope="col">Change</th>
            </tr>
          </thead>
          <tbody>
            {values.map(({ name, before, after }) => (
              <tr key={name}>
                <th scope="row">{name}</th>
                <td>{bytes(before)}</td>
                <td>{bytes(after)}</td>
                <td>{signedBytes(after - before)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      ) : (
        <p className="muted">No bytes observed.</p>
      )}
    </div>
  );
}

function ChangedProjects({
  title,
  data,
}: {
  title: string;
  data: ProjectChanges;
}) {
  return (
    <section className="changedProjects" aria-label={title}>
      <h3>
        {title} <span>{data.count.toLocaleString()}</span>
      </h3>
      {data.projects.length ? (
        <ul>
          {data.projects.map(({ path, before, after }) => (
            <li key={path}>
              <span title={path}>
                <strong>{basename(path)}</strong>
                <small>{path}</small>
              </span>
              <b className="number">
                {before === null || after === null
                  ? bytes(after ?? before ?? 0)
                  : signedBytes(after - before)}
              </b>
            </li>
          ))}
        </ul>
      ) : (
        <p className="muted">None observed.</p>
      )}
      {data.count > data.projects.length && (
        <p className="muted">
          Showing the {data.projects.length} largest observations.
        </p>
      )}
    </section>
  );
}

function ScanComparison({ after, before }: { after: number; before: number }) {
  const [result, setResult] = useState<Comparison | null>(null);
  const [error, setError] = useState("");
  useEffect(() => {
    let live = true;
    invoke<Comparison>("history_compare", { beforeId: before, afterId: after })
      .then((comparison) => {
        if (live) setResult(comparison);
      })
      .catch((error) => {
        if (live) setError(String(error));
      });
    return () => {
      live = false;
    };
  }, [after, before]);
  if (error)
    return (
      <p className="historyHint historyError" role="alert">
        {error}
      </p>
    );
  if (!result)
    return (
      <p className="historyHint" role="status">
        Comparing saved scans…
      </p>
    );
  return (
    <>
      <div className="comparisonTotal">
        <div>
          <span className="eyelabel">Logical size change</span>
          <strong>{signedBytes(result.logical_bytes_delta)}</strong>
        </div>
        <p>
          Differences describe observed artifacts. Filesystem allocation can
          differ.
        </p>
      </div>
      {result.growth && result.discovered && result.no_longer_observed ? (
        <details className="historyDisclosure" open>
          <summary>Project changes</summary>
          <div className="changesGrid">
            <ChangedProjects title="Growing projects" data={result.growth} />
            <ChangedProjects title="Newly observed" data={result.discovered} />
            <ChangedProjects
              title="No longer observed"
              data={result.no_longer_observed}
            />
          </div>
          <p className="muted">
            Project paths identify observations. Moves and renames can appear as
            two changes.
          </p>
        </details>
      ) : (
        <p className="historyHint">
          Project comparisons are unavailable because a scan exceeded the
          project detail limit.
        </p>
      )}
      <details className="historyDisclosure">
        <summary>Ecosystem and artifact changes</summary>
        <BreakdownTable
          label="By artifact ecosystem"
          values={result.ecosystems}
        />
        <BreakdownTable
          label="By artifact category"
          values={result.categories}
        />
      </details>
    </>
  );
}

function ComparePanel({
  entry,
  entries,
}: {
  entry: HistoryEntry;
  entries: HistoryEntry[];
}) {
  const options = compatibleScans(entries, entry);
  const [requestedBaseline, setRequestedBaseline] = useState<number | null>(
    null,
  );
  const baseline =
    options.find((option) => option.id === requestedBaseline) ?? options[0];
  return (
    <section
      className="panel historyCompare"
      aria-labelledby="comparison-title"
    >
      <header className="sectionHead">
        <div>
          <GitCompare />
          <h2 id="comparison-title">Compare scans</h2>
        </div>
      </header>
      {baseline ? (
        <>
          <label className="historyBaseline">
            <span>Compare with</span>
            <select
              value={baseline.id}
              onChange={(e) => setRequestedBaseline(Number(e.target.value))}
            >
              {options.map((option) => (
                <option key={option.id} value={option.id}>
                  {formatScanDate(option.completed_at_ms)} ·{" "}
                  {bytes(option.totals.logical_bytes)}
                </option>
              ))}
            </select>
          </label>
          <ScanComparison
            key={baseline.id}
            before={baseline.id}
            after={entry.id}
          />
        </>
      ) : (
        <p className="historyHint">
          {entry.complete
            ? "No earlier complete scan used these folders, exclusions, protection settings and inventory rules."
            : "Incomplete inventories are excluded from comparisons."}
        </p>
      )}
    </section>
  );
}

function DetailPanel({
  entry,
  busy,
  remove,
}: {
  entry: HistoryEntry;
  busy: boolean;
  remove: (trigger: HTMLButtonElement) => void;
}) {
  const [detail, setDetail] = useState<HistoryDetail | null>(null);
  const [error, setError] = useState("");
  useEffect(() => {
    let live = true;
    invoke<HistoryDetail>("history_detail", { id: entry.id })
      .then((detail) => {
        if (live) setDetail(detail);
      })
      .catch((error) => {
        if (live) setError(String(error));
      });
    return () => {
      live = false;
    };
  }, [entry.id]);
  return (
    <section
      className="panel historyDetail"
      aria-labelledby="scan-detail-title"
    >
      <header className="sectionHead">
        <div>
          <Clock3 />
          <h2 id="scan-detail-title">Scan details</h2>
        </div>
        <Completeness entry={entry} />
      </header>
      <div className="snapshotBody">
        <div className="snapshotTitle">
          <div>
            <p className="eyelabel">Saved scan</p>
            <h3>{formatScanDate(entry.completed_at_ms)}</h3>
          </div>
          <span className="mono muted">
            {entry.duration_ms < 1000
              ? entry.duration_ms.toLocaleString() + " ms"
              : (entry.duration_ms / 1000).toLocaleString(undefined, {
                  maximumFractionDigits: 1,
                }) + " s"}
          </span>
        </div>
        <dl className="snapshotMetrics">
          {[
            ["Logical size", bytes(entry.totals.logical_bytes)],
            ["Reviewable", bytes(entry.totals.reviewable_bytes)],
            ["Protected", bytes(entry.totals.protected_bytes)],
            ["Projects", entry.project_count.toLocaleString()],
            ["Candidates", entry.candidate_count.toLocaleString()],
            ["Warnings", entry.warning_count.toLocaleString()],
          ].map(([label, value]) => (
            <div key={label}>
              <dt>{label}</dt>
              <dd>{value}</dd>
            </div>
          ))}
        </dl>
        {!entry.complete && (
          <p className="incompleteNote">
            <CircleAlert size={15} />
            This snapshot includes partial observations.
            {entry.incomplete_candidate_count > 0 &&
              " Candidates with incomplete sizes: " +
                entry.incomplete_candidate_count.toLocaleString() +
                "."}
          </p>
        )}
      </div>
      {error ? (
        <p className="historyHint historyError" role="alert">
          {error}
        </p>
      ) : detail ? (
        <>
          <details className="historyDisclosure" open={!entry.complete}>
            <summary>Scope and warnings</summary>
            <h3>Scanned roots</h3>
            <ul className="scopePaths">
              {detail.roots.map((path) => (
                <li key={path}>{path}</li>
              ))}
            </ul>
            <p className="muted">
              {entry.directories.toLocaleString()} directories visited
            </p>
            {!!detail.exclusions.length && (
              <p className="scopePaths">
                Exclusions: {detail.exclusions.join(", ")}
              </p>
            )}
            {!!detail.warnings.length && (
              <ul className="scopePaths warningPaths">
                {detail.warnings.map((warning, i) => (
                  <li key={i}>{warning}</li>
                ))}
              </ul>
            )}
          </details>
          <details className="historyDisclosure">
            <summary>Largest storage consumers</summary>
            {detail.largest_projects.length ? (
              <ul className="largestSaved">
                {detail.largest_projects.map((project) => (
                  <li key={project.path}>
                    <span title={project.path}>
                      <strong>{basename(project.path)}</strong>
                      <small>{project.path}</small>
                    </span>
                    <b className="number">{bytes(project.logical_bytes)}</b>
                  </li>
                ))}
              </ul>
            ) : (
              <p className="muted">
                {detail.project_details_available
                  ? "No project artifact bytes observed."
                  : "This scan exceeded the project detail limit."}
              </p>
            )}
          </details>
          <details className="historyDisclosure">
            <summary>Storage breakdown</summary>
            {(
              [
                ["Artifact ecosystems", detail.ecosystems],
                ["Artifact categories", detail.categories],
              ] as const
            ).map(([label, values]) => (
              <div className="historyBreakdown" key={label}>
                <h3>{label}</h3>
                {Object.keys(values).length ? (
                  <table aria-label={label}>
                    <thead>
                      <tr>
                        <th scope="col">Name</th>
                        <th scope="col">Logical size</th>
                      </tr>
                    </thead>
                    <tbody>
                      {Object.entries(values).map(([name, value]) => (
                        <tr key={name}>
                          <th scope="row">{name}</th>
                          <td>{bytes(value)}</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                ) : (
                  <p className="muted">No bytes observed.</p>
                )}
              </div>
            ))}
          </details>
          <ExportControl
            source={{ kind: "history", id: entry.id }}
            disabled={busy}
          />
        </>
      ) : (
        <p className="historyHint" role="status">
          Loading scan details…
        </p>
      )}
      <footer className="snapshotFooter">
        <button
          className="linkBtn historyDanger"
          disabled={busy}
          onClick={(event) => remove(event.currentTarget)}
        >
          <Trash2 size={14} /> Remove saved scan
        </button>
      </footer>
    </section>
  );
}

type Removal = { kind: "clear" } | { kind: "remove"; id: number };

function ConfirmRemoval({
  target,
  busy,
  cancel,
  confirm,
}: {
  target: Removal;
  busy: boolean;
  cancel: () => void;
  confirm: () => void;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const cancelButton = useRef<HTMLButtonElement>(null);
  const clear = target.kind === "clear";
  useEffect(() => {
    const node = dialog.current;
    node?.showModal();
    cancelButton.current?.focus();
    return () => node?.close();
  }, []);
  return (
    <dialog
      ref={dialog}
      className="historyDialog"
      aria-labelledby="history-confirm-title"
      aria-describedby="history-confirm-note"
      onCancel={(e) => {
        e.preventDefault();
        if (!busy) cancel();
      }}
    >
      <h2 id="history-confirm-title">
        {clear ? "Clear scan history?" : "Remove this saved scan?"}
      </h2>
      <p id="history-confirm-note">
        {clear
          ? "All saved scans will be removed from this device."
          : "This observation will be removed from the timeline and comparisons."}{" "}
        Workspace files are unaffected.
      </p>
      <div className="historyActions">
        <button ref={cancelButton} disabled={busy} onClick={cancel}>
          Cancel
        </button>
        <button className="historyDanger" disabled={busy} onClick={confirm}>
          {clear ? "Clear history" : "Remove scan"}
        </button>
      </div>
    </dialog>
  );
}

function Retention({
  current,
  disabled,
  save,
  clear,
  hasEntries,
}: {
  current: number;
  disabled: boolean;
  save: (count: number) => void;
  clear: (trigger: HTMLButtonElement) => void;
  hasEntries: boolean;
}) {
  const [requestedLimit, setRequestedLimit] = useState<number | null>(null);
  const limit = requestedLimit ?? current;
  const options = [...new Set([10, 25, 50, 100, current])].sort(
    (a, b) => a - b,
  );
  return (
    <details className="panel historyManage">
      <summary>
        <Settings2 size={15} /> History settings
      </summary>
      <div>
        <label className="retentionLabel">
          Keep latest
          <select
            value={limit}
            disabled={disabled}
            onChange={(event) => setRequestedLimit(Number(event.target.value))}
          >
            {options.map((value) => (
              <option key={value} value={value}>
                {value} scans
              </option>
            ))}
          </select>
        </label>
        <p>
          Oldest scans are removed first when the count or 24 MiB history budget
          is reached.
        </p>
        <div className="historyActions">
          <button
            disabled={disabled || limit === current}
            onClick={() => save(limit)}
          >
            Apply limit
          </button>
          <button
            className="historyDanger"
            disabled={disabled || !hasEntries}
            onClick={(event) => clear(event.currentTarget)}
          >
            Clear history…
          </button>
        </div>
      </div>
    </details>
  );
}

export function History({
  index,
  error,
  native,
  busy,
  reload,
  openOverview,
}: {
  index: HistoryIndex | null;
  error: string;
  native: boolean;
  busy: boolean;
  reload: () => Promise<void>;
  openOverview: () => void;
}) {
  const [query, setQuery] = useState("");
  const [status, setStatus] = useState("all");
  const [page, setPage] = useState(0);
  const [requestedId, setRequestedId] = useState<number | null>(null);
  const [confirmation, setConfirmation] = useState<Removal | null>(null);
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState("");
  const [updateError, setUpdateError] = useState("");
  const doingUpdate = useRef(false);
  const timelineTitle = useRef<HTMLHeadingElement>(null);
  const confirmationTrigger = useRef<HTMLButtonElement>(null);
  const focusAfterDialog = useRef<"trigger" | "timeline" | null>(null);
  useEffect(() => {
    if (confirmation || !focusAfterDialog.current) return;
    const trigger =
      focusAfterDialog.current === "trigger"
        ? confirmationTrigger.current
        : null;
    focusAfterDialog.current = null;
    (trigger?.isConnected ? trigger : timelineTitle.current)?.focus();
  }, [confirmation]);
  const entries = index?.entries;
  const filtered = useMemo(() => {
    const term = query.trim().toLocaleLowerCase();
    return (
      entries?.filter(
        (entry) =>
          (status === "all" || entry.complete === (status === "complete")) &&
          (entry.root_hint + " " + formatScanDate(entry.completed_at_ms))
            .toLocaleLowerCase()
            .includes(term),
      ) ?? []
    );
  }, [entries, query, status]);
  const pages = Math.max(1, Math.ceil(filtered.length / PAGE_SIZE));
  const pageIndex = Math.min(page, pages - 1);
  const visible = filtered.slice(
    pageIndex * PAGE_SIZE,
    (pageIndex + 1) * PAGE_SIZE,
  );
  const selected =
    filtered.find((entry) => entry.id === requestedId) ?? visible[0];
  const disabled = busy || saving;
  function requestRemoval(target: Removal, trigger: HTMLButtonElement) {
    confirmationTrigger.current = trigger;
    setConfirmation(target);
  }
  function cancelRemoval() {
    focusAfterDialog.current = "trigger";
    setConfirmation(null);
  }
  async function change(
    command: string,
    args: Record<string, unknown>,
    success: string,
  ) {
    if (doingUpdate.current || busy) return;
    doingUpdate.current = true;
    setSaving(true);
    setMessage("");
    setUpdateError("");
    try {
      await invoke(command, args);
      await reload();
      setMessage(success);
    } catch (error) {
      setUpdateError(String(error));
    } finally {
      if (confirmation) {
        focusAfterDialog.current = "timeline";
        setConfirmation(null);
      }
      doingUpdate.current = false;
      setSaving(false);
    }
  }

  if (!native)
    return (
      <section className="panel">
        <Empty Icon={HistoryIcon} title="History lives on your device">
          Open the desktop app to browse saved scans.
        </Empty>
      </section>
    );
  if (error)
    return (
      <section className="panel">
        <Empty Icon={CircleAlert} title="History unavailable">
          <span role="alert">{error}</span>
        </Empty>
        <div className="historyActions historyHint">
          <button onClick={() => void reload()}>Try again</button>
        </div>
      </section>
    );
  if (!index)
    return (
      <p className="historyHint" role="status">
        Loading saved scans…
      </p>
    );
  return (
    <div className="historyLayout">
      {message && (
        <p className="historyFeedback" role="status">
          {message}
        </p>
      )}
      {updateError && (
        <p className="historyFeedback historyError" role="alert">
          {updateError}
        </p>
      )}
      {selected && <StorageTrend selected={selected} entries={index.entries} />}
      <div
        className={
          "historyColumns" + (index.entries.length ? "" : " historyEmpty")
        }
      >
        <div className="historyStack">
          <section
            className="panel historyTimeline"
            aria-labelledby="timeline-title"
          >
            <header className="sectionHead">
              <div>
                <HistoryIcon />
                <h2 id="timeline-title" ref={timelineTitle} tabIndex={-1}>
                  Scan timeline
                </h2>
              </div>
              <span className="mono muted">{index.entries.length} saved</span>
            </header>
            {!index.entries.length ? (
              <>
                <Empty Icon={HistoryIcon} title="No saved scans yet">
                  Completed scans appear here. Run two complete scans with the
                  same scope to compare them.
                </Empty>
                <div className="historyEmptyAction">
                  <button className="linkBtn" onClick={openOverview}>
                    Go to Overview <ArrowRight size={14} />
                  </button>
                </div>
              </>
            ) : (
              <>
                <div className="historyFilters">
                  <input
                    aria-label="Search saved scans by first root or date"
                    placeholder="Search by first root or date"
                    value={query}
                    onChange={(e) => {
                      setQuery(e.target.value);
                      setPage(0);
                      setRequestedId(null);
                    }}
                  />
                  <select
                    aria-label="Scan completeness"
                    value={status}
                    onChange={(e) => {
                      setStatus(e.target.value);
                      setPage(0);
                      setRequestedId(null);
                    }}
                  >
                    <option value="all">All scans</option>
                    <option value="complete">Complete</option>
                    <option value="incomplete">Incomplete</option>
                  </select>
                </div>
                {visible.length ? (
                  <ol className="timelineList" aria-label="Saved scans">
                    {visible.map((entry) => (
                      <li key={entry.id}>
                        <button
                          className="timelineScan"
                          aria-pressed={selected?.id === entry.id}
                          onClick={() => setRequestedId(entry.id)}
                        >
                          <span className="timelineIdentity">
                            <strong>
                              {formatScanDate(entry.completed_at_ms)}
                            </strong>
                            <small title={entry.root_hint}>
                              {basename(entry.root_hint)}
                              {entry.root_count > 1
                                ? " +" + (entry.root_count - 1)
                                : ""}
                            </small>
                          </span>
                          <span className="timelineMetrics">
                            <b className="number">
                              {bytes(entry.totals.logical_bytes)}
                            </b>
                            <Completeness entry={entry} />
                          </span>
                        </button>
                      </li>
                    ))}
                  </ol>
                ) : (
                  <Empty Icon={HistoryIcon} title="No matching scans">
                    Try another search or change the completeness filter.
                  </Empty>
                )}
                {pages > 1 && (
                  <div className="historyPaging">
                    <button
                      aria-label="Previous timeline page"
                      disabled={pageIndex === 0}
                      onClick={() => {
                        setRequestedId(null);
                        setPage(pageIndex - 1);
                      }}
                    >
                      <ArrowLeft size={14} />
                    </button>
                    <span aria-live="polite">
                      {pageIndex + 1} / {pages}
                    </span>
                    <button
                      aria-label="Next timeline page"
                      disabled={pageIndex + 1 >= pages}
                      onClick={() => {
                        setRequestedId(null);
                        setPage(pageIndex + 1);
                      }}
                    >
                      <ArrowRight size={14} />
                    </button>
                  </div>
                )}
              </>
            )}
          </section>
          <Retention
            current={index.retention}
            disabled={disabled}
            hasEntries={index.entries.length > 0}
            save={(count) =>
              void change(
                "history_set_retention",
                { count },
                "History limit updated.",
              )
            }
            clear={(trigger) => requestRemoval({ kind: "clear" }, trigger)}
          />
        </div>
        {selected && (
          <div className="historyStack">
            <ComparePanel
              key={"compare-" + selected.id}
              entry={selected}
              entries={index.entries}
            />
            <DetailPanel
              key={"detail-" + selected.id}
              entry={selected}
              busy={disabled}
              remove={(trigger) =>
                requestRemoval({ kind: "remove", id: selected.id }, trigger)
              }
            />
          </div>
        )}
      </div>
      {confirmation && (
        <ConfirmRemoval
          target={confirmation}
          busy={saving}
          cancel={cancelRemoval}
          confirm={() => {
            const clear = confirmation.kind === "clear";
            void change(
              clear ? "history_clear" : "history_remove",
              clear ? {} : { id: confirmation.id },
              clear ? "Scan history cleared." : "Saved scan removed.",
            );
          }}
        />
      )}
    </div>
  );
}
