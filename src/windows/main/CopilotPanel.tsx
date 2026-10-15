import { useCallback, useEffect, useMemo, useState } from "react";
import {
  CartesianGrid,
  Line,
  LineChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import { api } from "../../shared/api";
import type {
  CopilotUsageHistoryPoint,
  CopilotUsageSummary,
} from "../../shared/types";

function cssVar(name: string, fallback: string): string {
  if (typeof document === "undefined") return fallback;
  const v = getComputedStyle(document.documentElement)
    .getPropertyValue(name)
    .trim();
  return v || fallback;
}

function fmtQty(n: number): string {
  if (!Number.isFinite(n)) return "—";
  return Number.isInteger(n) ? String(n) : n.toFixed(1);
}

function fmtPct(n: number): string {
  if (!Number.isFinite(n)) return "—";
  return `${Math.round(n)}%`;
}

function fmtDate(iso: string | null | undefined): string {
  if (!iso) return "—";
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? iso : d.toLocaleString();
}

function fmtDay(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso.slice(5, 10);
  return `${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

function poolLabel(id: string): string {
  switch (id) {
    case "premium_interactions":
      return "Premium";
    case "chat":
      return "Chat";
    case "completions":
      return "Completions";
    default:
      return id.replace(/_/g, " ");
  }
}

type Props = {
  /** Live summary from tracker-state; drives KPIs without polling this tab. */
  usage: CopilotUsageSummary | null;
};

export function CopilotPanel({ usage }: Props) {
  const [history, setHistory] = useState<CopilotUsageHistoryPoint[]>([]);
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const loadHistory = useCallback(async () => {
    try {
      const h = await api.getCopilotUsageHistory(500);
      setHistory(h);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void loadHistory();
  }, [loadHistory]);

  // Reload history when background poll sync timestamp or error changes — not every tracker tick.
  useEffect(() => {
    if (loading) return;
    void loadHistory();
    // eslint-disable-next-line react-hooks/exhaustive-deps -- identity keys only
  }, [usage?.lastSyncedAt, usage?.lastError]);

  const refresh = useCallback(async () => {
    setRefreshing(true);
    setError(null);
    try {
      await api.refreshCopilotUsage();
      await loadHistory();
    } catch (e) {
      setError(String(e));
    } finally {
      setRefreshing(false);
    }
  }, [loadHistory]);

  const ink = cssVar("--ink", "#e8eef2");
  const muted = cssVar("--muted", "#8b9aab");
  const line = cssVar("--line", "rgba(232,238,242,0.1)");
  const accent = cssVar("--accent", "#3d9b8f");

  const chartData = useMemo(
    () =>
      history.map((p) => ({
        t: fmtDay(p.fetchedAt),
        used: Math.round(p.used * 10) / 10,
        percent: Math.round(p.percentUsed * 10) / 10,
        entitlement: p.entitlement,
      })),
    [history],
  );

  const tooltipStyle = {
    background: cssVar("--bg-elevated", "#1a2128"),
    border: `1px solid ${line}`,
    borderRadius: 8,
    color: ink,
  };

  const refreshButton = (
    <button type="button" disabled={refreshing} onClick={() => void refresh()}>
      {refreshing ? "Refreshing…" : "Refresh now"}
    </button>
  );

  if (loading) {
    return <p className="muted">Loading Copilot usage…</p>;
  }

  if (!usage && !error) {
    return (
      <div className="copilot-empty">
        <p>
          No usage data yet. Open Settings, enable Copilot usage tracking, add a
          token, and run Test connection — or refresh now.
        </p>
        <div className="row-actions">{refreshButton}</div>
      </div>
    );
  }

  return (
    <div className="copilot">
      <div className="copilot-toolbar">
        <div className="row-actions">{refreshButton}</div>
        <p className="muted copilot-sync-meta">
          {usage?.lastSyncedAt
            ? `Last synced ${fmtDate(usage.lastSyncedAt)}`
            : "Not synced yet"}
          {usage?.limitSource ? ` · Limit: ${usage.limitSource}` : ""}
        </p>
      </div>

      {error && <p className="err">{error}</p>}
      {usage?.lastError && !error && (
        <p className="err">Last error: {usage.lastError}</p>
      )}

      {usage && (
        <>
          <div className="copilot-kpis">
            <div className="copilot-kpi">
              <span className="copilot-kpi-label">Usage</span>
              <strong className="copilot-kpi-value mono">
                {usage.unlimited ? "∞" : fmtPct(usage.percentUsed)}
              </strong>
            </div>
            <div className="copilot-kpi">
              <span className="copilot-kpi-label">Used</span>
              <strong className="copilot-kpi-value mono">
                {usage.unlimited
                  ? "—"
                  : `${fmtQty(usage.used)} / ${fmtQty(usage.entitlement)}`}
              </strong>
              <span className="muted">{usage.unit}</span>
            </div>
            <div className="copilot-kpi">
              <span className="copilot-kpi-label">Remaining</span>
              <strong className="copilot-kpi-value mono">
                {usage.unlimited ? "Unlimited" : fmtQty(usage.remaining)}
              </strong>
            </div>
            <div className="copilot-kpi">
              <span className="copilot-kpi-label">Resets</span>
              <strong className="copilot-kpi-value">
                {usage.resetDate
                  ? new Date(usage.resetDate).toLocaleDateString()
                  : "—"}
              </strong>
            </div>
            <div className="copilot-kpi">
              <span className="copilot-kpi-label">Plan</span>
              <strong className="copilot-kpi-value">
                {usage.plan || "—"}
              </strong>
              {usage.username ? (
                <span className="muted">@{usage.username}</span>
              ) : null}
            </div>
            <div className="copilot-kpi">
              <span className="copilot-kpi-label">Overage</span>
              <strong className="copilot-kpi-value mono">
                {fmtQty(usage.overageCount)}
              </strong>
              <span className="muted">
                {usage.overagePermitted ? "permitted" : "blocked"}
              </span>
            </div>
          </div>

          {usage.pools.length > 0 && (
            <section className="copilot-section">
              <h3>Quota pools</h3>
              <div className="copilot-pools">
                {usage.pools.map((p) => (
                  <div key={p.id} className="copilot-pool">
                    <div className="copilot-pool-head">
                      <strong>{poolLabel(p.id)}</strong>
                      <span className="mono muted">
                        {p.unlimited
                          ? "Unlimited"
                          : `${fmtPct(p.percentUsed)} used`}
                      </span>
                    </div>
                    {!p.unlimited && (
                      <div
                        className="copilot-pool-bar"
                        role="progressbar"
                        aria-valuenow={Math.round(p.percentUsed)}
                        aria-valuemin={0}
                        aria-valuemax={100}
                      >
                        <div
                          className="copilot-pool-bar-fill"
                          style={{
                            width: `${Math.min(100, Math.max(0, p.percentUsed))}%`,
                          }}
                        />
                      </div>
                    )}
                    <p className="muted mono">
                      {p.unlimited
                        ? "No monthly cap"
                        : `${fmtQty(p.used)} / ${fmtQty(p.entitlement)} · ${fmtQty(p.remaining)} left`}
                      {p.overageCount > 0
                        ? ` · overage ${fmtQty(p.overageCount)}`
                        : ""}
                    </p>
                  </div>
                ))}
              </div>
            </section>
          )}

          <section className="copilot-section">
            <h3>Usage over time</h3>
            {chartData.length < 2 ? (
              <p className="muted">
                Charts appear after a few successful polls. Interval is
                configurable in Settings (default 30 minutes).
              </p>
            ) : (
              <div className="copilot-chart-card">
                <ResponsiveContainer width="100%" height={260}>
                  <LineChart data={chartData}>
                    <CartesianGrid stroke={line} strokeDasharray="3 3" />
                    <XAxis
                      dataKey="t"
                      stroke={muted}
                      tick={{ fill: muted, fontSize: 11 }}
                    />
                    <YAxis
                      stroke={muted}
                      tick={{ fill: muted, fontSize: 11 }}
                      width={40}
                    />
                    <Tooltip contentStyle={tooltipStyle} />
                    <Line
                      type="monotone"
                      dataKey="used"
                      name={`Used (${usage.unit})`}
                      stroke={accent}
                      strokeWidth={2}
                      dot={false}
                    />
                    <Line
                      type="monotone"
                      dataKey="percent"
                      name="% used"
                      stroke={cssVar("--goal-met", "#d4a054")}
                      strokeWidth={2}
                      dot={false}
                    />
                  </LineChart>
                </ResponsiveContainer>
              </div>
            )}
          </section>
        </>
      )}
    </div>
  );
}
