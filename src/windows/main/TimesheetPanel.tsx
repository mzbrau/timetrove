import { useEffect, useState } from "react";
import { api } from "../../shared/api";
import {
  dayHeaderLabel,
  isoWeekParts,
  mondayOfIsoWeek,
  parseDay,
} from "../../shared/datetime";
import { formatDecimalHours } from "../../shared/format";
import type { Task, TimeSlice, Timesheet } from "../../shared/types";
import { useDemoShot } from "../../shared/useDemoShot";
import { DayTimeline } from "./DayTimeline";

type TimesheetTimeFormat = "hhmm" | "decimal";

const TIME_FORMAT_KEY = "timetrove.timesheetTimeFormat";

function loadTimeFormat(): TimesheetTimeFormat {
  try {
    const v = localStorage.getItem(TIME_FORMAT_KEY);
    if (v === "decimal" || v === "hhmm") return v;
  } catch {
    /* ignore */
  }
  return "hhmm";
}

function isWeekendDay(day: string): boolean {
  const dow = parseDay(day).getDay(); // 0=Sun … 6=Sat
  return dow === 0 || dow === 6;
}

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
  const [timeFormat, setTimeFormat] = useState<TimesheetTimeFormat>(loadTimeFormat);
  const [selectedTaskId, setSelectedTaskId] = useState<string | null>(null);

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

  const setFormat = (next: TimesheetTimeFormat) => {
    setTimeFormat(next);
    try {
      localStorage.setItem(TIME_FORMAT_KEY, next);
    } catch {
      /* ignore */
    }
  };

  const fmt = (ms: number, hhmm: string) =>
    timeFormat === "decimal" ? formatDecimalHours(ms) : hhmm;

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
          <div className="timesheet-format-toggle" role="group" aria-label="Time format">
            <button
              type="button"
              className={timeFormat === "hhmm" ? "active" : undefined}
              onClick={() => setFormat("hhmm")}
            >
              HH:MM
            </button>
            <button
              type="button"
              className={timeFormat === "decimal" ? "active" : undefined}
              onClick={() => setFormat("decimal")}
            >
              Decimal
            </button>
          </div>
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
                  <th
                    key={d}
                    className={isWeekendDay(d) ? "timesheet-weekend" : undefined}
                  >
                    {dayHeaderLabel(d)}
                  </th>
                ))}
                <th className="total-cell">Total</th>
              </tr>
            </thead>
            <tbody>
              {sheet.rows.map((row) => {
                const selected = selectedTaskId === row.task.id;
                return (
                  <tr
                    key={row.task.id}
                    className={selected ? "timesheet-row-selected" : undefined}
                    onClick={() =>
                      setSelectedTaskId((prev) =>
                        prev === row.task.id ? null : row.task.id,
                      )
                    }
                  >
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
                      const weekend = isWeekendDay(c.day);
                      return (
                        <td
                          key={c.day}
                          className={`mono timesheet-day-cell${
                            hasNotes ? " has-notes" : ""
                          }${weekend ? " timesheet-weekend" : ""}`}
                          title={hasNotes ? c.notes : undefined}
                        >
                          {c.durationMs > 0 ? (
                            <>
                              {fmt(c.durationMs, c.display)}
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
                    <td className="mono total-cell">
                      {fmt(row.totalMs, row.totalDisplay)}
                    </td>
                  </tr>
                );
              })}
            </tbody>
            <tfoot>
              <tr>
                <th className="total-cell">Total</th>
                {sheet.dayTotalsDisplay.map((d, i) => {
                  const day = sheet.days[i];
                  const ms = sheet.dayTotalsMs[i] ?? 0;
                  return (
                    <th
                      key={day}
                      className={`mono total-cell${
                        isWeekendDay(day) ? " timesheet-weekend" : ""
                      }`}
                    >
                      {fmt(ms, d)}
                    </th>
                  );
                })}
                <th className="mono total-cell">
                  {fmt(sheet.grandTotalMs, sheet.grandTotalDisplay)}
                </th>
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
