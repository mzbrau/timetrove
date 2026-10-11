import { useEffect, useMemo, useState } from "react";
import { api } from "../../shared/api";
import type { Task } from "../../shared/types";
import "./switcher.css";

export function SwitcherApp() {
  const [tasks, setTasks] = useState<Task[]>([]);
  const [query, setQuery] = useState("");
  const [confirmCreate, setConfirmCreate] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState(0);

  const load = () => {
    api.listTasks(false).then(setTasks).catch((e) => setError(String(e)));
  };

  useEffect(() => {
    load();
    const onFocus = () => {
      setQuery("");
      setConfirmCreate(false);
      setSelected(0);
      load();
    };
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, []);

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

  const submit = async () => {
    setError(null);
    const q = query.trim();
    if (!q) return;
    try {
      if (exact) {
        await api.switchTask(exact.id);
        return;
      }
      if (filtered.length > 0 && !confirmCreate) {
        await api.switchTask(filtered[Math.min(selected, filtered.length - 1)].id);
        return;
      }
      if (!confirmCreate) {
        setConfirmCreate(true);
        return;
      }
      await api.createAndStart(q);
    } catch (e) {
      setError(String(e));
    }
  };

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        api.closeSwitcher();
      } else if (e.key === "ArrowDown") {
        e.preventDefault();
        setSelected((s) => Math.min(s + 1, Math.max(filtered.length - 1, 0)));
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        setSelected((s) => Math.max(s - 1, 0));
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [filtered.length]);

  return (
    <div className="switcher">
      <form
        onSubmit={(e) => {
          e.preventDefault();
          void submit();
        }}
      >
        <input
          autoFocus
          placeholder="Search or create a task…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
      </form>
      {error && <div className="err">{error}</div>}
      {confirmCreate && !exact && (
        <div className="confirm">
          Create task “{query.trim()}”? Press Enter to confirm.
        </div>
      )}
      <ul>
        {filtered.map((task, i) => (
          <li key={task.id}>
            <button
              type="button"
              className={i === selected ? "active" : ""}
              onMouseEnter={() => setSelected(i)}
              onClick={() => api.switchTask(task.id).catch((e) => setError(String(e)))}
            >
              <span className="dot" style={{ background: task.color }} />
              {task.name}
            </button>
          </li>
        ))}
        {filtered.length === 0 && query.trim() && (
          <li className="empty">No matches — Enter to create</li>
        )}
      </ul>
    </div>
  );
}
