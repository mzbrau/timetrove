import { useEffect, useState } from "react";
import { api } from "../../shared/api";
import { fromLocalInput, toLocalInput, todayStr } from "../../shared/datetime";
import type { Task, TimeSlice } from "../../shared/types";

export function EntriesPanel() {
  const [day, setDay] = useState(todayStr());
  const [tasks, setTasks] = useState<Task[]>([]);
  const [slices, setSlices] = useState<TimeSlice[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [addTaskId, setAddTaskId] = useState("");
  const [addStart, setAddStart] = useState("");
  const [addEnd, setAddEnd] = useState("");

  const load = async () => {
    const [t, s] = await Promise.all([
      api.listTasks(true),
      api.listSlicesForDay(day),
    ]);
    setTasks(t);
    setSlices(s);
    if (!addTaskId && t[0]) setAddTaskId(t[0].id);
  };

  useEffect(() => {
    load().catch((e) => setError(String(e)));
  }, [day]);

  const taskName = (id: string) =>
    tasks.find((t) => t.id === id)?.name ?? id.slice(0, 8);

  return (
    <div className="entries">
      <div className="entries-toolbar">
        <label>
          Day
          <input
            type="date"
            value={day}
            onChange={(e) => setDay(e.target.value)}
          />
        </label>
      </div>
      {error && <p className="err">{error}</p>}

      <h3>Add entry</h3>
      <div className="entries-toolbar">
        <label>
          Task
          <select
            value={addTaskId}
            onChange={(e) => setAddTaskId(e.target.value)}
          >
            {tasks
              .filter((t) => !t.archived)
              .map((t) => (
                <option key={t.id} value={t.id}>
                  {t.name}
                </option>
              ))}
          </select>
        </label>
        <label>
          Start
          <input
            type="datetime-local"
            value={addStart}
            onChange={(e) => setAddStart(e.target.value)}
          />
        </label>
        <label>
          End
          <input
            type="datetime-local"
            value={addEnd}
            onChange={(e) => setAddEnd(e.target.value)}
          />
        </label>
        <button
          type="button"
          onClick={async () => {
            try {
              await api.createSlice({
                taskId: addTaskId,
                start: fromLocalInput(addStart),
                end: fromLocalInput(addEnd),
              });
              await load();
            } catch (e) {
              setError(String(e));
            }
          }}
        >
          Add
        </button>
      </div>

      <h3>Entries</h3>
      <table>
        <thead>
          <tr>
            <th>Task</th>
            <th>Start</th>
            <th>End</th>
            <th>Actions</th>
          </tr>
        </thead>
        <tbody>
          {slices.map((slice) => (
            <tr key={slice.id}>
              <td>{taskName(slice.taskId)}</td>
              <td className="mono">{toLocalInput(slice.start)}</td>
              <td className="mono">
                {slice.end ? toLocalInput(slice.end) : "running"}
              </td>
              <td>
                <div className="row-actions">
                  {slice.end && (
                    <>
                      <button
                        type="button"
                        onClick={async () => {
                          const start = prompt(
                            "Start (local)",
                            toLocalInput(slice.start),
                          );
                          const end = prompt(
                            "End (local)",
                            toLocalInput(slice.end!),
                          );
                          if (!start || !end) return;
                          try {
                            await api.updateSlice({
                              id: slice.id,
                              start: fromLocalInput(start),
                              end: fromLocalInput(end),
                            });
                            await load();
                          } catch (e) {
                            setError(String(e));
                          }
                        }}
                      >
                        Edit
                      </button>
                      <button
                        type="button"
                        onClick={async () => {
                          const at = prompt(
                            "Split at (local)",
                            toLocalInput(slice.start),
                          );
                          if (!at) return;
                          try {
                            await api.splitSlice({
                              id: slice.id,
                              at: fromLocalInput(at),
                            });
                            await load();
                          } catch (e) {
                            setError(String(e));
                          }
                        }}
                      >
                        Split
                      </button>
                      <button
                        type="button"
                        onClick={async () => {
                          const name = prompt(
                            "Move to task name",
                            taskName(slice.taskId),
                          );
                          if (!name) return;
                          const task = tasks.find(
                            (t) =>
                              t.name.toLowerCase() === name.trim().toLowerCase(),
                          );
                          if (!task) {
                            setError("Task not found");
                            return;
                          }
                          try {
                            await api.moveSlice(slice.id, task.id);
                            await load();
                          } catch (e) {
                            setError(String(e));
                          }
                        }}
                      >
                        Move
                      </button>
                    </>
                  )}
                  <button
                    type="button"
                    onClick={async () => {
                      if (!confirm("Delete this entry?")) return;
                      try {
                        await api.deleteSlice(slice.id);
                        await load();
                      } catch (e) {
                            setError(String(e));
                      }
                    }}
                  >
                    Delete
                  </button>
                </div>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
