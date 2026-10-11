import { useEffect, useMemo, useRef, useState } from "react";
import { api } from "../../shared/api";
import { normalizeHexColor } from "../../shared/format";
import type { Task, TaskBookingSummary, TrackerState } from "../../shared/types";
import { ConfirmBar } from "./ConfirmBar";

type Draft = {
  name: string;
  color: string;
  description: string;
  notes: string;
};

function draftFromTask(task: Task): Draft {
  return {
    name: task.name,
    color: normalizeHexColor(task.color),
    description: task.description,
    notes: task.notes,
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
  });
  const [error, setError] = useState<string | null>(null);
  const [pendingSelectId, setPendingSelectId] = useState<string | null>(null);
  const [booking, setBooking] = useState<TaskBookingSummary | null>(null);

  const load = async () => {
    const list = await api.listTasks(true);
    setTasks(list);
    if (!selectedId && list[0]) setSelectedId(list[0].id);
  };

  useEffect(() => {
    load().catch((e) => setError(String(e)));
  }, []);

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
    }
  }, [selectedId, selected?.updatedAt]);

  useEffect(() => {
    if (!selectedId) {
      setBooking(null);
      return;
    }
    let cancelled = false;
    setBooking(null);
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
  }, [selectedId]);

  const isDirty = useMemo(() => {
    if (!selected) return false;
    const baseline = draftFromTask(selected);
    return (
      draft.name !== baseline.name ||
      draft.color !== baseline.color ||
      draft.description !== baseline.description ||
      draft.notes !== baseline.notes
    );
  }, [selected, draft]);

  useEffect(() => {
    onDirtyChange?.(isDirty);
  }, [isDirty, onDirtyChange]);

  const save = async () => {
    if (!selected) return;
    const color = normalizeHexColor(draft.color);
    await api.updateTask({
      id: selected.id,
      name: draft.name,
      color,
      description: draft.description,
      notes: draft.notes,
    });
    setDraft((d) => ({ ...d, color }));
    await load();
    onSaved?.();
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
            message="You have unsaved changes."
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
