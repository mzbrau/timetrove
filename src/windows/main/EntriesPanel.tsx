import { useEffect, useMemo, useState } from "react";
import { api } from "../../shared/api";
import {
  addDays,
  fromLocalInput,
  toLocalInput,
  todayStr,
  weekdayLong,
} from "../../shared/datetime";
import { formatHm, normalizeHexColor } from "../../shared/format";
import type { Task, TimeSlice, TrackerState } from "../../shared/types";
import { ConfirmBar } from "./ConfirmBar";
import { DayTimeline } from "./DayTimeline";
import { TaskPicker } from "./TaskPicker";

type RowMode =
  | { kind: "idle" }
  | { kind: "move"; taskId: string }
  | { kind: "split"; at: string }
  | { kind: "delete" };

function midpointLocal(startIso: string, endIso: string) {
  const mid =
    (new Date(startIso).getTime() + new Date(endIso).getTime()) / 2;
  return toLocalInput(new Date(mid).toISOString());
}

function defaultRangeForDay(day: string) {
  return {
    start: `${day}T09:00`,
    end: `${day}T10:00`,
  };
}

function sliceDurationMs(slice: TimeSlice, nowMs: number) {
  const end = slice.end ? new Date(slice.end).getTime() : nowMs;
  return Math.max(0, end - new Date(slice.start).getTime());
}

type Props = {
  state: TrackerState | null;
};

export function EntriesPanel({ state }: Props) {
  const [day, setDay] = useState(todayStr());
  const [tasks, setTasks] = useState<Task[]>([]);
  const [slices, setSlices] = useState<TimeSlice[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [showAdd, setShowAdd] = useState(false);
  const [addTaskId, setAddTaskId] = useState("");
  const [addStart, setAddStart] = useState(
    () => defaultRangeForDay(todayStr()).start,
  );
  const [addEnd, setAddEnd] = useState(
    () => defaultRangeForDay(todayStr()).end,
  );
  const [rowModes, setRowModes] = useState<Record<string, RowMode>>({});
  const [nowMs, setNowMs] = useState(() => Date.now());
  const [merging, setMerging] = useState(false);

  const activeTasks = useMemo(
    () => tasks.filter((t) => !t.archived),
    [tasks],
  );

  const load = async () => {
    const [t, s] = await Promise.all([
      api.listTasks(true),
      api.listSlicesForDay(day),
    ]);
    setTasks(t);
    setSlices(s);
    if (!addTaskId) {
      const first = t.find((x) => !x.archived);
      if (first) setAddTaskId(first.id);
    }
  };

  useEffect(() => {
    load().catch((e) => setError(String(e)));
  }, [day]);

  useEffect(() => {
    const range = defaultRangeForDay(day);
    setAddStart(range.start);
    setAddEnd(range.end);
  }, [day]);

  useEffect(() => {
    const tick = () => setNowMs(Date.now());
    tick();
    let interval: ReturnType<typeof setInterval> | null = null;

    const syncInterval = () => {
      if (interval) {
        clearInterval(interval);
        interval = null;
      }
      if (document.visibilityState === "visible") {
        tick();
        interval = setInterval(tick, 60_000);
      }
    };

    syncInterval();
    document.addEventListener("visibilitychange", syncInterval);
    return () => {
      if (interval) clearInterval(interval);
      document.removeEventListener("visibilitychange", syncInterval);
    };
  }, []);

  // Refresh sooner when tracker state changes (start/stop)
  useEffect(() => {
    setNowMs(Date.now());
    if (state) {
      load().catch((e) => setError(String(e)));
    }
  }, [state?.phase, state?.tracking]);

  const taskName = (id: string) =>
    tasks.find((t) => t.id === id)?.name ?? id.slice(0, 8);

  const taskColor = (id: string) =>
    normalizeHexColor(tasks.find((t) => t.id === id)?.color ?? "#0f766e");

  const summary = useMemo(() => {
    const byTask = new Map<string, number>();
    let total = 0;
    for (const slice of slices) {
      const ms = sliceDurationMs(slice, nowMs);
      total += ms;
      byTask.set(slice.taskId, (byTask.get(slice.taskId) ?? 0) + ms);
    }
    const perTask = [...byTask.entries()]
      .map(([taskId, ms]) => ({ taskId, ms }))
      .sort((a, b) => b.ms - a.ms);
    return { total, perTask };
  }, [slices, nowMs]);

  const addValid = useMemo(() => {
    if (!addTaskId || !addStart || !addEnd) return false;
    const start = new Date(fromLocalInput(addStart)).getTime();
    const end = new Date(fromLocalInput(addEnd)).getTime();
    return Number.isFinite(start) && Number.isFinite(end) && end > start;
  }, [addTaskId, addStart, addEnd]);

  const addDuration = useMemo(() => {
    if (!addValid) return null;
    const start = new Date(fromLocalInput(addStart)).getTime();
    const end = new Date(fromLocalInput(addEnd)).getTime();
    return formatHm(end - start);
  }, [addValid, addStart, addEnd]);

  const setMode = (id: string, mode: RowMode) => {
    setRowModes((m) => ({ ...m, [id]: mode }));
  };

  const clearMode = (id: string) => {
    setRowModes((m) => {
      const next = { ...m };
      delete next[id];
      return next;
    });
  };

  return (
    <div className="entries">
      <div className="entries-toolbar">
        <div className="row-actions entries-day-nav">
          <button type="button" onClick={() => setDay((d) => addDays(d, -1))}>
            Previous
          </button>
          <label className="field">
            Day
            <input
              type="date"
              value={day}
              onChange={(e) => setDay(e.target.value)}
            />
          </label>
          <button type="button" onClick={() => setDay((d) => addDays(d, 1))}>
            Next
          </button>
          <span className="entries-weekday">{weekdayLong(day)}</span>
        </div>
        <div className="row-actions">
          <button type="button" onClick={() => setDay(todayStr())}>
            Today
          </button>
          <button
            type="button"
            disabled={merging}
            onClick={async () => {
              setMerging(true);
              setError(null);
              try {
                await api.coalesceNearAdjacent();
                await load();
              } catch (e) {
                setError(String(e));
              } finally {
                setMerging(false);
              }
            }}
          >
            {merging ? "Merging…" : "Merge adjacent"}
          </button>
          <button type="button" onClick={() => setShowAdd((v) => !v)}>
            {showAdd ? "Hide add entry" : "Add entry"}
          </button>
        </div>
      </div>

      <div className="entries-summary">
        <div className="entries-summary-total mono">
          {formatHm(summary.total)}
        </div>
        <div className="entries-summary-tasks">
          {summary.perTask.length === 0 ? (
            <span className="muted">No time logged</span>
          ) : (
            summary.perTask.map(({ taskId, ms }) => (
              <span key={taskId} className="entries-summary-task">
                <span
                  className="dot"
                  style={{ backgroundColor: taskColor(taskId) }}
                />
                {taskName(taskId)} {formatHm(ms)}
              </span>
            ))
          )}
        </div>
      </div>

      {error && <p className="err">{error}</p>}

      {showAdd && (
        <div className="entries-toolbar add-entry-form">
          <label className="field">
            Task
            <TaskPicker
              tasks={activeTasks}
              value={addTaskId}
              onChange={setAddTaskId}
            />
          </label>
          <label className="field">
            Start
            <input
              type="datetime-local"
              value={addStart}
              onChange={(e) => setAddStart(e.target.value)}
            />
          </label>
          <label className="field">
            End
            <input
              type="datetime-local"
              value={addEnd}
              onChange={(e) => setAddEnd(e.target.value)}
            />
          </label>
          <label className="field">
            Duration
            <span className="mono duration-readout">
              {addDuration ?? "—"}
            </span>
          </label>
          <button
            type="button"
            disabled={!addValid}
            onClick={async () => {
              try {
                setError(null);
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
      )}

      <DayTimeline
        day={day}
        slices={slices}
        tasks={tasks}
        nowMs={nowMs}
        onChanged={async () => {
          setError(null);
          await load();
        }}
        onError={(message) => setError(message)}
      />

      <h3>Entries</h3>
      <div className="entries-table-wrap">
        <table>
          <thead>
            <tr>
              <th>Task</th>
              <th>Start</th>
              <th>End</th>
              <th>Duration</th>
              <th>Actions</th>
            </tr>
          </thead>
          <tbody>
            {slices.map((slice) => {
              const mode = rowModes[slice.id] ?? { kind: "idle" };
              const dur = sliceDurationMs(slice, nowMs);
              return (
                <tr key={slice.id}>
                  <td>
                    <span className="task-cell">
                      <span
                        className="dot"
                        style={{ backgroundColor: taskColor(slice.taskId) }}
                      />
                      {taskName(slice.taskId)}
                    </span>
                  </td>
                  <td className="mono">{toLocalInput(slice.start)}</td>
                  <td className="mono">
                    {slice.end ? toLocalInput(slice.end) : "running"}
                  </td>
                  <td className="mono">{formatHm(dur)}</td>
                  <td>
                    {mode.kind === "idle" && (
                      <div className="row-actions">
                        {slice.end && (
                          <>
                            <button
                              type="button"
                              onClick={() =>
                                setMode(slice.id, {
                                  kind: "move",
                                  taskId: slice.taskId,
                                })
                              }
                            >
                              Move
                            </button>
                            <button
                              type="button"
                              onClick={() =>
                                setMode(slice.id, {
                                  kind: "split",
                                  at: midpointLocal(slice.start, slice.end!),
                                })
                              }
                            >
                              Split
                            </button>
                          </>
                        )}
                        <button
                          type="button"
                          onClick={() =>
                            setMode(slice.id, { kind: "delete" })
                          }
                        >
                          Delete
                        </button>
                      </div>
                    )}
                    {mode.kind === "split" && (
                      <div className="inline-action">
                        <label className="field">
                          Split at
                          <input
                            type="datetime-local"
                            value={mode.at}
                            onChange={(e) =>
                              setMode(slice.id, {
                                kind: "split",
                                at: e.target.value,
                              })
                            }
                          />
                        </label>
                        <div className="row-actions">
                          <button
                            type="button"
                            className="btn"
                            onClick={async () => {
                              try {
                                setError(null);
                                await api.splitSlice({
                                  id: slice.id,
                                  at: fromLocalInput(mode.at),
                                });
                                clearMode(slice.id);
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
                            onClick={() => clearMode(slice.id)}
                          >
                            Cancel
                          </button>
                        </div>
                      </div>
                    )}
                    {mode.kind === "move" && (
                      <div className="inline-action">
                        <TaskPicker
                          tasks={activeTasks}
                          value={mode.taskId}
                          onChange={(taskId) =>
                            setMode(slice.id, { kind: "move", taskId })
                          }
                        />
                        <div className="row-actions">
                          <button
                            type="button"
                            className="btn"
                            disabled={mode.taskId === slice.taskId}
                            onClick={async () => {
                              try {
                                setError(null);
                                await api.moveSlice(slice.id, mode.taskId);
                                clearMode(slice.id);
                                await load();
                              } catch (e) {
                                setError(String(e));
                              }
                            }}
                          >
                            Move
                          </button>
                          <button
                            type="button"
                            onClick={() => clearMode(slice.id)}
                          >
                            Cancel
                          </button>
                        </div>
                      </div>
                    )}
                    {mode.kind === "delete" && (
                      <ConfirmBar
                        message="Delete this entry?"
                        confirmLabel="Delete"
                        onConfirm={async () => {
                          try {
                            setError(null);
                            await api.deleteSlice(slice.id);
                            clearMode(slice.id);
                            await load();
                          } catch (e) {
                            setError(String(e));
                          }
                        }}
                        onCancel={() => clearMode(slice.id)}
                      />
                    )}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </div>
  );
}
