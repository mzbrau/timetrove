import { LogicalPosition } from "@tauri-apps/api/dpi";
import { listen } from "@tauri-apps/api/event";
import { CheckMenuItem, Menu, MenuItem, Submenu } from "@tauri-apps/api/menu";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { useEffect, useRef, useState } from "react";
import { api } from "../../shared/api";
import { formatHms } from "../../shared/format";
import { jiraBrowseUrl } from "../../shared/jira";
import type { Settings } from "../../shared/types";
import { useDemoShot } from "../../shared/useDemoShot";
import { useTrackerState } from "../../shared/useTrackerState";
import "./tracker.css";

type TrackerMenuItem = MenuItem | CheckMenuItem | Submenu;

function pad2(n: number) {
  return String(n).padStart(2, "0");
}

/** Next local 17:00 as ISO (today if still ahead, else tomorrow). */
function nextLocal1700Iso(): string {
  const d = new Date();
  const target = new Date(d);
  target.setHours(17, 0, 0, 0);
  if (target.getTime() <= d.getTime()) {
    target.setDate(target.getDate() + 1);
  }
  return target.toISOString();
}

function formatSnoozeUntil(iso: string): string {
  const d = new Date(iso);
  return `${pad2(d.getHours())}:${pad2(d.getMinutes())}`;
}

function hoursFromNow(hours: number): string {
  return new Date(Date.now() + hours * 3_600_000).toISOString();
}

export function TrackerApp() {
  const { state } = useTrackerState();
  const alwaysOnTopRef = useRef(true);
  const [showSeconds, setShowSeconds] = useState(false);
  const [jiraEnabled, setJiraEnabled] = useState(false);
  const [jiraSiteUrl, setJiraSiteUrl] = useState("");
  const [jiraSummaryOverride, setJiraSummaryOverride] = useState<string | null>(
    null,
  );
  const refreshedJiraKeyRef = useRef<string | null>(null);
  const [tick, setTick] = useState(0);
  const receivedAtRef = useRef(Date.now());
  const prevStateRef = useRef(state);
  const lastMainClickRef = useRef<{ t: number; x: number; y: number } | null>(
    null,
  );
  const [demoHover, setDemoHover] = useState(false);
  useDemoShot((shot) => setDemoHover(Boolean(shot.hover)));

  useEffect(() => {
    api
      .getSettings()
      .then((s) => {
        alwaysOnTopRef.current = s.trackerAlwaysOnTop;
        setShowSeconds(s.trackerShowSeconds);
        setJiraEnabled(s.jiraEnabled);
        setJiraSiteUrl(s.jiraSiteUrl);
      })
      .catch(() => {});
  }, []);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    listen<Settings>("settings-changed", (e) => {
      alwaysOnTopRef.current = e.payload.trackerAlwaysOnTop;
      setShowSeconds(e.payload.trackerShowSeconds);
      setJiraEnabled(e.payload.jiraEnabled);
      setJiraSiteUrl(e.payload.jiraSiteUrl);
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }, []);

  const jiraKey =
    state?.tracking && state.currentTask?.jiraIssueKey
      ? state.currentTask.jiraIssueKey
      : "";
  const jiraSummary =
    jiraSummaryOverride ??
    (state?.currentTask?.jiraIssueSummary?.trim() || "");

  useEffect(() => {
    setJiraSummaryOverride(null);
    refreshedJiraKeyRef.current = null;
  }, [jiraKey]);

  useEffect(() => {
    if (!jiraEnabled || !jiraKey || !state?.currentTask) return;
    if (jiraSummary) return;
    if (refreshedJiraKeyRef.current === jiraKey) return;
    refreshedJiraKeyRef.current = jiraKey;
    const taskId = state.currentTask.id;
    void api
      .refreshJiraIssue(taskId)
      .then((details) => {
        if (details?.summary) setJiraSummaryOverride(details.summary);
      })
      .catch(() => {});
  }, [jiraEnabled, jiraKey, jiraSummary, state?.currentTask]);

  if (state !== prevStateRef.current) {
    prevStateRef.current = state;
    receivedAtRef.current = Date.now();
  }

  useEffect(() => {
    if (!showSeconds || !state?.tracking) return;
    const id = window.setInterval(() => setTick((n) => n + 1), 1000);
    return () => window.clearInterval(id);
  }, [showSeconds, state?.tracking]);

  const dailyTotalLabel = (() => {
    if (!state) return "00:00";
    if (!showSeconds) return state.dailyTotalDisplay;
    let ms = state.dailyTotalMs;
    if (state.tracking) {
      void tick;
      ms += Math.max(0, Date.now() - receivedAtRef.current);
    }
    return formatHms(ms);
  })();

  const onMouseDown = async (e: React.MouseEvent) => {
    if (e.button !== 0) return;
    const target = e.target as HTMLElement;
    if (target.closest("button")) return;

    // startDragging() swallows the click sequence, so detect double-click here
    // before beginning a drag on the task/dot area.
    if (target.closest(".tracker-main")) {
      const now = Date.now();
      const prev = lastMainClickRef.current;
      const near =
        prev &&
        now - prev.t < 350 &&
        Math.abs(e.clientX - prev.x) < 8 &&
        Math.abs(e.clientY - prev.y) < 8;
      if (near) {
        lastMainClickRef.current = null;
        void api.openMainWindow();
        return;
      }
      lastMainClickRef.current = { t: now, x: e.clientX, y: e.clientY };
    } else {
      lastMainClickRef.current = null;
    }

    await getCurrentWebviewWindow().startDragging();
  };

  const onContextMenu = async (e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();

    let alwaysOnTop = alwaysOnTopRef.current;
    try {
      const s = await api.getSettings();
      alwaysOnTop = s.trackerAlwaysOnTop;
      alwaysOnTopRef.current = alwaysOnTop;
    } catch {
      /* use cached */
    }

    const snoozing = Boolean(state?.snoozeUntil);
    const tracking = Boolean(state?.tracking);

    const items: TrackerMenuItem[] = [
      await MenuItem.new({
        id: "open",
        text: "Open TimeTrove",
        action: () => {
          void api.openMainWindow();
        },
      }),
    ];

    if (snoozing && state?.snoozeUntil) {
      items.push(
        await MenuItem.new({
          id: "snooze_until_label",
          text: `Snoozing until ${formatSnoozeUntil(state.snoozeUntil)}`,
          enabled: false,
        }),
        await MenuItem.new({
          id: "exit_snooze",
          text: "Exit snooze",
          action: () => {
            void api.clearSnooze();
          },
        }),
      );
    } else if (tracking) {
      items.push(
        await Submenu.new({
          id: "snooze",
          text: "Snooze",
          items: [
            await MenuItem.new({
              id: "snooze_1h",
              text: "1 hour",
              action: () => {
                void api.startSnooze(hoursFromNow(1));
              },
            }),
            await MenuItem.new({
              id: "snooze_4h",
              text: "4 hours",
              action: () => {
                void api.startSnooze(hoursFromNow(4));
              },
            }),
            await MenuItem.new({
              id: "snooze_8h",
              text: "8 hours",
              action: () => {
                void api.startSnooze(hoursFromNow(8));
              },
            }),
            await MenuItem.new({
              id: "snooze_1700",
              text: "Until 17:00",
              action: () => {
                void api.startSnooze(nextLocal1700Iso());
              },
            }),
            await MenuItem.new({
              id: "snooze_custom",
              text: "Custom…",
              action: () => {
                void api.openSnoozePicker();
              },
            }),
          ],
        }),
      );
    }

    items.push(
      await CheckMenuItem.new({
        id: "always_on_top",
        text: "Always on top",
        checked: alwaysOnTop,
        action: () => {
          const next = !alwaysOnTopRef.current;
          void api.setTrackerAlwaysOnTop(next).then((saved) => {
            alwaysOnTopRef.current = saved.trackerAlwaysOnTop;
          });
        },
      }),
      await MenuItem.new({
        id: "reset_position",
        text: "Reset position",
        action: () => {
          void api.resetTrackerPosition();
        },
      }),
      await MenuItem.new({
        id: "close",
        text: "Close",
        action: () => {
          void api.setTrackerVisible(false);
        },
      }),
    );

    const menu = await Menu.new({ items });
    await menu.popup(new LogicalPosition(e.clientX, e.clientY));
  };

  if (!state) {
    return <div className="tracker loading">…</div>;
  }

  const snoozing = Boolean(state.snoozeUntil);

  return (
    <div
      className={`tracker ${state.tracking ? "tracking" : "idle"}${snoozing ? " snoozing" : ""}${demoHover ? " demo-hover" : ""}`}
      onMouseDown={onMouseDown}
      onContextMenu={(e) => {
        void onContextMenu(e);
      }}
    >
      <div className="tracker-main">
        {state.tracking && state.currentTask ? (
          <>
            <span
              className="dot"
              style={{ background: state.currentTask.color }}
            />
            <span className="name">{state.currentTask.name}</span>
            {snoozing && (
              <span
                className="snooze-mark"
                title={
                  state.snoozeUntil
                    ? `Snoozing until ${formatSnoozeUntil(state.snoozeUntil)}`
                    : "Snoozing"
                }
              >
                zᴢ
              </span>
            )}
          </>
        ) : (
          <span className="name muted">Not tracking</span>
        )}
      </div>
      <div className="tracker-actions">
        {state.tracking && (
          <button
            type="button"
            className="tracker-stop"
            title="Stop tracking"
            onClick={() => {
              void api.stopTracking();
            }}
          >
            <span className="tracker-stop-icon" aria-hidden />
          </button>
        )}
        {jiraEnabled && jiraKey && (
          <button
            type="button"
            className="tracker-jira"
            title={
              jiraSiteUrl.trim()
                ? jiraSummary
                  ? `${jiraKey} — ${jiraSummary}`
                  : jiraKey
                : `${jiraKey} (set Jira site URL in Settings to open)`
            }
            onClick={() => {
              const url = jiraBrowseUrl(jiraSiteUrl, jiraKey);
              if (!url) return;
              void api.openExternalUrl(url);
            }}
          >
            <svg
              className="tracker-jira-icon"
              viewBox="0 0 24 24"
              width="12"
              height="12"
              aria-hidden
            >
              <path
                fill="currentColor"
                d="M11.53 2c0 2.4 1.97 4.35 4.35 4.35h1.52v1.5c0 2.4 1.94 4.34 4.35 4.34V2.84A.84.84 0 0 0 20.91 2H11.53zM6.22 7.3c0 2.4 1.97 4.35 4.35 4.35h1.52v1.5c0 2.4 1.94 4.34 4.35 4.34V8.14a.84.84 0 0 0-.84-.84H6.22zm-5.3 5.3c0 2.4 1.97 4.35 4.35 4.35h1.52v1.5c0 2.4 1.94 4.34 4.35 4.34v-9.35a.84.84 0 0 0-.84-.84H.92z"
              />
            </svg>
          </button>
        )}
        <button
          className={`total mono time ${state.goalMet ? "met" : "under"}`}
          onClick={() => api.openSwitcher()}
          title="Switch task"
        >
          {dailyTotalLabel}
        </button>
        {state.copilotUsage && (
          <button
            type="button"
            className={`copilot-usage mono ${copilotUsageBand(state.copilotUsage.percentUsed, state.copilotUsage.unlimited)}`}
            title={copilotUsageTooltip(state.copilotUsage)}
            onClick={() => {
              void api.openMainWindow();
            }}
          >
            <svg
              className="copilot-usage-icon"
              viewBox="0 0 16 16"
              width="12"
              height="12"
              aria-hidden
            >
              <path
                fill="currentColor"
                d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27s1.36.09 2 .27c1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0016 8c0-4.42-3.58-8-8-8z"
              />
            </svg>
            <span>
              {state.copilotUsage.unlimited
                ? "∞"
                : `${Math.round(state.copilotUsage.percentUsed)}%`}
            </span>
          </button>
        )}
      </div>
    </div>
  );
}

function copilotUsageBand(
  percentUsed: number,
  unlimited: boolean,
): "ok" | "warn" | "danger" | "unlimited" {
  if (unlimited) return "unlimited";
  if (percentUsed >= 90) return "danger";
  if (percentUsed >= 75) return "warn";
  return "ok";
}

function copilotUsageTooltip(usage: {
  percentUsed: number;
  used: number;
  entitlement: number;
  remaining: number;
  unit: string;
  plan: string;
  unlimited: boolean;
  resetDate: string | null;
  lastSyncedAt: string | null;
  lastError: string | null;
}): string {
  const lines: string[] = ["GitHub Copilot"];
  if (usage.plan) lines.push(`Plan: ${usage.plan}`);
  if (usage.unlimited) {
    lines.push("Allowance: unlimited");
  } else {
    lines.push(
      `Used: ${fmtQty(usage.used)} / ${fmtQty(usage.entitlement)} ${usage.unit} (${Math.round(usage.percentUsed)}%)`,
    );
    lines.push(`Remaining: ${fmtQty(usage.remaining)} ${usage.unit}`);
  }
  if (usage.resetDate) {
    const d = new Date(usage.resetDate);
    lines.push(
      `Resets: ${Number.isNaN(d.getTime()) ? usage.resetDate : d.toLocaleDateString()}`,
    );
  }
  if (usage.lastSyncedAt) {
    lines.push(`Synced: ${new Date(usage.lastSyncedAt).toLocaleString()}`);
  }
  if (usage.lastError) {
    lines.push(`Error: ${usage.lastError}`);
  }
  return lines.join("\n");
}

function fmtQty(n: number): string {
  if (!Number.isFinite(n)) return "—";
  return Number.isInteger(n) ? String(n) : n.toFixed(1);
}
