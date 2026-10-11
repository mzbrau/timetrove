import { useEffect, useState } from "react";
import { api } from "../../shared/api";
import {
  dayHeaderLabel,
  isoWeekParts,
  mondayOfIsoWeek,
} from "../../shared/datetime";
import type { Timesheet } from "../../shared/types";

export function TimesheetPanel() {
  const [weekStart, setWeekStart] = useState<string>("");
  const [sheet, setSheet] = useState<Timesheet | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [weekInput, setWeekInput] = useState("");
  const [yearInput, setYearInput] = useState("");

  const load = async (start?: string) => {
    const ws = start ?? (await api.currentWeekStart());
    setWeekStart(ws);
    const parts = isoWeekParts(ws);
    setWeekInput(String(parts.week));
    setYearInput(String(parts.year));
    setSheet(await api.getTimesheet(ws));
  };

  useEffect(() => {
    load().catch((e) => setError(String(e)));
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
                {row.cells.map((c) => (
                  <td key={c.day} className="mono">
                    {c.durationMs > 0 ? c.display : "—"}
                  </td>
                ))}
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
      )}
    </div>
  );
}
