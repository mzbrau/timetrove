import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import { api } from "../../shared/api";
import { applyTheme } from "../../shared/theme";
import type { Settings } from "../../shared/types";

export function SettingsPanel() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [dbPath, setDbPath] = useState("");
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    Promise.all([api.getSettings(), api.getDbPath()])
      .then(([s, path]) => {
        setSettings(s);
        setDbPath(path);
      })
      .catch((e) => setError(String(e)));
  }, []);

  // Tracker context menu can toggle always-on-top independently; keep checkbox in sync
  // without clobbering unsaved edits to other fields.
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    listen<Settings>("settings-changed", (e) => {
      setSettings((prev) =>
        prev
          ? { ...prev, trackerAlwaysOnTop: e.payload.trackerAlwaysOnTop }
          : prev,
      );
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }, []);

  if (!settings) {
    if (error) {
      return <p className="err">{error}</p>;
    }
    return <p className="muted">Loading settings…</p>;
  }

  return (
    <div className="settings">
      {error && <p className="err">{error}</p>}
      {message && <p className="hint">{message}</p>}

      <label>
        Theme
        <select
          value={settings.theme}
          onChange={(e) => {
            const theme = e.target.value === "light" ? "light" : "dark";
            setSettings({ ...settings, theme });
            applyTheme(theme);
          }}
        >
          <option value="dark">Dark</option>
          <option value="light">Light</option>
        </select>
      </label>

      <label>
        Idle threshold (minutes)
        <input
          type="number"
          min={1}
          value={settings.idleThresholdMins}
          onChange={(e) =>
            setSettings({
              ...settings,
              idleThresholdMins: Number(e.target.value),
            })
          }
        />
      </label>

      <label>
        Daily goal (minutes)
        <input
          type="number"
          min={1}
          value={settings.dailyGoalMins}
          onChange={(e) =>
            setSettings({
              ...settings,
              dailyGoalMins: Number(e.target.value),
            })
          }
        />
        <span className="hint">
          Default 480 = 8 hours. Displayed as HH:MM in the tracker.
        </span>
      </label>

      <label>
        Switch-task shortcut
        <input
          value={settings.switchShortcut}
          onChange={(e) =>
            setSettings({ ...settings, switchShortcut: e.target.value })
          }
        />
        <span className="hint">
          Example: CommandOrControl+Shift+T — restart app after changing to
          rebind.
        </span>
      </label>

      <label style={{ gridTemplateColumns: "auto 1fr", alignItems: "center" }}>
        <input
          type="checkbox"
          checked={settings.autostart}
          onChange={(e) =>
            setSettings({ ...settings, autostart: e.target.checked })
          }
        />
        Start at login
      </label>

      <label style={{ gridTemplateColumns: "auto 1fr", alignItems: "center" }}>
        <input
          type="checkbox"
          checked={settings.trackerAlwaysOnTop}
          onChange={(e) =>
            setSettings({
              ...settings,
              trackerAlwaysOnTop: e.target.checked,
            })
          }
        />
        <span>
          Keep tracker always on top
          <span className="hint">
            {" "}
            Recommended on Windows when docked over the taskbar.
          </span>
        </span>
      </label>

      <div className="row-actions">
        <button
          type="button"
          onClick={async () => {
            try {
              const saved = await api.updateSettings(settings);
              setSettings(saved);
              applyTheme(saved.theme === "light" ? "light" : "dark");
              setMessage("Settings saved.");
            } catch (e) {
              setError(String(e));
            }
          }}
        >
          Save settings
        </button>
      </div>

      <hr
        style={{ border: 0, borderTop: "1px solid var(--line)", width: "100%" }}
      />

      <div>
        <strong>Database</strong>
        <p className="hint mono">{dbPath}</p>
        <div className="row-actions">
          <button type="button" onClick={() => api.revealDatabase()}>
            Reveal database
          </button>
          <button
            type="button"
            onClick={async () => {
              const path = prompt(
                "Export path (full file path)",
                `${dbPath}.bak`,
              );
              if (!path) return;
              try {
                await api.exportDatabase(path);
                setMessage("Database exported.");
              } catch (e) {
                setError(String(e));
              }
            }}
          >
            Export backup
          </button>
          <button
            type="button"
            onClick={async () => {
              const path = prompt("Import database from path");
              if (!path) return;
              if (
                !confirm("Replace the current database? This cannot be undone.")
              )
                return;
              try {
                await api.importDatabase(path);
                setMessage("Database imported. Restart recommended.");
              } catch (e) {
                setError(String(e));
              }
            }}
          >
            Import backup
          </button>
        </div>
      </div>
    </div>
  );
}
