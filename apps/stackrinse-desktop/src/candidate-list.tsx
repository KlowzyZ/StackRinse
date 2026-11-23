import { useDeferredValue, useMemo, useState, type ReactNode } from "react";
import { ArrowLeft, ArrowRight, Search } from "lucide-react";
import type { Candidate } from "./model";

const pageSize = 20;

export function CandidateList({
  candidates,
  children,
}: {
  candidates: Candidate[];
  children: (candidate: Candidate) => ReactNode;
}) {
  const [query, setQuery] = useState("");
  const [page, setPage] = useState(0);
  const search = useDeferredValue(query.trim().toLocaleLowerCase());
  const filtered = useMemo(
    () =>
      search
        ? candidates.filter((c) => c.path.toLocaleLowerCase().includes(search))
        : candidates,
    [candidates, search],
  );
  const pages = Math.max(1, Math.ceil(filtered.length / pageSize));
  const current = Math.min(page, pages - 1);
  const start = current * pageSize;

  return (
    <>
      {(candidates.length > pageSize || query) && (
        <div className="candidateTools">
          <label className="search">
            <Search size={15} />
            <input
              aria-label="Filter candidate paths"
              placeholder="Filter candidate paths…"
              value={query}
              onChange={(event) => {
                setQuery(event.target.value);
                setPage(0);
              }}
            />
          </label>
          <span className="mono muted" role="status">
            {filtered.length
              ? `${start + 1}–${Math.min(start + pageSize, filtered.length)} of ${filtered.length}`
              : "No matching paths"}
          </span>
          {pages > 1 && (
            <div className="pageControls">
              <button
                aria-label="Previous candidates"
                disabled={current === 0}
                onClick={() => setPage(current - 1)}
              >
                <ArrowLeft size={14} />
              </button>
              <button
                aria-label="Next candidates"
                disabled={current + 1 === pages}
                onClick={() => setPage(current + 1)}
              >
                <ArrowRight size={14} />
              </button>
            </div>
          )}
        </div>
      )}
      {filtered.slice(start, start + pageSize).map(children)}
    </>
  );
}
