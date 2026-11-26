import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useRef, useState } from "react";
import { bytes, type Report } from "./model";

export interface HistoryEntry {
  id: number;
  completed_at_ms: number;
  duration_ms: number;
  scope_key: string;
  discovery_rule_version: number;
  root_hint: string;
  root_count: number;
  project_count: number;
  candidate_count: number;
  warning_count: number;
  incomplete_candidate_count: number;
  directories: number;
  complete: boolean;
  totals: Report["totals"];
}

export interface HistoryIndex {
  entries: HistoryEntry[];
  retention: number;
}

export interface HistoryDetail {
  entry: HistoryEntry;
  roots: string[];
  exclusions: string[];
  warnings: string[];
  ecosystems: Record<string, number>;
  categories: Record<string, number>;
  largest_projects: {
    path: string;
    ecosystems: string[];
    logical_bytes: number;
    candidate_count: number;
  }[];
  project_details_available: boolean;
}

export interface ProjectChanges {
  count: number;
  projects: { path: string; before: number | null; after: number | null }[];
}

export interface Comparison {
  logical_bytes_delta: number;
  ecosystems: { name: string; before: number; after: number }[];
  categories: Comparison["ecosystems"];
  growth: ProjectChanges | null;
  discovered: ProjectChanges | null;
  no_longer_observed: ProjectChanges | null;
}

export function useHistoryIndex(native: boolean) {
  const [index, setIndex] = useState<HistoryIndex | null>(null);
  const [error, setError] = useState("");
  const request = useRef(0);
  const reload = useCallback(async () => {
    if (!native) return;
    const id = ++request.current;
    setError("");
    try {
      const result = await invoke<HistoryIndex>("history_list");
      if (id === request.current) setIndex(result);
    } catch (error) {
      if (id === request.current) setError(String(error));
    }
  }, [native]);
  useEffect(() => {
    void reload();
    return () => {
      request.current++;
    };
  }, [reload]);
  return { index, error, reload };
}

const timestamp = new Intl.DateTimeFormat(undefined, {
  dateStyle: "medium",
  timeStyle: "medium",
});
const shortDate = new Intl.DateTimeFormat(undefined, {
  month: "short",
  day: "numeric",
});
export const formatScanDate = (ms: number) => timestamp.format(ms);
export const formatChartDate = (ms: number) => shortDate.format(ms);

export function signedBytes(value: number) {
  return (value > 0 ? "+" : value < 0 ? "−" : "") + bytes(Math.abs(value));
}

export function compatibleScans(entries: HistoryEntry[], entry: HistoryEntry) {
  return entry.complete
    ? entries.filter(
        (e) =>
          sameComparableScope(e, entry) &&
          (e.completed_at_ms < entry.completed_at_ms ||
            (e.completed_at_ms === entry.completed_at_ms && e.id < entry.id)),
      )
    : [];
}

export function sameComparableScope(first: HistoryEntry, second: HistoryEntry) {
  return (
    first.complete &&
    second.complete &&
    first.discovery_rule_version === second.discovery_rule_version &&
    first.scope_key === second.scope_key
  );
}
