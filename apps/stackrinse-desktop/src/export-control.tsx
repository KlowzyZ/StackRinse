import { invoke } from "@tauri-apps/api/core";
import { Download } from "lucide-react";
import { useRef, useState } from "react";

type Source = { kind: "fresh" } | { kind: "history"; id: number };

export function ExportControl({
  source,
  disabled,
}: {
  source: Source;
  disabled: boolean;
}) {
  const pending = useRef(false);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");
  async function save(format: "json" | "csv") {
    if (pending.current || disabled) return;
    pending.current = true;
    setBusy(true);
    setMessage("");
    setError("");
    try {
      const saved = await invoke<boolean>("export_inventory", {
        source,
        format,
      });
      if (saved) setMessage(format.toUpperCase() + " report saved.");
    } catch (error) {
      setError(String(error));
    } finally {
      pending.current = false;
      setBusy(false);
    }
  }
  return (
    <details className="inventoryExport">
      <summary>
        <Download size={15} /> Export inventory
      </summary>
      <div>
        <p>
          Reports may contain local filesystem paths. Choose where to save your
          copy.
        </p>
        <div className="historyActions">
          <button disabled={disabled || busy} onClick={() => void save("json")}>
            Save JSON…
          </button>
          <button disabled={disabled || busy} onClick={() => void save("csv")}>
            Save CSV…
          </button>
        </div>
        <p role="status" hidden={!busy && !message}>
          {busy ? "Waiting for the save dialog…" : message}
        </p>
        {error && (
          <p role="alert" className="historyError">
            {error}
          </p>
        )}
      </div>
    </details>
  );
}
