import { listen } from "@tauri-apps/api/event";
import { useEffect, useMemo, useRef, useState } from "react";
import { api } from "../../shared/api";
import { formatHm, normalizeHexColor } from "../../shared/format";
import { useDemoShot } from "../../shared/useDemoShot";
import type {
  JiraIssueDetails,
  Settings,
  Task,
  TaskBookingSummary,
  TaskJiraHistoryEntry,
  TrackerState,
} from "../../shared/types";
import { ConfirmBar } from "./ConfirmBar";

type Draft = {
  name: string;
  color: string;
  description: string;
  notes: string;
  jiraIssueKey: string;
};

function normalizeIssueKeyDraft(raw: string): string {
  return raw.trim().toUpperCase();
}

function timetrovePostedMs(task: Task): number {
  return Math.max(0, (task.jiraReportedMs ?? 0) - (task.jiraLinkBaselineMs ?? 0));
}

function detailsFromTaskCache(task: Task): JiraIssueDetails | null {
  if (!task.jiraIssueKey) return null;
  return {
    issueKey: task.jiraIssueKey,
    summary: task.jiraIssueSummary ?? "",
    status: task.jiraIssueStatus ?? "",
    assignee: task.jiraIssueAssignee || "Unassigned",
    estimateSeconds: task.jiraIssueEstimateS ?? null,
    totalSpentSeconds: task.jiraIssueTotalSpentS ?? null,
  };
}

function formatEstimateSeconds(seconds: number | null | undefined): string {
  if (seconds == null || !Number.isFinite(seconds) || seconds < 0) return "—";
  return formatHm(seconds * 1000);
}

function applyIssueCache(task: Task, details: JiraIssueDetails): Task {
  return {
    ...task,
    jiraIssueSummary: details.summary,
    jiraIssueStatus: details.status,
    jiraIssueAssignee: details.assignee,
    jiraIssueEstimateS: details.estimateSeconds,
    jiraIssueTotalSpentS: details.totalSpentSeconds,
    jiraIssueFetchedAt: new Date().toISOString(),
  };
}

function draftFromTask(task: Task): Draft {
  return {
    name: task.name,
    color: normalizeHexColor(task.color),
    description: task.description,
    notes: task.notes,
    jiraIssueKey: task.jiraIssueKey ?? "",
  };
}

function formatLastBooked(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

type Props = {
  state: TrackerState | null;
  onDirtyChange?: (dirty: boolean) => void;
  saveRequestId?: number;
  onSaved?: () => void;
};

export function TasksPanel({
  state,
  onDirtyChange,
  saveRequestId = 0,
  onSaved,
}: Props) {
  const [tasks, setTasks] = useState<Task[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [showArchived, setShowArchived] = useState(false);
  const [filterQuery, setFilterQuery] = useState("");
  const [draft, setDraft] = useState<Draft>({
    name: "",
    color: "#0f766e",
    description: "",
    notes: "",
    jiraIssueKey: "",
  });
  const [error, setError] = useState<string | null>(null);
  const [pendingSelectId, setPendingSelectId] = useState<string | null>(null);
  const [booking, setBooking] = useState<TaskBookingSummary | null>(null);
  const [jiraEnabled, setJiraEnabled] = useState(false);
  const [jiraHistory, setJiraHistory] = useState<TaskJiraHistoryEntry[]>([]);
  /** Last issue successfully verified via Add (must match draft to Save a new key). */
  const [verifiedKey, setVerifiedKey] = useState<string | null>(null);
  const [stagedDetails, setStagedDetails] = useState<JiraIssueDetails | null>(
    null,
  );
  const [liveDetails, setLiveDetails] = useState<JiraIssueDetails | null>(null);
  const [jiraAdding, setJiraAdding] = useState(false);
  const [jiraRefreshError, setJiraRefreshError] = useState<string | null>(null);

  const load = async () => {
    const list = await api.listTasks(true);
    setTasks(list);
    if (!selectedId && list[0]) setSelectedId(list[0].id);
  };

  const [demoTaskName, setDemoTaskName] = useState<string | null>(null);
  useDemoShot((shot) => {
    if (shot.showArchived !== undefined) setShowArchived(shot.showArchived);
    if (shot.task) setDemoTaskName(shot.task);
  });
  useEffect(() => {
    if (!demoTaskName) return;
    const match = tasks.find((t) => t.name === demoTaskName);
    if (match) setSelectedId(match.id);
  }, [demoTaskName, tasks]);

  useEffect(() => {
    load().catch((e) => setError(String(e)));
    api
      .getSettings()
      .then((s) => setJiraEnabled(s.jiraEnabled))
      .catch((e) => setError(String(e)));
  }, []);

  useEffect(() => {
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

  const phaseKind = state?.phase.kind;
  const trackingTaskId =
    state?.phase.kind === "tracking" ? state.phase.taskId : null;

  // Refresh when switcher/create/start/stop changes tracker phase (not every idle poll).
  useEffect(() => {
    if (state) {
      load().catch((e) => setError(String(e)));
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps -- scalars only; avoid phase object identity
  }, [phaseKind, trackingTaskId, state?.tracking]);

  const filter = filterQuery.trim().toLowerCase();
  const visible = tasks.filter(
    (t) =>
      (showArchived || !t.archived) &&
      (!filter || t.name.toLowerCase().includes(filter)),
  );
  const selected = tasks.find((t) => t.id === selectedId) ?? null;

  useEffect(() => {
    if (selected) {
      setDraft(draftFromTask(selected));
      setVerifiedKey(selected.jiraIssueKey || null);
      setStagedDetails(null);
      setLiveDetails(detailsFromTaskCache(selected));
      setJiraRefreshError(null);
    }
    // Only reset on selection change. Updates like starring bump updatedAt and
    // must not wipe an in-progress dirty draft.
  }, [selectedId]);

  useEffect(() => {
    if (!selectedId) {
      setBooking(null);
      return;
    }
    let cancelled = false;
    api
      .getTaskBookingSummary(selectedId)
      .then((summary) => {
        if (!cancelled) setBooking(summary);
      })
      .catch((e) => {
        if (!cancelled) setError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [
    selectedId,
    state?.tracking,
    state?.currentTask?.id,
    state?.dailyTotalMs,
  ]);

  useEffect(() => {
    if (!selectedId || !jiraEnabled) {
      setJiraHistory([]);
      return;
    }
    let cancelled = false;
    api
      .listTaskJiraHistory(selectedId)
      .then((rows) => {
        if (!cancelled) setJiraHistory(rows);
      })
      .catch((e) => {
        if (!cancelled) setError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [selectedId, jiraEnabled]);

  // Background refresh of linked issue details — never blocks form render.
  useEffect(() => {
    if (!selectedId || !jiraEnabled) return;
    const linkedKey = selected?.jiraIssueKey;
    if (!linkedKey) return;
    let cancelled = false;
    api
      .refreshJiraIssue(selectedId)
      .then((details) => {
        if (cancelled || !details) return;
        setLiveDetails(details);
        setTasks((prev) =>
          prev.map((t) =>
            t.id === selectedId ? applyIssueCache(t, details) : t,
          ),
        );
        setJiraRefreshError(null);
      })
      .catch((e) => {
        if (!cancelled) setJiraRefreshError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [selectedId, jiraEnabled, selected?.jiraIssueKey]);

  const isDirty = useMemo(() => {
    if (!selected) return false;
    const baseline = draftFromTask(selected);
    return (
      draft.name !== baseline.name ||
      draft.color !== baseline.color ||
      draft.description !== baseline.description ||
      draft.notes !== baseline.notes ||
      normalizeIssueKeyDraft(draft.jiraIssueKey) !==
        normalizeIssueKeyDraft(baseline.jiraIssueKey)
    );
  }, [selected, draft]);

  const jiraKeyDirty = useMemo(() => {
    if (!selected || !jiraEnabled) return false;
    return (
      normalizeIssueKeyDraft(draft.jiraIssueKey) !==
      normalizeIssueKeyDraft(selected.jiraIssueKey)
    );
  }, [selected, draft.jiraIssueKey, jiraEnabled]);

  const unsavedMessage = jiraKeyDirty
    ? "You have unsaved changes, including a Jira issue link."
    : "You have unsaved changes.";

  useEffect(() => {
    onDirtyChange?.(isDirty);
  }, [isDirty, onDirtyChange]);

  const cardDetails = useMemo((): JiraIssueDetails | null => {
    const draftKey = normalizeIssueKeyDraft(draft.jiraIssueKey);
    if (!draftKey) return null;
    if (
      stagedDetails &&
      normalizeIssueKeyDraft(stagedDetails.issueKey) === draftKey
    ) {
      return stagedDetails;
    }
    if (
      selected &&
      normalizeIssueKeyDraft(selected.jiraIssueKey) === draftKey
    ) {
      return liveDetails ?? detailsFromTaskCache(selected);
    }
    return null;
  }, [draft.jiraIssueKey, stagedDetails, selected, liveDetails]);

  const cardTimetrovePostedMs = useMemo(() => {
    if (!selected || !cardDetails) return 0;
    const draftKey = normalizeIssueKeyDraft(draft.jiraIssueKey);
    if (normalizeIssueKeyDraft(selected.jiraIssueKey) === draftKey) {
      return timetrovePostedMs(selected);
    }
    // Staged new key not yet saved — nothing posted yet.
    return 0;
  }, [selected, cardDetails, draft.jiraIssueKey]);

  const save = async () => {
    if (!selected) return;
    const color = normalizeHexColor(draft.color);
    if (jiraEnabled) {
      const nextKey = normalizeIssueKeyDraft(draft.jiraIssueKey);
      const savedKey = normalizeIssueKeyDraft(selected.jiraIssueKey);
      if (nextKey && nextKey !== savedKey && verifiedKey !== nextKey) {
        throw new Error("Add the issue to verify it before saving.");
      }
    }
    await api.updateTask({
      id: selected.id,
      name: draft.name,
      color,
      description: draft.description,
      notes: draft.notes,
      ...(jiraEnabled
        ? {
            jiraIssueKey: draft.jiraIssueKey.trim(),
            ...(stagedDetails &&
            normalizeIssueKeyDraft(stagedDetails.issueKey) ===
              normalizeIssueKeyDraft(draft.jiraIssueKey)
              ? { jiraIssueDetails: stagedDetails }
              : {}),
          }
        : {}),
    });
    const savedKey = jiraEnabled
      ? normalizeIssueKeyDraft(draft.jiraIssueKey)
      : "";
    const detailsToKeep =
      stagedDetails &&
      normalizeIssueKeyDraft(stagedDetails.issueKey) === savedKey
        ? stagedDetails
        : null;
    setDraft((d) => ({ ...d, color }));
    setStagedDetails(null);
    setVerifiedKey(savedKey || null);
    if (!savedKey) {
      setLiveDetails(null);
    } else if (detailsToKeep) {
      setLiveDetails(detailsToKeep);
    }
    await load();
    if (selectedId) {
      const history = await api.listTaskJiraHistory(selectedId);
      setJiraHistory(history);
    }
    onSaved?.();
  };

  const addJiraIssue = async () => {
    const key = normalizeIssueKeyDraft(draft.jiraIssueKey);
    if (!key || jiraAdding) return;
    setJiraAdding(true);
    setError(null);
    try {
      const details = await api.fetchJiraIssue(key);
      setDraft((d) => ({ ...d, jiraIssueKey: details.issueKey }));
      setVerifiedKey(details.issueKey);
      setStagedDetails(details);
      setLiveDetails(details);
    } catch (e) {
      setError(String(e));
      setVerifiedKey(null);
      setStagedDetails(null);
    } finally {
      setJiraAdding(false);
    }
  };

  const lastSaveRequest = useRef(0);
  useEffect(() => {
    if (saveRequestId <= 0 || saveRequestId === lastSaveRequest.current) return;
    lastSaveRequest.current = saveRequestId;
    void save().catch((e) => setError(String(e)));
  }, [saveRequestId]);

  const requestSelect = (id: string) => {
    if (id === selectedId) return;
    if (isDirty) {
      setPendingSelectId(id);
      return;
    }
    setSelectedId(id);
  };

  const toggleStar = async (task: Task) => {
    try {
      setError(null);
      await api.updateTask({ id: task.id, starred: !task.starred });
      await load();
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <div className="tasks-layout">
      <div className="task-list">
        <div className="row-actions">
          <button
            type="button"
            onClick={async () => {
              try {
                if (isDirty) {
                  setPendingSelectId("__new__");
                  return;
                }
                const task = await api.createTask({
                  name: `New task ${new Date().toLocaleTimeString()}`,
                });
                await load();
                setSelectedId(task.id);
              } catch (e) {
                setError(String(e));
              }
            }}
          >
            New task
          </button>
          <label style={{ display: "flex", alignItems: "center", gap: 6 }}>
            <input
              type="checkbox"
              checked={showArchived}
              onChange={(e) => setShowArchived(e.target.checked)}
            />
            Archived
          </label>
        </div>
        <input
          className="task-filter"
          type="search"
          placeholder="Filter tasks…"
          value={filterQuery}
          onChange={(e) => setFilterQuery(e.target.value)}
        />
        <ul>
          {visible.map((task) => (
            <li key={task.id}>
              <button
                type="button"
                className={task.id === selectedId ? "selected" : ""}
                onClick={() => requestSelect(task.id)}
              >
                <span
                  className="dot"
                  style={{ backgroundColor: normalizeHexColor(task.color) }}
                />
                <span>
                  {task.name}
                  {task.archived ? " (archived)" : ""}
                </span>
              </button>
            </li>
          ))}
        </ul>
      </div>

      <div className="task-detail">
        {error && <div className="err">{error}</div>}
        {pendingSelectId && (
          <ConfirmBar
            message={unsavedMessage}
            confirmLabel="Save"
            secondaryLabel="Discard"
            cancelLabel="Cancel"
            onConfirm={async () => {
              try {
                await save();
                const next = pendingSelectId;
                setPendingSelectId(null);
                if (next === "__new__") {
                  const task = await api.createTask({
                    name: `New task ${new Date().toLocaleTimeString()}`,
                  });
                  await load();
                  setSelectedId(task.id);
                } else {
                  setSelectedId(next);
                }
              } catch (e) {
                setError(String(e));
              }
            }}
            onSecondary={async () => {
              const next = pendingSelectId;
              setPendingSelectId(null);
              if (next === "__new__") {
                try {
                  const task = await api.createTask({
                    name: `New task ${new Date().toLocaleTimeString()}`,
                  });
                  await load();
                  setSelectedId(task.id);
                } catch (e) {
                  setError(String(e));
                }
              } else {
                setSelectedId(next);
              }
            }}
            onCancel={() => setPendingSelectId(null)}
          />
        )}
        {!selected ? (
          <p className="muted">Select or create a task.</p>
        ) : (
          <div className="task-detail-body">
            <button
              type="button"
              className={`task-detail-star${selected.starred ? " on" : ""}`}
              aria-label={selected.starred ? "Unstar task" : "Star task"}
              title={selected.starred ? "Unstar" : "Star"}
              onClick={() => void toggleStar(selected)}
            >
              {selected.starred ? "★" : "☆"}
            </button>
            <label>
              Name
              <input
                value={draft.name}
                onChange={(e) => setDraft({ ...draft, name: e.target.value })}
              />
            </label>
            <label>
              Color
              <div className="color-row">
                <input
                  type="color"
                  value={draft.color}
                  onChange={(e) =>
                    setDraft({
                      ...draft,
                      color: normalizeHexColor(e.target.value),
                    })
                  }
                />
                <input
                  className="mono"
                  type="text"
                  value={draft.color}
                  onChange={(e) =>
                    setDraft({ ...draft, color: e.target.value })
                  }
                  onBlur={() =>
                    setDraft((d) => ({
                      ...d,
                      color: normalizeHexColor(d.color),
                    }))
                  }
                />
              </div>
            </label>
            <label>
              Description
              <textarea
                rows={3}
                value={draft.description}
                onChange={(e) =>
                  setDraft({ ...draft, description: e.target.value })
                }
              />
            </label>
            <label>
              Notes
              <textarea
                rows={4}
                value={draft.notes}
                onChange={(e) => setDraft({ ...draft, notes: e.target.value })}
              />
            </label>
            {jiraEnabled && (
              <div className="jira-issue-block">
                <label>
                  Jira issue
                  <div className="jira-issue-row">
                    <input
                      value={draft.jiraIssueKey}
                      placeholder="THING-123"
                      spellCheck={false}
                      onChange={(e) => {
                        const value = e.target.value;
                        setDraft({ ...draft, jiraIssueKey: value });
                        const next = normalizeIssueKeyDraft(value);
                        if (!next || next !== verifiedKey) {
                          setStagedDetails(null);
                          if (next !== normalizeIssueKeyDraft(selected.jiraIssueKey)) {
                            setLiveDetails(null);
                          }
                        }
                      }}
                      onKeyDown={(e) => {
                        if (e.key === "Enter") {
                          e.preventDefault();
                          void addJiraIssue();
                        }
                      }}
                    />
                    <button
                      type="button"
                      className="btn"
                      disabled={
                        jiraAdding ||
                        !normalizeIssueKeyDraft(draft.jiraIssueKey) ||
                        normalizeIssueKeyDraft(draft.jiraIssueKey) ===
                          verifiedKey
                      }
                      onClick={() => void addJiraIssue()}
                    >
                      {jiraAdding ? "…" : "Add"}
                    </button>
                  </div>
                  {jiraKeyDirty && (
                    <span className="hint jira-save-hint">
                      {normalizeIssueKeyDraft(draft.jiraIssueKey)
                        ? verifiedKey ===
                          normalizeIssueKeyDraft(draft.jiraIssueKey)
                          ? "Save to start booking time on this issue."
                          : "Add the issue to verify it, then Save."
                        : "Save to unlink this issue."}
                    </span>
                  )}
                </label>
                {cardDetails && (
                  <div className="jira-issue-card" aria-live="polite">
                    <div className="jira-issue-card-key mono">
                      {cardDetails.issueKey}
                    </div>
                    <div className="jira-issue-card-title">
                      {cardDetails.summary || "Untitled issue"}
                    </div>
                    <dl className="jira-issue-card-meta">
                      <div>
                        <dt>Status</dt>
                        <dd>{cardDetails.status || "—"}</dd>
                      </div>
                      <div>
                        <dt>Assignee</dt>
                        <dd>{cardDetails.assignee || "Unassigned"}</dd>
                      </div>
                      <div>
                        <dt>Estimate</dt>
                        <dd className="mono">
                          {formatEstimateSeconds(cardDetails.estimateSeconds)}
                        </dd>
                      </div>
                      <div>
                        <dt>TimeTrove booked</dt>
                        <dd className="mono">
                          {formatHm(cardTimetrovePostedMs)}
                        </dd>
                      </div>
                      {cardDetails.totalSpentSeconds != null &&
                        Math.round(cardDetails.totalSpentSeconds * 1000) !==
                          cardTimetrovePostedMs && (
                          <div>
                            <dt>Jira total</dt>
                            <dd className="mono">
                              {formatEstimateSeconds(
                                cardDetails.totalSpentSeconds,
                              )}
                            </dd>
                          </div>
                        )}
                    </dl>
                    {jiraRefreshError && (
                      <p className="muted jira-refresh-err">
                        Could not refresh: {jiraRefreshError}
                      </p>
                    )}
                  </div>
                )}
                {jiraHistory.length > 0 && (
                  <div className="jira-previous">
                    <div className="jira-previous-title">Previous tickets</div>
                    <ul className="jira-previous-list">
                      {jiraHistory.map((row) => (
                        <li key={row.id}>
                          <span className="mono jira-previous-key">
                            {row.issueKey}
                          </span>
                          <span className="jira-previous-summary">
                            {row.title || row.issueKey}
                          </span>
                          <span className="mono jira-previous-hours">
                            {formatHm(row.bookedMs)}
                          </span>
                        </li>
                      ))}
                    </ul>
                  </div>
                )}
              </div>
            )}
            <div className="task-activity" aria-live="polite">
              <div className="task-activity-title">Recent booking</div>
              {booking ? (
                booking.thisWeekMs === 0 && booking.lastWeekMs === 0 ? (
                  <p className="muted task-activity-idle">
                    {booking.lastBookedAt
                      ? `Last booked ${formatLastBooked(booking.lastBookedAt)}`
                      : "Never booked"}
                  </p>
                ) : (
                  <dl className="task-activity-weeks">
                    <div>
                      <dt>This week</dt>
                      <dd className="mono">{booking.thisWeekDisplay}</dd>
                    </div>
                    <div>
                      <dt>Last week</dt>
                      <dd className="mono">{booking.lastWeekDisplay}</dd>
                    </div>
                  </dl>
                )
              ) : (
                <p className="muted">Loading…</p>
              )}
            </div>
            <div className="row-actions">
              <button
                type="button"
                className="btn"
                disabled={!isDirty}
                onClick={async () => {
                  try {
                    setError(null);
                    await save();
                  } catch (e) {
                    setError(String(e));
                  }
                }}
              >
                Save
              </button>
              <button
                type="button"
                onClick={() => api.startTask(selected.id)}
                disabled={
                  selected.archived ||
                  state?.currentTask?.id === selected.id
                }
              >
                Start
              </button>
              <button
                type="button"
                onClick={async () => {
                  await api.updateTask({
                    id: selected.id,
                    archived: !selected.archived,
                  });
                  await load();
                }}
              >
                {selected.archived ? "Unarchive" : "Archive"}
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
