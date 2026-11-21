import {
  Folder,
  ArrowUpRight,
  ShieldCheck,
  ShieldAlert,
  LockKeyhole,
  type LucideIcon,
} from "lucide-react";
import { basename, bytes, riskName, type Project, type Risk } from "./model";

export function Empty({
  Icon = Folder,
  title,
  children,
}: {
  Icon?: LucideIcon;
  title: string;
  children: React.ReactNode;
}) {
  return (
    <div className="empty">
      <span className="emptyIcon">
        <Icon size={22} />
      </span>
      <strong>{title}</strong>
      <p>{children}</p>
    </div>
  );
}
export const riskIcon = {
  rebuildable: ShieldCheck,
  review_required: ShieldAlert,
  protected: LockKeyhole,
};
export function RiskLabel({ risk }: { risk: Risk }) {
  const Icon = riskIcon[risk];
  return (
    <span className={"risk " + risk}>
      <Icon size={14} />
      {riskName[risk]}
    </span>
  );
}
export function ProjectRow({
  p,
  open,
}: {
  p: Project;
  open: (p: Project) => void;
}) {
  return (
    <button className="projectRow" onClick={() => open(p)}>
      <span className="folderIcon">
        <Folder size={17} />
      </span>
      <span className="rowIdentity">
        <strong>{p.name || basename(p.path)}</strong>
        <small title={p.path}>{p.path}</small>
      </span>
      <span className="eco">{p.ecosystems.join(" · ")}</span>
      <span className="number">{bytes(p.logical_bytes)}</span>
      <ArrowUpRight size={15} />
    </button>
  );
}
