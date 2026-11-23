export type Risk = "rebuildable" | "review_required" | "protected";
export interface Candidate {
  path: string;
  category: string;
  logical_bytes: number;
  classification: Risk;
  reason: string;
  rebuild: string;
  cost: "low" | "medium" | "high" | "unknown";
}
export interface Project {
  path: string;
  name: string;
  ecosystems: string[];
  logical_bytes: number;
  last_activity: number | null;
  protected: boolean;
  candidates: Candidate[];
}
export interface Report {
  projects: Project[];
  warnings: string[];
  cancelled: boolean;
  directories: number;
}
export interface Settings {
  roots: string[];
  appearance: "dark" | "light" | "system";
  excludedNames: string[];
  protectedProjects: string[];
}
export interface Plan {
  candidates: Candidate[];
  logical_bytes: number;
}
export const initialSettings: Settings = {
  roots: [],
  appearance: "dark",
  excludedNames: [],
  protectedProjects: [],
};
export const riskName: Record<Risk, string> = {
  rebuildable: "Rebuildable",
  review_required: "Review required",
  protected: "Protected",
};
export const basename = (path: string) =>
  path.split(/[\\/]/).filter(Boolean).pop() || path;
export function bytes(value: number) {
  if (!value) return "0 B";
  const units = ["B", "KiB", "MiB", "GiB", "TiB", "PiB", "EiB"];
  const power = Math.min(
    units.length - 1,
    Math.floor(Math.log(value) / Math.log(1024)),
  );
  return (
    (value / 1024 ** power).toLocaleString(undefined, {
      maximumFractionDigits: power ? 1 : 0,
    }) +
    " " +
    units[power]
  );
}
export function activity(project: Project) {
  return project.last_activity
    ? new Date(project.last_activity * 1000).toLocaleDateString(undefined, {
        month: "short",
        day: "numeric",
        year: "numeric",
      })
    : "Unknown";
}
export const palette = [
  "#77dcc2",
  "#76abdf",
  "#b1a0e4",
  "#e2b578",
  "#c3d78e",
  "#d88fa9",
];
