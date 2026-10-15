import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import { api } from "../../shared/api";
import { applyTheme } from "../../shared/theme";
import type {
  CopilotConnectionTestEvent,
  JiraConnectionTestEvent,
  Settings,
} from "../../shared/types";

export function SettingsPanel() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [dbPath, setDbPath] = useState("");
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [jiraTesting, setJiraTesting] = useState(false);
  const [jiraTestPhase, setJiraTestPhase] = useState<string | null>(null);
  const [jiraTestStatus, setJiraTestStatus] = useState<string | null>(null);
  const [copilotTesting, setCopilotTesting] = useState(false);
  const [copilotTestPhase, setCopilotTestPhase] = useState<string | null>(null);
  const [copilotTestStatus, setCopilotTestStatus] = useState<string | null>(
    null,
  );

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
          ? {
              ...prev,
              trackerAlwaysOnTop: e.payload.trackerAlwaysOnTop,
              trackerShowSeconds: e.payload.trackerShowSeconds,
            }
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

      <label style={{ gridTemplateColumns: "auto 1fr", alignItems: "center" }}>
        <input
          type="checkbox"
          checked={settings.trackerShowSeconds}
          onChange={(e) =>
            setSettings({
              ...settings,
              trackerShowSeconds: e.target.checked,
            })
          }
        />
        <span>
          Show seconds on tracker
          <span className="hint">
            {" "}
            When off, the tracker shows HH:MM (default). When on, live HH:MM:SS.
          </span>
        </span>
      </label>

      <hr
        style={{ border: 0, borderTop: "1px solid var(--line)", width: "100%" }}
      />

      <div>
        <strong>Jira Cloud</strong>
        <p className="hint">
          Optionally log tracked time to Jira worklogs (hourly and on stop/switch).
        </p>
        <label style={{ gridTemplateColumns: "auto 1fr", alignItems: "center" }}>
          <input
            type="checkbox"
            checked={settings.jiraEnabled}
            onChange={(e) =>
              setSettings({ ...settings, jiraEnabled: e.target.checked })
            }
          />
          Enable Jira worklog sync
        </label>
        {settings.jiraEnabled && (
          <>
            <label>
              Site URL
              <input
                value={settings.jiraSiteUrl}
                placeholder="https://your-org.atlassian.net"
                onChange={(e) =>
                  setSettings({ ...settings, jiraSiteUrl: e.target.value })
                }
              />
            </label>
            <label>
              Email
              <input
                type="email"
                autoComplete="username"
                value={settings.jiraEmail}
                onChange={(e) =>
                  setSettings({ ...settings, jiraEmail: e.target.value })
                }
              />
            </label>
            <label>
              API token
              <input
                type="password"
                autoComplete="current-password"
                value={settings.jiraApiToken}
                onChange={(e) =>
                  setSettings({ ...settings, jiraApiToken: e.target.value })
                }
              />
              <span className="hint">
                Create a token at id.atlassian.com — stored only in your local
                database.
              </span>
            </label>
            <label>
              Proxy URL (optional)
              <input
                value={settings.jiraProxyUrl}
                placeholder="http://host:port"
                autoComplete="off"
                onChange={(e) =>
                  setSettings({ ...settings, jiraProxyUrl: e.target.value })
                }
              />
              <span className="hint">
                HTTP CONNECT proxy if required by your network — e.g.
                http://host:port or http://user:pass@host:port. Leave empty for
                a direct connection.
              </span>
            </label>
            <div className="row-actions">
              <button
                type="button"
                disabled={jiraTesting}
                onClick={async () => {
                  setError(null);
                  setMessage(null);
                  setJiraTesting(true);
                  setJiraTestPhase("saving");
                  setJiraTestStatus("Saving settings…");

                  let unlisten: (() => void) | undefined;
                  try {
                    const saved = await api.updateSettings(settings);
                    setSettings(saved);

                    unlisten = await listen<JiraConnectionTestEvent>(
                      "jira-connection-test",
                      (e) => {
                        const { phase, message: status } = e.payload;
                        setJiraTestPhase(phase);
                        setJiraTestStatus(status);
                        if (phase === "succeeded" || phase === "failed") {
                          void api.getSettings().then(setSettings).catch(() => {});
                          setJiraTesting(false);
                          if (phase === "succeeded") {
                            setMessage(status);
                            setError(null);
                          } else {
                            setError(status);
                            setMessage(null);
                          }
                          unlisten?.();
                          unlisten = undefined;
                        }
                      },
                    );

                    await api.startJiraConnectionTest();
                  } catch (e) {
                    unlisten?.();
                    setJiraTesting(false);
                    setJiraTestPhase("failed");
                    setJiraTestStatus(String(e));
                    setError(String(e));
                    const refreshed = await api.getSettings().catch(() => null);
                    if (refreshed) setSettings(refreshed);
                  }
                }}
              >
                {jiraTesting ? "Testing…" : "Test connection"}
              </button>
            </div>
            {jiraTestStatus && (
              <p
                className={
                  jiraTestPhase === "failed"
                    ? "err"
                    : jiraTestPhase === "succeeded"
                      ? "hint"
                      : "muted"
                }
              >
                {jiraTestStatus}
              </p>
            )}
            {(settings.jiraLastError || settings.jiraLastSyncedAt) && (
              <p className="hint">
                {settings.jiraLastError
                  ? `Last error: ${settings.jiraLastError}`
                  : null}
                {settings.jiraLastError && settings.jiraLastSyncedAt
                  ? " · "
                  : null}
                {settings.jiraLastSyncedAt
                  ? `Last synced: ${new Date(settings.jiraLastSyncedAt).toLocaleString()}`
                  : null}
              </p>
            )}
          </>
        )}
      </div>

      <hr
        style={{ border: 0, borderTop: "1px solid var(--line)", width: "100%" }}
      />

      <div>
        <strong>GitHub Copilot usage</strong>
        <p className="hint">
          Track your Copilot premium request / credit allowance against the
          Enterprise Cloud quota API (same source VS Code uses). Disabled by
          default.
        </p>
        <label style={{ gridTemplateColumns: "auto 1fr", alignItems: "center" }}>
          <input
            type="checkbox"
            checked={settings.copilotEnabled}
            onChange={(e) =>
              setSettings({ ...settings, copilotEnabled: e.target.checked })
            }
          />
          Enable Copilot usage tracking
        </label>
        {settings.copilotEnabled && (
          <>
            <label>
              GitHub host
              <input
                value={settings.copilotGithubHost}
                placeholder="github.com"
                onChange={(e) =>
                  setSettings({
                    ...settings,
                    copilotGithubHost: e.target.value,
                  })
                }
              />
              <span className="hint">
                Use github.com or your Enterprise Cloud host (e.g.
                company.ghe.com).
              </span>
            </label>
            <label>
              Username (optional)
              <input
                value={settings.copilotUsername}
                placeholder="Auto-detected from token"
                autoComplete="username"
                onChange={(e) =>
                  setSettings({
                    ...settings,
                    copilotUsername: e.target.value,
                  })
                }
              />
            </label>
            <label>
              API token
              <input
                type="password"
                autoComplete="current-password"
                value={settings.copilotApiToken}
                onChange={(e) =>
                  setSettings({
                    ...settings,
                    copilotApiToken: e.target.value,
                  })
                }
              />
              <span className="hint">
                Prefer a fine-grained PAT with Copilot Requests (user-owned), or
                an OAuth/`gh` token. Classic ghp_ tokens often fail on this API.
                Stored only in your local database.
              </span>
            </label>
            <label>
              Poll interval (minutes)
              <input
                type="number"
                min={5}
                max={120}
                value={settings.copilotPollIntervalMins}
                onChange={(e) =>
                  setSettings({
                    ...settings,
                    copilotPollIntervalMins: Math.max(
                      5,
                      Number(e.target.value) || 30,
                    ),
                  })
                }
              />
            </label>
            <label>
              Monthly limit override (optional)
              <input
                type="number"
                min={1}
                step={1}
                value={settings.copilotMonthlyLimitOverride ?? ""}
                placeholder="Use API entitlement"
                onChange={(e) => {
                  const raw = e.target.value.trim();
                  setSettings({
                    ...settings,
                    copilotMonthlyLimitOverride: raw
                      ? Number(raw) || null
                      : null,
                  });
                }}
              />
              <span className="hint">
                Used only when the API has no usable entitlement (or reports
                unlimited). Leave empty to trust the API.
              </span>
            </label>
            <div className="row-actions">
              <button
                type="button"
                disabled={copilotTesting}
                onClick={async () => {
                  setError(null);
                  setMessage(null);
                  setCopilotTesting(true);
                  setCopilotTestPhase("saving");
                  setCopilotTestStatus("Saving settings…");

                  let unlisten: (() => void) | undefined;
                  try {
                    const saved = await api.updateSettings(settings);
                    setSettings(saved);

                    unlisten = await listen<CopilotConnectionTestEvent>(
                      "copilot-connection-test",
                      (e) => {
                        const { phase, message: status } = e.payload;
                        setCopilotTestPhase(phase);
                        setCopilotTestStatus(status);
                        if (phase === "succeeded" || phase === "failed") {
                          void api
                            .getSettings()
                            .then(setSettings)
                            .catch(() => {});
                          setCopilotTesting(false);
                          if (phase === "succeeded") {
                            setMessage(status);
                            setError(null);
                          } else {
                            setError(status);
                            setMessage(null);
                          }
                          unlisten?.();
                          unlisten = undefined;
                        }
                      },
                    );

                    await api.startCopilotConnectionTest();
                  } catch (e) {
                    unlisten?.();
                    setCopilotTesting(false);
                    setCopilotTestPhase("failed");
                    setCopilotTestStatus(String(e));
                    setError(String(e));
                    const refreshed = await api.getSettings().catch(() => null);
                    if (refreshed) setSettings(refreshed);
                  }
                }}
              >
                {copilotTesting ? "Testing…" : "Test connection"}
              </button>
            </div>
            {copilotTestStatus && (
              <p
                className={
                  copilotTestPhase === "failed"
                    ? "err"
                    : copilotTestPhase === "succeeded"
                      ? "hint"
                      : "muted"
                }
              >
                {copilotTestStatus}
              </p>
            )}
            {(settings.copilotLastError || settings.copilotLastSyncedAt) && (
              <p className="hint">
                {settings.copilotLastError
                  ? `Last error: ${settings.copilotLastError}`
                  : null}
                {settings.copilotLastError && settings.copilotLastSyncedAt
                  ? " · "
                  : null}
                {settings.copilotLastSyncedAt
                  ? `Last synced: ${new Date(settings.copilotLastSyncedAt).toLocaleString()}`
                  : null}
              </p>
            )}
          </>
        )}
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
    </div>
  );
}
