import { useEffect, useMemo, useState } from "react";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { api } from "../../shared/api";
import { formatGap } from "../../shared/format";
import { useTrackerState } from "../../shared/useTrackerState";
import type { Task } from "../../shared/types";
import "./gap.css";

export function GapApp() {
  const { state } = useTrackerState();
  const [mode, setMode] = useState<"choose" | "search">("choose");
  const [tasks, setTasks] = useState<Task[]>([]);
  const [query, setQuery] = useState("");
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api.listTasks(false).then(setTasks).catch(() => undefined);
  }, []);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return tasks;
    return tasks.filter((t) => t.name.toLowerCase().includes(q));
  }, [tasks, query]);

  useEffect(() => {
    setSelectedIndex(0);
  }, [query, filtered.length]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (mode === "choose") {
        if (e.key === "Escape") {
          e.preventDefault();
          e.stopPropagation();
          return;
        }
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
        return;
      }

      // search mode
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        setMode("choose");
        setQuery("");
        return;
      }
      if (e.key === "ArrowDown") {
        e.preventDefault();
        setSelectedIndex((i) =>
          filtered.length === 0 ? 0 : Math.min(i + 1, filtered.length - 1),
        );
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        setSelectedIndex((i) => Math.max(i - 1, 0));
      } else if (e.key === "Enter") {
        e.preventDefault();
        const task = filtered[selectedIndex];
        if (task) {
          void api
            .resolveGapOther(task.id)
            .catch((err) => setError(String(err)));
        }
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [mode, filtered, selectedIndex]);

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
            {filtered.map((task, index) => (
              <li key={task.id}>
                <button
                  className={index === selectedIndex ? "selected" : undefined}
                  onMouseEnter={() => setSelectedIndex(index)}
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
