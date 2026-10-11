import { useEffect, useMemo, useState } from "react";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { api } from "../../shared/api";
import { useTrackerState } from "../../shared/useTrackerState";
import type { Task } from "../../shared/types";
import "./gap.css";

function formatGap(ms: number) {
  const totalMins = Math.floor(ms / 60000);
  const hours = Math.floor(totalMins / 60);
  const mins = totalMins % 60;
  if (hours > 0) return `${hours}h ${mins}m`;
  return `${mins}m`;
}

export function GapApp() {
  const { state } = useTrackerState();
  const [mode, setMode] = useState<"choose" | "search">("choose");
  const [tasks, setTasks] = useState<Task[]>([]);
  const [query, setQuery] = useState("");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api.listTasks(false).then(setTasks).catch(() => undefined);
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        return;
      }
      if (mode !== "choose") return;
      if (e.key === "1") {
        e.preventDefault();
        void api.resolveGapPrevious().catch((err) => setError(String(err)));
      } else if (e.key === "2") {
        e.preventDefault();
        setMode("search");
      } else if (e.key === "3") {
        e.preventDefault();
        void api.resolveGapDiscard().catch((err) => setError(String(err)));
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [mode]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return tasks;
    return tasks.filter((t) => t.name.toLowerCase().includes(q));
  }, [tasks, query]);

  useEffect(() => {
    if (state && state.gapDurationMs == null) {
      void getCurrentWebviewWindow().hide();
    }
  }, [state]);

  if (!state || state.gapDurationMs == null) {
    return null;
  }

  const prev = state.previousTask?.name ?? "previous task";
  const band = state.gapBand ?? "short";

  return (
    <div className={`gap band-${band}`}>
      <div className="duration mono">{formatGap(state.gapDurationMs)}</div>
      <p className="hint">Where should this time go?</p>
      {error && <div className="err">{error}</div>}

      {mode === "choose" ? (
        <div className="actions">
          <button
            className="primary"
            autoFocus
            onClick={() =>
              api.resolveGapPrevious().catch((err) => setError(String(err)))
            }
          >
            <kbd>1</kbd> Assign to {prev}
          </button>
          <button onClick={() => setMode("search")}>
            <kbd>2</kbd> Assign to another task
          </button>
          <button
            className="danger"
            onClick={() =>
              api.resolveGapDiscard().catch((err) => setError(String(err)))
            }
          >
            <kbd>3</kbd> Discard time
          </button>
        </div>
      ) : (
        <div className="search">
          <input
            autoFocus
            placeholder="Find task…"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
          <ul>
            {filtered.map((task) => (
              <li key={task.id}>
                <button
                  onClick={() =>
                    api
                      .resolveGapOther(task.id)
                      .catch((err) => setError(String(err)))
                  }
                >
                  <span className="dot" style={{ background: task.color }} />
                  {task.name}
                </button>
              </li>
            ))}
          </ul>
          <button className="back" onClick={() => setMode("choose")}>
            Back
          </button>
        </div>
      )}
    </div>
  );
}
