import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { api } from "../../shared/api";
import { useTrackerState } from "../../shared/useTrackerState";
import "./tracker.css";

export function TrackerApp() {
  const { state } = useTrackerState();

  const onMouseDown = async (e: React.MouseEvent) => {
    if ((e.target as HTMLElement).closest("button")) return;
    await getCurrentWebviewWindow().startDragging();
  };

  if (!state) {
    return <div className="tracker loading">…</div>;
  }

  return (
    <div
      className={`tracker ${state.tracking ? "tracking" : "idle"}`}
      onMouseDown={onMouseDown}
    >
      <div className="tracker-main" onDoubleClick={() => api.openMainWindow()}>
        {state.tracking && state.currentTask ? (
          <>
            <span
              className="dot"
              style={{ background: state.currentTask.color }}
            />
            <span className="name">{state.currentTask.name}</span>
          </>
        ) : (
          <span className="name muted">Not tracking</span>
        )}
      </div>
      <button
        className={`total mono time ${state.goalMet ? "met" : "under"}`}
        onClick={() => api.openSwitcher()}
        title="Switch task"
      >
        {state.dailyTotalDisplay}
      </button>
    </div>
  );
}
