import { useEffect, useState } from "react";
import { api } from "../../shared/api";
import {
  dayHeaderLabel,
  isoWeekParts,
  mondayOfIsoWeek,
} from "../../shared/datetime";
import type { Task, TimeSlice, Timesheet } from "../../shared/types";
import { useDemoShot } from "../../shared/useDemoShot";
import { DayTimeline } from "./DayTimeline";

export function TimesheetPanel() {
  const [weekStart, setWeekStart] = useState<string>("");
  const [sheet, setSheet] = useState<Timesheet | null>(null);
  const [tasks, setTasks] = useState<Task[]>([]);
  const [slicesByDay, setSlicesByDay] = useState<Record<string, TimeSlice[]>>(
    {},
  );
  const [error, setError] = useState<string | null>(null);
  const [weekInput, setWeekInput] = useState("");
  const [yearInput, setYearInput] = useState("");
  const [nowMs, setNowMs] = useState(() => Date.now());

  const load = async (start?: string) => {
    const ws = start ?? (await api.currentWeekStart());
    setWeekStart(ws);
    const parts = isoWeekParts(ws);
    setWeekInput(String(parts.week));
    setYearInput(String(parts.year));
    const nextSheet = await api.getTimesheet(ws);
    setSheet(nextSheet);
    const [taskList, ...daySlices] = await Promise.all([
      api.listTasks(true),
      ...nextSheet.days.map((day) => api.listSlicesForDay(day)),
    ]);
    setTasks(taskList);
    const byDay: Record<string, TimeSlice[]> = {};
    nextSheet.days.forEach((day, i) => {
      byDay[day] = daySlices[i] ?? [];
    });
    setSlicesByDay(byDay);
  };

  useEffect(() => {
    load().catch((e) => setError(String(e)));
  }, []);

  useDemoShot((shot) => {
    const offset = shot.weekOffset;
    if (offset === undefined) return;
    void api
      .currentWeekStart()
      .then((ws) => api.shiftWeek(ws, offset))
      .then((ws) => load(ws))
      .catch((e) => setError(String(e)));
  });

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

  const jumpToWeekYear = async () => {
    const week = Number(weekInput);
    const year = Number(yearInput);
    if (
      !Number.isInteger(week) ||
      week < 1 ||
      week > 53 ||
      !Number.isInteger(year) ||
      year < 1970 ||
      year > 2100
    ) {
      setError("Enter a valid week (1–53) and year");
      return;
    }
    setError(null);
    const monday = mondayOfIsoWeek(week, year);
    const parts = isoWeekParts(monday);
    if (parts.week !== week || parts.year !== year) {
      setError("That ISO week does not exist for the chosen year");
      return;
    }
    await load(monday);
  };

  const copy = async () => {
    if (!sheet) return;
    await navigator.clipboard.writeText(sheet.csv);
  };

  const download = () => {
    if (!sheet) return;
    const blob = new Blob([sheet.csv], { type: "text/csv" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `timesheet-${sheet.weekStart}.csv`;
    a.click();
    URL.revokeObjectURL(url);
  };

  return (
    <div className="timesheet">
      <div className="timesheet-toolbar">
        <div className="row-actions">
          <button
            type="button"
            onClick={async () => {
              const prev = await api.shiftWeek(weekStart, -1);
              await load(prev);
            }}
          >
            Previous
          </button>
          <button
            type="button"
            onClick={async () => {
              await load(await api.currentWeekStart());
            }}
          >
            This week
          </button>
          <label className="week-field">
            Week
            <input
              type="number"
              min={1}
              max={53}
              value={weekInput}
              onChange={(e) => setWeekInput(e.target.value)}
              onBlur={() => void jumpToWeekYear()}
              onKeyDown={(e) => {
                if (e.key === "Enter") void jumpToWeekYear();
              }}
            />
          </label>
          <label className="week-field">
            Year
            <input
              type="number"
              min={1970}
              max={2100}
              value={yearInput}
              onChange={(e) => setYearInput(e.target.value)}
              onBlur={() => void jumpToWeekYear()}
              onKeyDown={(e) => {
                if (e.key === "Enter") void jumpToWeekYear();
              }}
            />
          </label>
        </div>
        <div className="row-actions">
          <button type="button" onClick={() => void copy()}>
            Copy
          </button>
          <button type="button" onClick={download}>
            CSV
          </button>
        </div>
      </div>
      {error && <p className="err">{error}</p>}
      {sheet && (
        <>
          <table className="timesheet-table">
            <thead>
              <tr>
                <th>Task</th>
                {sheet.days.map((d) => (
                  <th key={d}>{dayHeaderLabel(d)}</th>
                ))}
                <th className="total-cell">Total</th>
              </tr>
            </thead>
            <tbody>
              {sheet.rows.map((row) => (
                <tr key={row.task.id}>
                  <td>
                    <div className="task-cell">
                      <span
                        className="dot"
                        style={{ background: row.task.color }}
                      />
                      {row.task.name}
                    </div>
                  </td>
                  {row.cells.map((c) => {
                    const hasNotes = Boolean(c.notes?.trim());
                    return (
                      <td
                        key={c.day}
                        className={`mono timesheet-day-cell${
                          hasNotes ? " has-notes" : ""
                        }`}
                        title={hasNotes ? c.notes : undefined}
                      >
                        {c.durationMs > 0 ? (
                          <>
                            {c.display}
                            {hasNotes && (
                              <span
                                className="timesheet-note-mark"
                                aria-hidden
                              />
                            )}
                          </>
                        ) : (
                          "—"
                        )}
                      </td>
                    );
                  })}
                  <td className="mono total-cell">{row.totalDisplay}</td>
                </tr>
              ))}
            </tbody>
            <tfoot>
              <tr>
                <th className="total-cell">Total</th>
                {sheet.dayTotalsDisplay.map((d, i) => (
                  <th key={sheet.days[i]} className="mono total-cell">
                    {d}
                  </th>
                ))}
                <th className="mono total-cell">{sheet.grandTotalDisplay}</th>
              </tr>
            </tfoot>
          </table>

          <section className="timesheet-week-timelines">
            <h3>Week timeline</h3>
            {sheet.days.map((day) => (
              <div key={day} className="timesheet-day-timeline-row">
                <div className="timesheet-day-timeline-label">
                  {dayHeaderLabel(day)}
                </div>
                <DayTimeline
                  readOnly
                  day={day}
                  slices={slicesByDay[day] ?? []}
                  tasks={tasks}
                  nowMs={nowMs}
                />
              </div>
            ))}
          </section>
        </>
      )}
    </div>
  );
}
