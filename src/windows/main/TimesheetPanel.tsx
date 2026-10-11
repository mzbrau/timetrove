import { useEffect, useState } from "react";
import { api } from "../../shared/api";
import type { Timesheet } from "../../shared/types";

export function TimesheetPanel() {
  const [weekStart, setWeekStart] = useState<string>("");
  const [sheet, setSheet] = useState<Timesheet | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = async (start?: string) => {
    const ws = start ?? (await api.currentWeekStart());
    setWeekStart(ws);
    setSheet(await api.getTimesheet(ws));
  };

  useEffect(() => {
    load().catch((e) => setError(String(e)));
  }, []);

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
    <div>
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
          <button
            type="button"
            onClick={async () => {
              const next = await api.shiftWeek(weekStart, 1);
              await load(next);
            }}
          >
            Next
          </button>
        </div>
        <div className="row-actions">
          <strong className="mono">{weekStart}</strong>
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
                <th key={d}>{d.slice(5)}</th>
              ))}
              <th>Total</th>
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
                <td className="mono">{row.totalDisplay}</td>
              </tr>
            ))}
          </tbody>
          <tfoot>
            <tr>
              <th>Total</th>
              {sheet.dayTotalsDisplay.map((d, i) => (
                <th key={sheet.days[i]} className="mono">
                  {d}
                </th>
              ))}
              <th className="mono">{sheet.grandTotalDisplay}</th>
            </tr>
          </tfoot>
        </table>
      )}
    </div>
  );
}
