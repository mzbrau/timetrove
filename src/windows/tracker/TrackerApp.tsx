import { LogicalPosition } from "@tauri-apps/api/dpi";
import { CheckMenuItem, Menu, MenuItem, Submenu } from "@tauri-apps/api/menu";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { useEffect, useRef } from "react";
import { api } from "../../shared/api";
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
      className={`tracker ${state.tracking ? "tracking" : "idle"}${snoozing ? " snoozing" : ""}`}
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
