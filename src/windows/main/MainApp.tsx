import { useState } from "react";
import { useTrackerState } from "../../shared/useTrackerState";
import { TasksPanel } from "./TasksPanel";
import { TimesheetPanel } from "./TimesheetPanel";
import { EntriesPanel } from "./EntriesPanel";
import { SettingsPanel } from "./SettingsPanel";
import { api } from "../../shared/api";
import "./main.css";

type Tab = "tasks" | "timesheet" | "entries" | "settings";

export function MainApp() {
  const { state, error, setError } = useTrackerState();
  const [tab, setTab] = useState<Tab>("tasks");
  const [dismissedError, setDismissedError] = useState<string | null>(null);

  const bannerError = error || state?.dbError || null;
  const showBanner = Boolean(bannerError && bannerError !== dismissedError);

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
          <h1>Time Keeper</h1>
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
              className={`mono day-total ${state?.goalMet ? "met" : "under"}`}
            >
              {state?.dailyTotalDisplay ?? "00:00"}
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
            onClick={() => setTab(id)}
          >
            {label}
          </button>
        ))}
      </nav>

      <main className="panel">
        {tab === "tasks" && <TasksPanel state={state} />}
        {tab === "timesheet" && <TimesheetPanel />}
        {tab === "entries" && <EntriesPanel />}
        {tab === "settings" && <SettingsPanel />}
      </main>
    </div>
  );
}
