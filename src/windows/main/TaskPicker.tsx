import { useEffect, useMemo, useRef, useState } from "react";
import type { Task } from "../../shared/types";

type Props = {
  tasks: Task[];
  value: string;
  onChange: (taskId: string) => void;
  placeholder?: string;
  disabled?: boolean;
};

export function TaskPicker({
  tasks,
  value,
  onChange,
  placeholder = "Filter tasks…",
  disabled,
}: Props) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const rootRef = useRef<HTMLDivElement>(null);

  const selected = tasks.find((t) => t.id === value) ?? null;

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return tasks;
    return tasks.filter((t) => t.name.toLowerCase().includes(q));
  }, [tasks, query]);

  useEffect(() => {
    if (!open) setQuery("");
  }, [open]);

  useEffect(() => {
    const onDoc = (e: MouseEvent) => {
      if (!rootRef.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", onDoc);
    return () => document.removeEventListener("mousedown", onDoc);
  }, []);

  return (
    <div className="task-picker" ref={rootRef}>
      <button
        type="button"
        className="task-picker-trigger"
        disabled={disabled}
        onClick={() => setOpen((o) => !o)}
      >
        {selected ? (
          <>
            <span className="dot" style={{ background: selected.color }} />
            <span>{selected.name}</span>
          </>
        ) : (
          <span className="muted">Select task</span>
        )}
      </button>
      {open && (
        <div className="task-picker-menu">
          <input
            autoFocus
            type="text"
            placeholder={placeholder}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Escape") setOpen(false);
              if (e.key === "Enter" && filtered[0]) {
                onChange(filtered[0].id);
                setOpen(false);
              }
            }}
          />
          <ul>
            {filtered.length === 0 && (
              <li className="muted">No matching tasks</li>
            )}
            {filtered.map((task) => (
              <li key={task.id}>
                <button
                  type="button"
                  className={task.id === value ? "selected" : ""}
                  onClick={() => {
                    onChange(task.id);
                    setOpen(false);
                  }}
                >
                  <span className="dot" style={{ background: task.color }} />
                  {task.name}
                </button>
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}
