import { listen } from "@tauri-apps/api/event";
import { useEffect, useMemo, useRef, useState } from "react";
import { api } from "../../shared/api";
import { useTrackerState } from "../../shared/useTrackerState";
import type { JiraDashboardIssue, Settings, Task } from "../../shared/types";
import "./switcher.css";

type Mode = "switch" | "note" | "link";

export function SwitcherApp() {
  const { state } = useTrackerState();
  const [tasks, setTasks] = useState<Task[]>([]);
  const [query, setQuery] = useState("");
  const [noteDraft, setNoteDraft] = useState("");
  const [confirmCreate, setConfirmCreate] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState(0);
  const [mode, setMode] = useState<Mode>("switch");
  const [jiraEnabled, setJiraEnabled] = useState(false);
  const [jiraIssues, setJiraIssues] = useState<JiraDashboardIssue[]>([]);
  const [jiraLoading, setJiraLoading] = useState(false);
  const [jiraTruncated, setJiraTruncated] = useState(false);
  const [pendingLink, setPendingLink] = useState<JiraDashboardIssue | null>(
    null,
  );
  const inputRef = useRef<HTMLInputElement | null>(null);

  const tracking = Boolean(state?.tracking);
  const noteModeAvailable = tracking;
  const linkModeAvailable = tracking && jiraEnabled;
  const trackingSliceId =
    state?.phase.kind === "tracking" ? state.phase.sliceId : null;
  const currentTask = state?.currentTask ?? null;

  const load = () => {
    api
      .listTasks(false)
      .then((list) => {
        setTasks(list);
        setError(null);
      })
      .catch((e) => setError(String(e)));
  };

  const loadJiraIssues = (force = false) => {
    setJiraLoading(true);
    api
      .listAssignedJiraIssues(force)
      .then((result) => {
        setJiraIssues(result.issues);
        setJiraTruncated(result.truncated);
        if (result.error) setError(result.error);
        else setError(null);
      })
      .catch((e) => setError(String(e)))
      .finally(() => setJiraLoading(false));
  };

  const resetToSwitch = () => {
    setMode("switch");
    setQuery("");
    setNoteDraft("");
    setConfirmCreate(false);
    setPendingLink(null);
    setSelected(0);
    setError(null);
  };

  useEffect(() => {
    void api
      .getSettings()
      .then((s) => setJiraEnabled(s.jiraEnabled))
      .catch(() => {});
    let unlisten: (() => void) | undefined;
    listen<Settings>("settings-changed", (e) => {
      setJiraEnabled(e.payload.jiraEnabled);
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }, []);

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
    if (!linkModeAvailable && mode === "link") {
      setMode("switch");
      setPendingLink(null);
    }
  }, [noteModeAvailable, linkModeAvailable, mode]);

  useEffect(() => {
    if (mode === "note" && noteModeAvailable) {
      setNoteDraft(state?.currentSliceNote ?? "");
    }
    // Depend on slice identity, not phase object / note string — polls must not wipe typing.
    // eslint-disable-next-line react-hooks/exhaustive-deps -- intentional: only re-prefill on mode/slice
  }, [mode, noteModeAvailable, trackingSliceId]);

  useEffect(() => {
    if (mode === "link" && linkModeAvailable) {
      loadJiraIssues(false);
    }
  }, [mode, linkModeAvailable]);

  useEffect(() => {
    inputRef.current?.focus();
  }, [mode, pendingLink]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return tasks;
    return tasks.filter((t) => t.name.toLowerCase().includes(q));
  }, [tasks, query]);

  const filteredIssues = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return jiraIssues;
    return jiraIssues.filter(
      (issue) =>
        issue.issueKey.toLowerCase().includes(q) ||
        issue.summary.toLowerCase().includes(q),
    );
  }, [jiraIssues, query]);

  const exact = filtered.find(
    (t) => t.name.toLowerCase() === query.trim().toLowerCase(),
  );

  useEffect(() => {
    setSelected(0);
    setConfirmCreate(false);
  }, [query]);

  const availableModes = useMemo(() => {
    const modes: Mode[] = ["switch"];
    if (noteModeAvailable) modes.push("note");
    if (linkModeAvailable) modes.push("link");
    return modes;
  }, [noteModeAvailable, linkModeAvailable]);

  const cycleMode = () => {
    if (availableModes.length < 2) return;
    const idx = availableModes.indexOf(mode);
    const next = availableModes[(idx + 1) % availableModes.length];
    if (next === "note") enterNoteMode();
    else if (next === "link") enterLinkMode();
    else enterSwitchMode();
  };

  const enterNoteMode = () => {
    if (!noteModeAvailable) return;
    setMode("note");
    setConfirmCreate(false);
    setPendingLink(null);
    setError(null);
    setNoteDraft(state?.currentSliceNote ?? "");
  };

  const enterLinkMode = () => {
    if (!linkModeAvailable) return;
    setMode("link");
    setConfirmCreate(false);
    setPendingLink(null);
    setQuery("");
    setError(null);
  };

  const enterSwitchMode = () => {
    setMode("switch");
    setPendingLink(null);
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

  const applyLink = async (issue: JiraDashboardIssue) => {
    if (!currentTask) {
      setError("No active task");
      return;
    }
    setError(null);
    try {
      await api.updateTask({
        id: currentTask.id,
        jiraIssueKey: issue.issueKey,
        jiraIssueDetails: {
          issueKey: issue.issueKey,
          summary: issue.summary,
          status: issue.status,
          assignee: issue.assignee,
          estimateSeconds: issue.estimateSeconds,
          totalSpentSeconds: issue.totalSpentSeconds,
        },
      });
      await api.closeSwitcher();
    } catch (e) {
      setError(String(e));
      setPendingLink(null);
    }
  };

  const selectIssue = (issue: JiraDashboardIssue) => {
    if (!currentTask) {
      setError("No active task");
      return;
    }
    const existing = currentTask.jiraIssueKey?.trim() ?? "";
    if (existing && existing.toUpperCase() === issue.issueKey.toUpperCase()) {
      void api.closeSwitcher();
      return;
    }
    if (existing) {
      setPendingLink(issue);
      return;
    }
    void applyLink(issue);
  };

  const submitLink = () => {
    if (pendingLink) {
      void applyLink(pendingLink);
      return;
    }
    if (filteredIssues.length === 0) return;
    const issue =
      filteredIssues[Math.min(selected, filteredIssues.length - 1)];
    selectIssue(issue);
  };

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        if (mode === "link" && pendingLink) {
          setPendingLink(null);
          return;
        }
        api.closeSwitcher();
        return;
      }
      if (e.key === "Tab" && e.ctrlKey && !e.metaKey && !e.altKey) {
        e.preventDefault();
        cycleMode();
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
      if (mode === "link" && !pendingLink) {
        if (e.key === "ArrowDown") {
          e.preventDefault();
          setSelected((s) =>
            Math.min(s + 1, Math.max(filteredIssues.length - 1, 0)),
          );
        } else if (e.key === "ArrowUp") {
          e.preventDefault();
          setSelected((s) => Math.max(s - 1, 0));
        }
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [
    filtered.length,
    filteredIssues.length,
    mode,
    pendingLink,
    availableModes,
    state?.currentSliceNote,
  ]);

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
        {linkModeAvailable && (
          <button
            type="button"
            role="tab"
            aria-selected={mode === "link"}
            className={mode === "link" ? "active" : ""}
            onClick={enterLinkMode}
          >
            Link Jira
          </button>
        )}
      </div>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          if (mode === "note") {
            void submitNote();
          } else if (mode === "link") {
            submitLink();
          } else {
            void submitSwitch();
          }
        }}
      >
        <input
          ref={inputRef}
          autoFocus
          disabled={mode === "link" && Boolean(pendingLink)}
          placeholder={
            mode === "note"
              ? "Note for current slice…"
              : mode === "link"
                ? "Filter issues by key or summary…"
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
      {mode === "link" && pendingLink && currentTask && (
        <div className="confirm link-confirm">
          <p>
            Replace <strong>{currentTask.jiraIssueKey}</strong> with{" "}
            <strong>{pendingLink.issueKey}</strong> on{" "}
            <strong>{currentTask.name}</strong>?
          </p>
          <p className="hint">Enter to confirm · Esc to cancel</p>
          <div className="link-confirm-actions">
            <button
              type="button"
              onClick={() => setPendingLink(null)}
            >
              Cancel
            </button>
            <button
              type="button"
              className="primary"
              onClick={() => void applyLink(pendingLink)}
            >
              Replace
            </button>
          </div>
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
      {mode === "link" && !pendingLink && (
        <>
          {jiraLoading && <div className="hint-bar">Loading issues…</div>}
          {!jiraLoading && jiraTruncated && (
            <div className="hint-bar">Showing up to 50 open issues</div>
          )}
          <ul>
            {filteredIssues.map((issue, i) => (
              <li key={issue.issueKey}>
                <button
                  type="button"
                  className={i === selected ? "active" : ""}
                  onMouseEnter={() => setSelected(i)}
                  onClick={() => selectIssue(issue)}
                >
                  <span className="jira-key mono">{issue.issueKey}</span>
                  <span className="jira-summary">{issue.summary}</span>
                  {issue.status && (
                    <span className="jira-status muted">{issue.status}</span>
                  )}
                </button>
              </li>
            ))}
            {!jiraLoading && filteredIssues.length === 0 && (
              <li className="empty">
                {query.trim()
                  ? "No matching issues"
                  : "No open issues assigned to you"}
              </li>
            )}
          </ul>
        </>
      )}
    </div>
  );
}
