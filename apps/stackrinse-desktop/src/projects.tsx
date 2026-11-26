import { CandidateList } from "./candidate-list";
import { useDeferredValue, useMemo, useState } from "react";
import {
  ArrowLeft,
  ArrowRight,
  Check,
  Folder,
  FolderSearch,
  Layers3,
  LockKeyhole,
  Search,
  Shield,
  SlidersHorizontal,
} from "lucide-react";
import {
  activity,
  basename,
  bytes,
  riskName,
  type Project,
  type Risk,
} from "./model";
import { Empty, ProjectRow, RiskLabel } from "./shared";

export function Explorer({
  projects,
  scanned,
  open,
}: {
  projects: Project[];
  scanned: boolean;
  open: (p: Project, button?: HTMLButtonElement) => void;
}) {
  const [query, setQuery] = useState(""),
    [tech, setTech] = useState(""),
    [risk, setRisk] = useState(""),
    [sort, setSort] = useState("size"),
    [page, setPage] = useState(0);
  const search = useDeferredValue(query.toLocaleLowerCase());
  const ecosystems = useMemo(
    () => [...new Set(projects.flatMap((p) => p.ecosystems))].sort(),
    [projects],
  );
  const filtered = useMemo(
    () =>
      projects
        .filter(
          (p) =>
            (!search || p.path.toLocaleLowerCase().includes(search)) &&
            (!tech || p.ecosystems.includes(tech)) &&
            (!risk || p.candidates.some((c) => c.classification === risk)),
        )
        .sort((a, b) =>
          sort === "size"
            ? b.logical_bytes - a.logical_bytes
            : sort === "activity"
              ? (b.last_activity || 0) - (a.last_activity || 0)
              : a.name.localeCompare(b.name),
        ),
    [projects, search, tech, risk, sort],
  );
  const per = 40,
    pages = Math.max(1, Math.ceil(filtered.length / per)),
    current = Math.min(page, pages - 1);
  return (
    <section className="panel">
      <div className="filters">
        <label className="search">
          <Search size={16} />
          <input
            aria-label="Search projects"
            placeholder="Search projects or paths…"
            value={query}
            onChange={(e) => {
              setQuery(e.target.value);
              setPage(0);
            }}
          />
        </label>
        <select
          aria-label="Technology filter"
          value={tech}
          onChange={(e) => {
            setTech(e.target.value);
            setPage(0);
          }}
        >
          <option value="">All technologies</option>
          {ecosystems.map((s) => (
            <option key={s}>{s}</option>
          ))}
        </select>
        <select
          aria-label="Safety filter"
          value={risk}
          onChange={(e) => {
            setRisk(e.target.value);
            setPage(0);
          }}
        >
          <option value="">All classifications</option>
          {Object.entries(riskName).map(([k, v]) => (
            <option key={k} value={k}>
              {v}
            </option>
          ))}
        </select>
        <select
          aria-label="Sort projects"
          value={sort}
          onChange={(e) => {
            setSort(e.target.value);
            setPage(0);
          }}
        >
          <option value="size">Largest first</option>
          <option value="activity">Latest manifest change</option>
          <option value="name">Name A–Z</option>
        </select>
      </div>
      <header className="sectionHead">
        <div>
          <Folder />
          {filtered.length} {filtered.length === 1 ? "project" : "projects"}
        </div>
        <span className="eyelabel">Candidate size</span>
      </header>
      {filtered.slice(current * per, (current + 1) * per).map((p) => (
        <ProjectRow p={p} key={p.path} open={open} />
      ))}
      {!filtered.length && (
        <Empty
          Icon={FolderSearch}
          title={
            projects.length
              ? "No matching projects"
              : scanned
                ? "No projects discovered"
                : "Ready to scan"
          }
        >
          {projects.length
            ? "Try another search or filter."
            : "Scan a workspace folder to build your inventory."}
        </Empty>
      )}
      {pages > 1 && (
        <div className="pagination">
          <button
            aria-label="Previous page"
            disabled={!current}
            onClick={() => setPage(current - 1)}
          >
            <ArrowLeft />
          </button>
          Page {current + 1} of {pages}
          <button
            aria-label="Next page"
            disabled={current + 1 === pages}
            onClick={() => setPage(current + 1)}
          >
            <ArrowRight />
          </button>
        </div>
      )}
    </section>
  );
}
const costs = {
  low: "Local regeneration with modest work.",
  medium: "Rebuilding or reinstalling dependencies may take additional work.",
  high: "A substantial rebuild or environment setup may be needed.",
  unknown: "Rebuild effort could not be established.",
};
export function ProjectDetails({
  p,
  preference,
  protect,
  locked,
  selected,
  toggle,
  review,
}: {
  p: Project;
  preference: boolean;
  protect: () => void;
  locked: boolean;
  selected: Set<string>;
  toggle: (s: string) => void;
  review: () => void;
}) {
  const protectedNow = p.protected || preference;
  const fixedReason =
    p.boundary_reason ||
    (p.protected && !preference
      ? "Protected by a parent folder preference."
      : null);
  return (
    <>
      <section className="panel projectMeta">
        <div>
          <span className="eyelabel">Logical candidate size</span>
          <strong>
            {bytes(p.logical_bytes)}
            {p.candidates.some((c) => !c.size_complete) && " observed"}
          </strong>
        </div>
        <div>
          <span className="eyelabel">Technologies</span>
          <strong>{p.ecosystems.join(", ")}</strong>
        </div>
        <div>
          <span className="eyelabel">Manifest modified</span>
          <strong>{activity(p)}</strong>
        </div>
        {fixedReason ? (
          <div className="protectionNote">
            <span>
              <LockKeyhole size={14} />{" "}
              {p.boundary_reason
                ? "Repository protection"
                : "Inherited protection"}
            </span>
            <small>{fixedReason}</small>
          </div>
        ) : (
          <button disabled={locked} aria-pressed={preference} onClick={protect}>
            <Shield size={15} />
            {preference ? "Remove protection preference" : "Protect project"}
          </button>
        )}
      </section>
      <section className="panel">
        <header className="sectionHead">
          <div>
            <Layers3 />
            Cleanup candidates
          </div>
          <button
            className="linkBtn"
            onClick={review}
            disabled={locked || !selected.size}
          >
            Review selection <ArrowRight size={14} />
          </button>
        </header>
        <CandidateList candidates={p.candidates}>
          {(c) => (
            <div className="candidate" key={c.path}>
              <div className="candidateTitle">
                <span className="folderIcon">
                  <Folder size={18} />
                </span>
                <div className="rowIdentity">
                  <strong>{basename(c.path)}</strong>
                  <small>{c.path}</small>
                </div>
                <span className="number">
                  {bytes(c.logical_bytes)}
                  {!c.size_complete && " observed"}
                </span>
              </div>
              <div className="candidateInfo">
                <RiskLabel
                  risk={protectedNow ? "protected" : c.classification}
                />
                <span>
                  {c.ecosystem} · {c.category}
                </span>
              </div>
              <p>{c.reason}</p>
              {c.evidence && !c.reason.includes(c.evidence) && (
                <p>
                  <b>Observed evidence</b> {c.evidence}
                </p>
              )}
              <p>
                <b>Rebuild</b> {c.rebuild}
              </p>
              <div className="candidateBottom">
                <span title={costs[c.cost]}>Rebuild cost · {c.cost}</span>
                <button
                  aria-pressed={selected.has(c.path)}
                  disabled={
                    locked ||
                    protectedNow ||
                    !c.size_complete ||
                    c.classification === "protected"
                  }
                  onClick={() => toggle(c.path)}
                >
                  {selected.has(c.path) ? (
                    <Check size={14} />
                  ) : (
                    <SlidersHorizontal size={14} />
                  )}{" "}
                  {selected.has(c.path) ? "Selected" : "Select for review"}
                </button>
              </div>
            </div>
          )}
        </CandidateList>
        {!p.candidates.length && (
          <Empty Icon={LockKeyhole} title="No recognized candidates">
            This project has no measured artifact directories.
          </Empty>
        )}
      </section>
    </>
  );
}
