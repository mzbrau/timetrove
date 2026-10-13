import { useEffect, useMemo, useRef, useState } from "react";
import { api } from "../../shared/api";
import { withStartupRetry } from "../../shared/startupRetry";
import { useTrackerState } from "../../shared/useTrackerState";
import type { Task } from "../../shared/types";
import "./switcher.css";

type Mode = "switch" | "note";

export function SwitcherApp() {
  const { state } = useTrackerState();
  const [tasks, setTasks] = useState<Task[]>([]);
  const [query, setQuery] = useState("");
  const [noteDraft, setNoteDraft] = useState("");
  const [confirmCreate, setConfirmCreate] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState(0);
  const [mode, setMode] = useState<Mode>("switch");
  const inputRef = useRef<HTMLInputElement | null>(null);

  const tracking = Boolean(state?.tracking);
  const noteModeAvailable = tracking;
  const trackingSliceId =
    state?.phase.kind === "tracking" ? state.phase.sliceId : null;

  const load = () => {
    withStartupRetry(() => api.listTasks(false))
      .then((list) => {
        setTasks(list);
        setError(null);
      })
      .catch((e) => setError(String(e)));
  };

  const resetToSwitch = () => {
    setMode("switch");
    setQuery("");
    setNoteDraft("");
    setConfirmCreate(false);
    setSelected(0);
    setError(null);
  };

  useEffect(() => {
    load();
    const onFocus = () => {
      resetToSwitch();
      load();
    };
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, []);

  useEffect(() => {
    if (!noteModeAvailable && mode === "note") {
      setMode("switch");
      setNoteDraft("");
    }
  }, [noteModeAvailable, mode]);

  useEffect(() => {
    if (mode === "note" && noteModeAvailable) {
      setNoteDraft(state?.currentSliceNote ?? "");
    }
    // Depend on slice identity, not phase object / note string — polls must not wipe typing.
    // eslint-disable-next-line react-hooks/exhaustive-deps -- intentional: only re-prefill on mode/slice
  }, [mode, noteModeAvailable, trackingSliceId]);

  useEffect(() => {
    inputRef.current?.focus();
  }, [mode]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return tasks;
    return tasks.filter((t) => t.name.toLowerCase().includes(q));
  }, [tasks, query]);

  const exact = filtered.find(
    (t) => t.name.toLowerCase() === query.trim().toLowerCase(),
  );

  useEffect(() => {
    setSelected(0);
    setConfirmCreate(false);
  }, [query]);

  const enterNoteMode = () => {
    if (!noteModeAvailable) return;
    setMode("note");
    setConfirmCreate(false);
    setError(null);
    setNoteDraft(state?.currentSliceNote ?? "");
  };

  const enterSwitchMode = () => {
    setMode("switch");
    setError(null);
  };

  const submitSwitch = async () => {
    setError(null);
    const q = query.trim();
    try {
      if (exact) {
        await api.switchTask(exact.id);
        return;
      }
      if (filtered.length > 0 && !confirmCreate) {
        await api.switchTask(
          filtered[Math.min(selected, filtered.length - 1)].id,
        );
        return;
      }
      if (!q) return;
      if (!confirmCreate) {
        setConfirmCreate(true);
        return;
      }
      await api.createAndStart(q);
    } catch (e) {
      setError(String(e));
    }
  };

  const submitNote = async () => {
    setError(null);
    if (!state || state.phase.kind !== "tracking") {
      setError("No active time slice");
      return;
    }
    try {
      await api.updateSlice({
        id: state.phase.sliceId,
        note: noteDraft,
      });
      await api.closeSwitcher();
    } catch (e) {
      setError(String(e));
    }
  };

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        api.closeSwitcher();
        return;
      }
      if (e.key === "Tab" && e.ctrlKey && !e.metaKey && !e.altKey) {
        e.preventDefault();
        if (!noteModeAvailable) return;
        if (mode === "switch") {
          enterNoteMode();
        } else {
          enterSwitchMode();
        }
        return;
      }
      if (mode === "switch") {
        if (e.key === "ArrowDown") {
          e.preventDefault();
          setSelected((s) => Math.min(s + 1, Math.max(filtered.length - 1, 0)));
        } else if (e.key === "ArrowUp") {
          e.preventDefault();
          setSelected((s) => Math.max(s - 1, 0));
        }
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [filtered.length, mode, noteModeAvailable, state?.currentSliceNote]);

  return (
    <div className="switcher">
      <div
        className="switcher-modes"
        role="tablist"
        aria-label="Switcher mode"
      >
        <button
          type="button"
          role="tab"
          aria-selected={mode === "switch"}
          className={mode === "switch" ? "active" : ""}
          onClick={enterSwitchMode}
        >
          Switch task
        </button>
        {noteModeAvailable && (
          <button
            type="button"
            role="tab"
            aria-selected={mode === "note"}
            className={mode === "note" ? "active" : ""}
            onClick={enterNoteMode}
          >
            Add note
          </button>
        )}
      </div>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          if (mode === "note") {
            void submitNote();
          } else {
            void submitSwitch();
          }
        }}
      >
        <input
          ref={inputRef}
          autoFocus
          placeholder={
            mode === "note"
              ? "Note for current slice…"
              : "Search or create a task…"
          }
          value={mode === "note" ? noteDraft : query}
          onChange={(e) => {
            if (mode === "note") {
              setNoteDraft(e.target.value);
            } else {
              setQuery(e.target.value);
            }
          }}
        />
      </form>
      {error && <div className="err">{error}</div>}
      {mode === "switch" && confirmCreate && !exact && (
        <div className="confirm">
          Create task “{query.trim()}”? Press Enter to confirm.
        </div>
      )}
      {mode === "switch" && (
        <ul>
          {filtered.map((task, i) => (
            <li key={task.id}>
              <button
                type="button"
                className={i === selected ? "active" : ""}
                onMouseEnter={() => setSelected(i)}
                onClick={() =>
                  api.switchTask(task.id).catch((e) => setError(String(e)))
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
      )}
    </div>
  );
}
