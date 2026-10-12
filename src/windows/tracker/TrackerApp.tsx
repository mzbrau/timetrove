import { LogicalPosition } from "@tauri-apps/api/dpi";
import { CheckMenuItem, Menu, MenuItem } from "@tauri-apps/api/menu";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { useEffect, useRef } from "react";
import { api } from "../../shared/api";
import { useTrackerState } from "../../shared/useTrackerState";
import "./tracker.css";

export function TrackerApp() {
  const { state } = useTrackerState();
  const alwaysOnTopRef = useRef(true);

  useEffect(() => {
    api
      .getSettings()
      .then((s) => {
        alwaysOnTopRef.current = s.trackerAlwaysOnTop;
      })
      .catch(() => {});
  }, []);

  const onMouseDown = async (e: React.MouseEvent) => {
    if ((e.target as HTMLElement).closest("button")) return;
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

    const menu = await Menu.new({
      items: [
        await MenuItem.new({
          id: "open",
          text: "Open TimeTrove",
          action: () => {
            void api.openMainWindow();
          },
        }),
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
      ],
    });

    await menu.popup(new LogicalPosition(e.clientX, e.clientY));
  };

  if (!state) {
    return <div className="tracker loading">…</div>;
  }

  return (
    <div
      className={`tracker ${state.tracking ? "tracking" : "idle"}`}
      onMouseDown={onMouseDown}
      onContextMenu={(e) => {
        void onContextMenu(e);
      }}
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
