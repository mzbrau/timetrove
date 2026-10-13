import { useEffect, useMemo, useState, type CSSProperties } from "react";
import {
  Bar,
  BarChart,
  CartesianGrid,
  Cell,
  Line,
  LineChart,
  Pie,
  PieChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import { api } from "../../shared/api";
import { addDays, formatDay, todayStr } from "../../shared/datetime";
import { formatHm, normalizeHexColor } from "../../shared/format";
import type { InsightsReport } from "../../shared/types";

type Preset = "lastWeek" | "lastMonth" | "lastYear" | "month" | "year" | "custom";

const WEEKDAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"] as const;
const MONTHS = [
  "January",
  "February",
  "March",
  "April",
  "May",
  "June",
  "July",
  "August",
  "September",
  "October",
  "November",
  "December",
] as const;

function trailingDays(n: number): { start: string; end: string } {
  const end = todayStr();
  return { start: addDays(end, -(n - 1)), end };
}

function monthRange(year: number, monthIndex: number): { start: string; end: string } {
  const start = formatDay(new Date(year, monthIndex, 1, 12));
  const end = formatDay(new Date(year, monthIndex + 1, 0, 12));
  return { start, end };
}

function yearRange(year: number): { start: string; end: string } {
  return {
    start: `${year}-01-01`,
    end: `${year}-12-31`,
  };
}

function formatClockMins(mins: number | null | undefined): string {
  if (mins == null || !Number.isFinite(mins)) return "—";
  const h = Math.floor(mins / 60) % 24;
  const m = mins % 60;
  return `${String(h).padStart(2, "0")}:${String(m).padStart(2, "0")}`;
}

function formatHours(ms: number | null | undefined): string {
  if (ms == null || !Number.isFinite(ms)) return "—";
  return formatHm(ms);
}

function cssVar(name: string, fallback: string): string {
  if (typeof document === "undefined") return fallback;
  const v = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  return v || fallback;
}

function resolveRange(
  preset: Preset,
  month: number,
  year: number,
  customStart: string,
  customEnd: string,
): { start: string; end: string } {
  switch (preset) {
    case "lastWeek":
      return trailingDays(7);
    case "lastMonth":
      return trailingDays(30);
    case "lastYear":
      return trailingDays(365);
    case "month":
      return monthRange(year, month);
    case "year":
      return yearRange(year);
    case "custom": {
      const start = customStart || todayStr();
      const end = customEnd || todayStr();
      return start <= end ? { start, end } : { start: end, end: start };
    }
  }
}

export function InsightsPanel() {
  const now = new Date();
  const [preset, setPreset] = useState<Preset>("lastYear");
  const [month, setMonth] = useState(now.getMonth());
  const [year, setYear] = useState(now.getFullYear());
  const [customStart, setCustomStart] = useState(() => addDays(todayStr(), -30));
  const [customEnd, setCustomEnd] = useState(todayStr);
  const [report, setReport] = useState<InsightsReport | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const range = useMemo(
    () => resolveRange(preset, month, year, customStart, customEnd),
    [preset, month, year, customStart, customEnd],
  );

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    void api
      .getInsights(range.start, range.end)
      .then((r) => {
        if (!cancelled) {
          setReport(r);
          setLoading(false);
        }
      })
      .catch((e) => {
        if (!cancelled) {
          setError(String(e));
          setReport(null);
          setLoading(false);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [range.start, range.end]);

  const ink = cssVar("--ink", "#e8eef2");
  const muted = cssVar("--muted", "#8b9aab");
  const line = cssVar("--line", "rgba(232,238,242,0.1)");
  const accent = cssVar("--accent", "#3d9b8f");

  const taskChartData = useMemo(
    () =>
      (report?.tasks ?? []).map((t) => ({
        name: t.name,
        totalMs: t.totalMs,
        hours: Math.round((t.totalMs / 3_600_000) * 100) / 100,
        color: normalizeHexColor(t.color),
      })),
    [report],
  );

  const weekdayData = useMemo(
    () =>
      WEEKDAYS.map((name, i) => {
        const ms = report?.avgByWeekdayMs[i] ?? null;
        return {
          name,
          hours: ms == null ? 0 : Math.round((ms / 3_600_000) * 100) / 100,
          hasData: ms != null,
        };
      }),
    [report],
  );

  const weeklyData = useMemo(
    () =>
      (report?.weekly ?? []).map((w) => ({
        week: w.weekStart.slice(5), // MM-DD
        hours: Math.round((w.totalMs / 3_600_000) * 100) / 100,
      })),
    [report],
  );

  const hourData = useMemo(
    () =>
      (report?.byHourMs ?? Array.from({ length: 24 }, () => 0)).map((ms, hour) => ({
        hour: String(hour).padStart(2, "0"),
        hours: Math.round((ms / 3_600_000) * 100) / 100,
      })),
    [report],
  );

  const hasData = (report?.totalMs ?? 0) > 0;

  return (
    <div className="insights">
      <div className="insights-toolbar">
        <div className="row-actions">
          {(
            [
              ["lastWeek", "Last week"],
              ["lastMonth", "Last month"],
              ["lastYear", "Last year"],
              ["month", "Month"],
              ["year", "Year"],
              ["custom", "Custom"],
            ] as const
          ).map(([id, label]) => (
            <button
              key={id}
              type="button"
              className={preset === id ? "active" : ""}
              onClick={() => setPreset(id)}
            >
              {label}
            </button>
          ))}
        </div>
        <div className="row-actions insights-period-fields">
          {(preset === "month" || preset === "year") && (
            <label className="week-field">
              Year
              <input
                type="number"
                min={2000}
                max={2100}
                value={year}
                onChange={(e) => setYear(Number(e.target.value) || now.getFullYear())}
              />
            </label>
          )}
          {preset === "month" && (
            <label className="week-field">
              Month
              <select
                value={month}
                onChange={(e) => setMonth(Number(e.target.value))}
              >
                {MONTHS.map((label, i) => (
                  <option key={label} value={i}>
                    {label}
                  </option>
                ))}
              </select>
            </label>
          )}
          {preset === "custom" && (
            <>
              <label className="field">
                From
                <input
                  type="date"
                  value={customStart}
                  onChange={(e) => setCustomStart(e.target.value)}
                />
              </label>
              <label className="field">
                To
                <input
                  type="date"
                  value={customEnd}
                  onChange={(e) => setCustomEnd(e.target.value)}
                />
              </label>
            </>
          )}
          <span className="muted insights-range-label">
            {range.start} → {range.end}
          </span>
        </div>
      </div>

      {error && <p className="err">{error}</p>}
      {loading && <p className="muted">Loading insights…</p>}

      {!loading && !error && report && !hasData && (
        <div className="insights-empty">
          <p>No tracked time in this period.</p>
          <p className="muted">Try a wider range or start tracking a task.</p>
        </div>
      )}

      {!loading && !error && report && hasData && (
        <div className="insights-body">
          <section className="insights-kpis" aria-label="Summary metrics">
            <Kpi label="Total worked" value={formatHours(report.totalMs)} />
            <Kpi label="Avg / day" value={formatHours(report.avgDayMs)} hint="≥2h days" />
            <Kpi label="Max day" value={formatHours(report.maxDayMs)} />
            <Kpi label="Days worked" value={String(report.qualifyingDays)} hint="≥2h" />
            <Kpi
              label="Goal hit rate"
              value={
                report.goalHitRate == null
                  ? "—"
                  : `${Math.round(report.goalHitRate * 100)}%`
              }
            />
            <Kpi label="Avg lunch" value={formatHours(report.avgLunchMs)} hint="10:00–14:00" />
            <Kpi label="Discarded" value={formatHours(report.discardedMs)} />
            <Kpi label="Avg session" value={formatHours(report.avgSessionMs)} />
          </section>

          <section className="insights-schedule" aria-label="Schedule">
            <div className="insights-schedule-stats">
              <Kpi label="Earliest start" value={formatClockMins(report.earliestStartMins)} />
              <Kpi label="Avg start" value={formatClockMins(report.avgStartMins)} />
              <Kpi label="Avg finish" value={formatClockMins(report.avgFinishMins)} />
              <Kpi label="Latest finish" value={formatClockMins(report.latestFinishMins)} />
            </div>
            <div className="insights-chart-card">
              <h3>Average hours by weekday</h3>
              <ResponsiveContainer width="100%" height={220}>
                <BarChart data={weekdayData} margin={{ top: 8, right: 8, left: 0, bottom: 0 }}>
                  <CartesianGrid stroke={line} vertical={false} />
                  <XAxis dataKey="name" tick={{ fill: muted, fontSize: 12 }} axisLine={false} />
                  <YAxis
                    tick={{ fill: muted, fontSize: 12 }}
                    axisLine={false}
                    width={36}
                    unit="h"
                  />
                  <Tooltip
                    formatter={(v) => [`${v}h`, "Avg"]}
                    contentStyle={tooltipStyle(ink, line)}
                  />
                  <Bar dataKey="hours" radius={[6, 6, 0, 0]}>
                    {weekdayData.map((d) => (
                      <Cell
                        key={d.name}
                        fill={d.hasData ? accent : "transparent"}
                        stroke={d.hasData ? accent : line}
                      />
                    ))}
                  </Bar>
                </BarChart>
              </ResponsiveContainer>
            </div>
          </section>

          <section className="insights-tasks" aria-label="Time by task">
            <div className="insights-chart-card">
              <h3>Time by task</h3>
              <div className="insights-pie-wrap">
                <ResponsiveContainer width="100%" height={260}>
                  <PieChart>
                    <Pie
                      data={taskChartData}
                      dataKey="hours"
                      nameKey="name"
                      cx="50%"
                      cy="50%"
                      innerRadius={55}
                      outerRadius={95}
                      paddingAngle={2}
                    >
                      {taskChartData.map((t) => (
                        <Cell key={t.name} fill={t.color} stroke="transparent" />
                      ))}
                    </Pie>
                    <Tooltip
                      formatter={(v, name) => [`${v}h`, String(name)]}
                      contentStyle={tooltipStyle(ink, line)}
                    />
                  </PieChart>
                </ResponsiveContainer>
                <ul className="insights-legend">
                  {taskChartData.map((t) => (
                    <li key={t.name}>
                      <span className="dot" style={{ background: t.color }} />
                      <span className="insights-legend-name">{t.name}</span>
                      <span className="mono muted">{formatHm(t.totalMs)}</span>
                    </li>
                  ))}
                </ul>
              </div>
            </div>
            <div className="insights-chart-card">
              <h3>Hours per task</h3>
              <ResponsiveContainer
                width="100%"
                height={Math.max(220, taskChartData.length * 36)}
              >
                <BarChart
                  data={taskChartData}
                  layout="vertical"
                  margin={{ top: 8, right: 16, left: 8, bottom: 0 }}
                >
                  <CartesianGrid stroke={line} horizontal={false} />
                  <XAxis
                    type="number"
                    tick={{ fill: muted, fontSize: 12 }}
                    axisLine={false}
                    unit="h"
                  />
                  <YAxis
                    type="category"
                    dataKey="name"
                    width={100}
                    tick={{ fill: ink, fontSize: 12 }}
                    axisLine={false}
                  />
                  <Tooltip
                    formatter={(v) => [`${v}h`, "Total"]}
                    contentStyle={tooltipStyle(ink, line)}
                  />
                  <Bar dataKey="hours" radius={[0, 6, 6, 0]}>
                    {taskChartData.map((t) => (
                      <Cell key={t.name} fill={t.color} />
                    ))}
                  </Bar>
                </BarChart>
              </ResponsiveContainer>
            </div>
          </section>

          <section className="insights-trends" aria-label="Trends">
            <div className="insights-chart-card">
              <h3>Weekly hours</h3>
              <ResponsiveContainer width="100%" height={240}>
                <LineChart data={weeklyData} margin={{ top: 8, right: 12, left: 0, bottom: 0 }}>
                  <CartesianGrid stroke={line} vertical={false} />
                  <XAxis dataKey="week" tick={{ fill: muted, fontSize: 11 }} axisLine={false} />
                  <YAxis
                    tick={{ fill: muted, fontSize: 12 }}
                    axisLine={false}
                    width={36}
                    unit="h"
                  />
                  <Tooltip
                    formatter={(v) => [`${v}h`, "Hours"]}
                    contentStyle={tooltipStyle(ink, line)}
                  />
                  <Line
                    type="monotone"
                    dataKey="hours"
                    stroke={accent}
                    strokeWidth={2.5}
                    dot={{ r: 3, fill: accent }}
                    activeDot={{ r: 5 }}
                  />
                </LineChart>
              </ResponsiveContainer>
            </div>
            <div className="insights-chart-card">
              <h3>Hours by time of day</h3>
              <ResponsiveContainer width="100%" height={240}>
                <BarChart data={hourData} margin={{ top: 8, right: 8, left: 0, bottom: 0 }}>
                  <CartesianGrid stroke={line} vertical={false} />
                  <XAxis dataKey="hour" tick={{ fill: muted, fontSize: 10 }} axisLine={false} />
                  <YAxis
                    tick={{ fill: muted, fontSize: 12 }}
                    axisLine={false}
                    width={36}
                    unit="h"
                  />
                  <Tooltip
                    formatter={(v) => [`${v}h`, "Booked"]}
                    contentStyle={tooltipStyle(ink, line)}
                  />
                  <Bar dataKey="hours" fill={accent} radius={[4, 4, 0, 0]} />
                </BarChart>
              </ResponsiveContainer>
            </div>
          </section>
        </div>
      )}
    </div>
  );
}

function Kpi({
  label,
  value,
  hint,
}: {
  label: string;
  value: string;
  hint?: string;
}) {
  return (
    <div className="insights-kpi">
      <div className="insights-kpi-label">
        {label}
        {hint ? <span className="muted"> · {hint}</span> : null}
      </div>
      <div className="insights-kpi-value mono">{value}</div>
    </div>
  );
}

function tooltipStyle(ink: string, line: string): CSSProperties {
  return {
    background: cssVar("--bg-elevated", "#1a2128"),
    border: `1px solid ${line}`,
    borderRadius: 8,
    color: ink,
    fontSize: 12,
  };
}
