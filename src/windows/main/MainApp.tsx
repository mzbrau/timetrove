import { useCallback, useEffect, useRef, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { formatHms } from "../../shared/format";
import { useTrackerState } from "../../shared/useTrackerState";
import { TasksPanel } from "./TasksPanel";
import { TimesheetPanel } from "./TimesheetPanel";
import { EntriesPanel } from "./EntriesPanel";
import { SettingsPanel } from "./SettingsPanel";
import { ConfirmBar } from "./ConfirmBar";
import { api } from "../../shared/api";
import "./main.css";

type Tab = "tasks" | "timesheet" | "entries" | "settings";

export function MainApp() {
  const { state, error, setError } = useTrackerState();
  const [tab, setTab] = useState<Tab>("tasks");
  const [dismissedError, setDismissedError] = useState<string | null>(null);
  const [tasksDirty, setTasksDirty] = useState(false);
  const [pendingTab, setPendingTab] = useState<Tab | null>(null);
  const [saveRequestId, setSaveRequestId] = useState(0);
  const [tick, setTick] = useState(0);
  const [appVersion, setAppVersion] = useState<string | null>(null);
  const receivedAtRef = useRef(Date.now());
  const prevStateRef = useRef(state);

  useEffect(() => {
    void getVersion()
      .then(setAppVersion)
      .catch(() => setAppVersion(null));
  }, []);

  const bannerError = error || state?.dbError || null;
  const showBanner = Boolean(bannerError && bannerError !== dismissedError);

  // Sync baseline in the same render as new dailyTotalMs (avoid useEffect lag → jump).
  if (state !== prevStateRef.current) {
    prevStateRef.current = state;
    receivedAtRef.current = Date.now();
  }

  useEffect(() => {
    if (!state?.tracking) return;
    const id = window.setInterval(() => setTick((n) => n + 1), 1000);
    return () => window.clearInterval(id);
  }, [state?.tracking]);

  const dailyTotalLabel = (() => {
    if (!state) return "00:00:00";
    let ms = state.dailyTotalMs;
    if (state.tracking) {
      void tick;
      ms += Math.max(0, Date.now() - receivedAtRef.current);
    }
    return formatHms(ms);
  })();

  const onDirtyChange = useCallback((dirty: boolean) => {
    setTasksDirty(dirty);
  }, []);

  const requestTab = (next: Tab) => {
    if (next === tab) return;
    if (tab === "tasks" && tasksDirty) {
      setPendingTab(next);
      return;
    }
    setTab(next);
  };

  return (
    <div className="main-app">
      {showBanner && bannerError && (
        <div className="db-error">
          <span>{bannerError}</span>
          <button
            type="button"
            onClick={() => {
              setDismissedError(bannerError);
              setError(null);
            }}
          >
            Dismiss
          </button>
        </div>
      )}

      <header className="topbar">
        <div className="brand">
          <img
            className="brand-logo"
            src="/timetrove-logo.png"
            alt=""
            width={32}
            height={32}
          />
          <h1>TimeTrove</h1>
        </div>
        <div className="status-band">
          <div className="status-row">
            {state?.tracking && state.currentTask ? (
              <>
                <span
                  className="dot"
                  style={{ background: state.currentTask.color }}
                />
                <strong>{state.currentTask.name}</strong>
              </>
            ) : (
              <strong className="muted">Not tracking</strong>
            )}
            <span
              className={`mono time day-total ${state?.goalMet ? "met" : "under"}`}
            >
              {dailyTotalLabel}
            </span>
          </div>
          <div className="status-actions">
            <button type="button" onClick={() => api.openSwitcher()}>
              Switch
            </button>
            {state?.tracking ? (
              <button type="button" onClick={() => api.stopTracking()}>
                Stop
              </button>
            ) : (
              <button
                type="button"
                disabled={!state?.previousTask}
                onClick={() =>
                  state?.previousTask && api.startTask(state.previousTask.id)
                }
              >
                Resume
              </button>
            )}
          </div>
        </div>
      </header>

      <nav className="tabs">
        {(
          [
            ["tasks", "Tasks"],
            ["timesheet", "Timesheet"],
            ["entries", "Time entries"],
            ["settings", "Settings"],
          ] as const
        ).map(([id, label]) => (
          <button
            key={id}
            type="button"
            className={tab === id ? "active" : ""}
            onClick={() => requestTab(id)}
          >
            {label}
          </button>
        ))}
      </nav>

      {pendingTab && (
        <ConfirmBar
          message="You have unsaved task changes."
          confirmLabel="Save"
          secondaryLabel="Discard"
          cancelLabel="Cancel"
          onConfirm={() => {
            setSaveRequestId((n) => n + 1);
          }}
          onSecondary={() => {
            const next = pendingTab;
            setPendingTab(null);
            setTasksDirty(false);
            setTab(next);
          }}
          onCancel={() => setPendingTab(null)}
        />
      )}

      <main className="panel">
        {tab === "tasks" && (
          <TasksPanel
            state={state}
            onDirtyChange={onDirtyChange}
            saveRequestId={saveRequestId}
            onSaved={() => {
              if (pendingTab) {
                const next = pendingTab;
                setPendingTab(null);
                setTab(next);
              }
            }}
          />
        )}
        {tab === "timesheet" && <TimesheetPanel />}
        {tab === "entries" && <EntriesPanel state={state} />}
        {tab === "settings" && <SettingsPanel />}
      </main>

      {appVersion && (
        <span className="app-version mono muted" aria-label={`Version ${appVersion}`}>
          v{appVersion}
        </span>
      )}
    </div>
  );
}
