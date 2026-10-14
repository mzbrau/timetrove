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
  const [confirmCreate, setConfirmCreate] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const loadTasks = () => {
    api
      .listTasks(false)
      .then((list) => {
        setTasks(list);
        setError(null);
      })
      .catch((e) => setError(String(e)));
  };

  const leaveSearch = () => {
    setMode("choose");
    setQuery("");
    setConfirmCreate(false);
    setSelectedIndex(0);
  };

  const enterSearch = () => {
    setMode("search");
    setQuery("");
    setConfirmCreate(false);
    setSelectedIndex(0);
    setError(null);
    loadTasks();
  };

  useEffect(() => {
    loadTasks();
    const onFocus = () => loadTasks();
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, []);

  const gapStart =
    state?.phase.kind === "needsAttribution" ? state.phase.gapStart : null;

  // New gap while the window stays mounted — return to the choose screen.
  useEffect(() => {
    if (gapStart) {
      leaveSearch();
      setError(null);
      loadTasks();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps -- reset when gap identity changes
  }, [gapStart]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return tasks;
    return tasks.filter((t) => t.name.toLowerCase().includes(q));
  }, [tasks, query]);

  const exact = filtered.find(
    (t) => t.name.toLowerCase() === query.trim().toLowerCase(),
  );

  useEffect(() => {
    setSelectedIndex(0);
    setConfirmCreate(false);
  }, [query, filtered.length]);

  const submitSearch = async () => {
    setError(null);
    const q = query.trim();
    try {
      if (exact) {
        await api.resolveGapOther(exact.id);
        return;
      }
      if (filtered.length > 0 && !confirmCreate) {
        await api.resolveGapOther(
          filtered[Math.min(selectedIndex, filtered.length - 1)].id,
        );
        return;
      }
      if (!q) return;
      if (!confirmCreate) {
        setConfirmCreate(true);
        return;
      }
      await api.createAndResolveGapOther(q);
    } catch (err) {
      setError(String(err));
    }
  };

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
          enterSearch();
        } else if (e.key === "3") {
          e.preventDefault();
          void api.resolveGapDiscard().catch((err) => setError(String(err)));
        }
        return;
      }

      // search mode — Enter handled by form submit
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        leaveSearch();
      } else if (e.key === "ArrowDown") {
        e.preventDefault();
        setSelectedIndex((i) =>
          filtered.length === 0 ? 0 : Math.min(i + 1, filtered.length - 1),
        );
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        setSelectedIndex((i) => Math.max(i - 1, 0));
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [mode, filtered.length]);

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
          <button onClick={enterSearch}>
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
          <form
            onSubmit={(e) => {
              e.preventDefault();
              void submitSearch();
            }}
          >
            <input
              autoFocus
              placeholder="Search or create a task…"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
            />
          </form>
          {confirmCreate && !exact && (
            <div className="confirm">
              Create task “{query.trim()}”? Press Enter to confirm.
            </div>
          )}
          <ul>
            {filtered.map((task, index) => (
              <li key={task.id}>
                <button
                  type="button"
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
            {filtered.length === 0 && (
              <li className="empty">
                {query.trim()
                  ? "No matches — Enter to create"
                  : "No tasks yet — type a name and press Enter to create"}
              </li>
            )}
          </ul>
          <button type="button" className="back" onClick={leaveSearch}>
            Back
          </button>
        </div>
      )}
    </div>
  );
}
