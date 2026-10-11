import { useEffect, useState } from "react";
import { api } from "../../shared/api";
import type { Task, TrackerState } from "../../shared/types";

export function TasksPanel({ state }: { state: TrackerState | null }) {
  const [tasks, setTasks] = useState<Task[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [showArchived, setShowArchived] = useState(false);
  const [draft, setDraft] = useState({
    name: "",
    color: "#0f766e",
    description: "",
    notes: "",
  });
  const [error, setError] = useState<string | null>(null);

  const load = async () => {
    const list = await api.listTasks(true);
    setTasks(list);
    if (!selectedId && list[0]) setSelectedId(list[0].id);
  };

  useEffect(() => {
    load().catch((e) => setError(String(e)));
  }, []);

  const visible = tasks.filter((t) => showArchived || !t.archived);
  const selected = tasks.find((t) => t.id === selectedId) ?? null;

  useEffect(() => {
    if (selected) {
      setDraft({
        name: selected.name,
        color: selected.color,
        description: selected.description,
        notes: selected.notes,
      });
    }
  }, [selectedId, selected?.updatedAt]);

  return (
    <div className="tasks-layout">
      <div className="task-list">
        <div className="row-actions">
          <button
            type="button"
            onClick={async () => {
              try {
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
        <ul>
          {visible.map((task) => (
            <li key={task.id}>
              <button
                type="button"
                className={task.id === selectedId ? "selected" : ""}
                onClick={() => setSelectedId(task.id)}
              >
                <span className="dot" style={{ background: task.color }} />
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
        {!selected ? (
          <p className="muted">Select or create a task.</p>
        ) : (
          <>
            <label>
              Name
              <input
                value={draft.name}
                onChange={(e) => setDraft({ ...draft, name: e.target.value })}
              />
            </label>
            <label>
              Color
              <input
                type="color"
                value={draft.color}
                onChange={(e) => setDraft({ ...draft, color: e.target.value })}
              />
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
            <div className="row-actions">
              <button
                type="button"
                className="btn"
                onClick={async () => {
                  try {
                    await api.updateTask({ id: selected.id, ...draft });
                    await load();
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
                disabled={selected.archived}
              >
                {state?.currentTask?.id === selected.id ? "Tracking" : "Start"}
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
          </>
        )}
      </div>
    </div>
  );
}
