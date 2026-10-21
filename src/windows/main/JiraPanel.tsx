import { useCallback, useEffect, useMemo, useState } from "react";
import {
  Bar,
  BarChart,
  Cell,
  Pie,
  PieChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import { api } from "../../shared/api";
import { formatHm } from "../../shared/format";
import { jiraPercentComplete } from "../../shared/jira";
import type { JiraAssignedIssuesResult, JiraDashboardIssue } from "../../shared/types";

function cssVar(name: string, fallback: string): string {
  if (typeof document === "undefined") return fallback;
  const v = getComputedStyle(document.documentElement)
    .getPropertyValue(name)
    .trim();
  return v || fallback;
}

function fmtDate(iso: string | null | undefined): string {
  if (!iso) return "—";
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? iso : d.toLocaleString();
}

function fmtHoursFromSeconds(s: number | null | undefined): string {
  if (s == null || !Number.isFinite(s)) return "—";
  return formatHm(s * 1000);
}

function fmtPct(n: number | null): string {
  if (n == null || !Number.isFinite(n)) return "—";
  return `${Math.round(n)}%`;
}

const STATUS_COLORS = [
  "#3d9b8f",
  "#d4a054",
  "#5b8def",
  "#c44e52",
  "#8b7ec8",
  "#6b8f71",
  "#e07a6a",
  "#7a9e9f",
];

type Props = {
  /** True while this tab is the active main-window tab. */
  active: boolean;
};

export function JiraPanel({ active }: Props) {
  const [result, setResult] = useState<JiraAssignedIssuesResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async (force: boolean) => {
    setLoading(true);
    setError(null);
    try {
      const next = await api.listAssignedJiraIssues(force);
      setResult(next);
      if (next.error) setError(next.error);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (!active) return;
    void load(false);
  }, [active, load]);

  const issues = result?.issues ?? [];

  const hoursChartData = useMemo(() => {
    const ranked = [...issues].sort((a, b) => {
      const aScore = Math.max(
        a.timetroveBookedMs ?? 0,
        (a.totalSpentSeconds ?? 0) * 1000,
      );
      const bScore = Math.max(
        b.timetroveBookedMs ?? 0,
        (b.totalSpentSeconds ?? 0) * 1000,
      );
      return bScore - aScore;
    });
    return ranked.slice(0, 10).map((issue) => ({
      key: issue.issueKey,
      label: issue.issueKey,
      timetroveHours: (issue.timetroveBookedMs ?? 0) / 3_600_000,
      jiraHours: (issue.totalSpentSeconds ?? 0) / 3600,
    }));
  }, [issues]);

  const statusChartData = useMemo(() => {
    const counts = new Map<string, number>();
    for (const issue of issues) {
      const status = issue.status.trim() || "Unknown";
      counts.set(status, (counts.get(status) ?? 0) + 1);
    }
    return [...counts.entries()]
      .map(([name, value]) => ({ name, value }))
      .sort((a, b) => b.value - a.value);
  }, [issues]);

  const accent = cssVar("--accent", "#3d9b8f");
  const ink = cssVar("--ink", "#1a2128");
  const muted = cssVar("--muted", "#6b7785");

  const openIssue = (issue: JiraDashboardIssue) => {
    if (!issue.browseUrl) return;
    void api.openExternalUrl(issue.browseUrl);
  };

  return (
    <div className="jira-panel">
      <div className="jira-toolbar">
        <div className="jira-toolbar-meta">
          {loading && (
            <span className="jira-spinner" aria-label="Refreshing" title="Refreshing…" />
          )}
          <span className="muted mono">
            {result?.fetchedAt
              ? `Updated ${fmtDate(result.fetchedAt)}`
              : loading
                ? "Loading…"
                : "Not loaded yet"}
            {result?.fromCache && !loading ? " · cache" : ""}
          </span>
        </div>
        <button
          type="button"
          disabled={loading}
          onClick={() => void load(true)}
        >
          {loading ? "Refreshing…" : "Refresh"}
        </button>
      </div>

      {error && (
        <p className="err">
          {error}{" "}
          <button type="button" className="linkish" onClick={() => void load(true)}>
            Retry
          </button>
        </p>
      )}

      {!loading && issues.length === 0 && !error && (
        <div className="jira-empty">
          <p>No open issues assigned to you in Jira.</p>
        </div>
      )}

      {issues.length > 0 && (
        <>
          {result?.truncated && (
            <p className="muted jira-cap-note">Showing up to 50 open issues</p>
          )}

          <div className="jira-table-wrap">
            <table className="jira-table">
              <thead>
                <tr>
                  <th />
                  <th>Key</th>
                  <th>Summary</th>
                  <th>Status</th>
                  <th>Estimate</th>
                  <th>TimeTrove</th>
                  <th>Jira spent</th>
                  <th>%</th>
                  <th>Task</th>
                </tr>
              </thead>
              <tbody>
                {issues.map((issue) => {
                  const pct = jiraPercentComplete(
                    issue.estimateSeconds,
                    issue.timetroveBookedMs,
                    issue.totalSpentSeconds,
                  );
                  return (
                    <tr key={issue.issueKey}>
                      <td>
                        <button
                          type="button"
                          className="jira-open-btn"
                          title={`Open ${issue.issueKey} in browser`}
                          onClick={() => openIssue(issue)}
                          disabled={!issue.browseUrl}
                        >
                          ↗
                        </button>
                      </td>
                      <td className="mono">
                        <div className="jira-key-cell">
                          <button
                            type="button"
                            className="linkish"
                            onClick={() => openIssue(issue)}
                            disabled={!issue.browseUrl}
                          >
                            {issue.issueKey}
                          </button>
                          <button
                            type="button"
                            className="jira-key-copy"
                            title="Copy key"
                            aria-label={`Copy ${issue.issueKey}`}
                            onClick={(e) => {
                              e.stopPropagation();
                              void navigator.clipboard.writeText(issue.issueKey);
                            }}
                          >
                            Copy
                          </button>
                        </div>
                      </td>
                      <td>{issue.summary || "—"}</td>
                      <td>{issue.status || "—"}</td>
                      <td className="mono">
                        {fmtHoursFromSeconds(issue.estimateSeconds)}
                      </td>
                      <td className="mono">
                        {issue.timetroveBookedMs != null
                          ? formatHm(issue.timetroveBookedMs)
                          : "—"}
                      </td>
                      <td className="mono">
                        {fmtHoursFromSeconds(issue.totalSpentSeconds)}
                      </td>
                      <td className="mono">{fmtPct(pct)}</td>
                      <td>{issue.taskName ?? "—"}</td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>

          <div className="jira-charts">
            <section className="jira-chart-card">
              <h3>Hours by issue</h3>
              <div className="jira-chart-wrap">
                <ResponsiveContainer width="100%" height={Math.max(220, hoursChartData.length * 28)}>
                  <BarChart
                    data={hoursChartData}
                    layout="vertical"
                    margin={{ top: 4, right: 16, left: 8, bottom: 4 }}
                  >
                    <XAxis type="number" tick={{ fill: muted, fontSize: 11 }} />
                    <YAxis
                      type="category"
                      dataKey="label"
                      width={72}
                      tick={{ fill: ink, fontSize: 11 }}
                    />
                    <Tooltip
                      formatter={(value, name) => [
                        `${Number(value ?? 0).toFixed(2)} h`,
                        name === "timetroveHours" ? "TimeTrove" : "Jira",
                      ]}
                    />
                    <Bar dataKey="timetroveHours" name="TimeTrove" fill={accent} />
                    <Bar
                      dataKey="jiraHours"
                      name="Jira"
                      fill="rgba(0, 82, 204, 0.75)"
                    />
                  </BarChart>
                </ResponsiveContainer>
              </div>
            </section>

            <section className="jira-chart-card">
              <h3>By status</h3>
              <div className="jira-chart-wrap jira-pie-wrap">
                <ResponsiveContainer width="100%" height={220}>
                  <PieChart>
                    <Pie
                      data={statusChartData}
                      dataKey="value"
                      nameKey="name"
                      cx="50%"
                      cy="50%"
                      innerRadius={48}
                      outerRadius={80}
                      paddingAngle={2}
                      isAnimationActive={false}
                    >
                      {statusChartData.map((_, i) => (
                        <Cell
                          key={statusChartData[i].name}
                          fill={STATUS_COLORS[i % STATUS_COLORS.length]}
                          stroke="transparent"
                        />
                      ))}
                    </Pie>
                    <Tooltip />
                  </PieChart>
                </ResponsiveContainer>
                <ul className="jira-legend">
                  {statusChartData.map((row, i) => (
                    <li key={row.name}>
                      <span
                        className="jira-legend-swatch"
                        style={{
                          background: STATUS_COLORS[i % STATUS_COLORS.length],
                        }}
                      />
                      <span className="jira-legend-name">{row.name}</span>
                      <span className="mono muted">{row.value}</span>
                    </li>
                  ))}
                </ul>
              </div>
            </section>
          </div>
        </>
      )}
    </div>
  );
}
