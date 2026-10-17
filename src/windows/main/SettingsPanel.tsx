import { listen } from "@tauri-apps/api/event";
import { useEffect, useMemo, useState } from "react";
import { api } from "../../shared/api";
import { applyTheme } from "../../shared/theme";
import type {
  CopilotConnectionTestEvent,
  JiraConnectionTestEvent,
  Settings,
} from "../../shared/types";

/** Editable fields only — sync metadata must not flip Save on. */
function editableSnapshot(s: Settings) {
  return {
    idleThresholdMins: s.idleThresholdMins,
    dailyGoalMins: s.dailyGoalMins,
    switchShortcut: s.switchShortcut,
    autostart: s.autostart,
    theme: s.theme,
    trackerAlwaysOnTop: s.trackerAlwaysOnTop,
    trackerShowSeconds: s.trackerShowSeconds,
    jiraEnabled: s.jiraEnabled,
    jiraSiteUrl: s.jiraSiteUrl,
    jiraEmail: s.jiraEmail,
    jiraApiToken: s.jiraApiToken,
    jiraProxyUrl: s.jiraProxyUrl,
    copilotEnabled: s.copilotEnabled,
    copilotGithubHost: s.copilotGithubHost,
    copilotUsername: s.copilotUsername,
    copilotApiToken: s.copilotApiToken,
    copilotProxyUrl: s.copilotProxyUrl,
    copilotPollIntervalMins: s.copilotPollIntervalMins,
    copilotMonthlyLimitOverride: s.copilotMonthlyLimitOverride,
    githubActivityEnabled: s.githubActivityEnabled,
    githubActivityPollIntervalMins: s.githubActivityPollIntervalMins,
  };
}

function settingsAreDirty(draft: Settings, saved: Settings): boolean {
  return (
    JSON.stringify(editableSnapshot(draft)) !==
    JSON.stringify(editableSnapshot(saved))
  );
}

export function SettingsPanel() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [saved, setSaved] = useState<Settings | null>(null);
  const [dbPath, setDbPath] = useState("");
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
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
        setSaved(s);
        setDbPath(path);
      })
      .catch((e) => setError(String(e)));
  }, []);

  // Tracker context menu can toggle always-on-top independently; keep checkbox in sync
  // without clobbering unsaved edits to other fields.
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    listen<Settings>("settings-changed", (e) => {
      const patch = {
        trackerAlwaysOnTop: e.payload.trackerAlwaysOnTop,
        trackerShowSeconds: e.payload.trackerShowSeconds,
      };
      setSettings((prev) => (prev ? { ...prev, ...patch } : prev));
      setSaved((prev) => (prev ? { ...prev, ...patch } : prev));
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }, []);

  const isDirty = useMemo(() => {
    if (!settings || !saved) return false;
    return settingsAreDirty(settings, saved);
  }, [settings, saved]);

  if (!settings) {
    if (error) {
      return <p className="err">{error}</p>;
    }
    return <p className="muted">Loading settings…</p>;
  }

  const commitSaved = (next: Settings) => {
    setSettings(next);
    setSaved(next);
  };

  return (
    <div className="settings">
      <div className="settings-header">
        <div className="settings-header-status">
          {error && <p className="err">{error}</p>}
          {message && <p className="hint">{message}</p>}
          {!error && !message && isDirty && (
            <p className="muted">Unsaved changes</p>
          )}
        </div>
        <button
          type="button"
          className="settings-save"
          disabled={!isDirty || saving}
          onClick={async () => {
            setSaving(true);
            setError(null);
            try {
              const next = await api.updateSettings(settings);
              commitSaved(next);
              applyTheme(next.theme === "light" ? "light" : "dark");
              setMessage("Settings saved.");
            } catch (e) {
              setError(String(e));
            } finally {
              setSaving(false);
            }
          }}
        >
          {saving ? "Saving…" : "Save settings"}
        </button>
      </div>

      <div className="settings-grid">
        <section className="settings-card">
          <div className="settings-card-header">
            <h3>General</h3>
            <p className="hint">Appearance, idle detection, and tracker behavior.</p>
          </div>
          <div className="settings-card-fields">
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

            <label className="settings-check">
              <input
                type="checkbox"
                checked={settings.autostart}
                onChange={(e) =>
                  setSettings({ ...settings, autostart: e.target.checked })
                }
              />
              Start at login
            </label>

            <label className="settings-check">
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

            <label className="settings-check">
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
                  When off, the tracker shows HH:MM (default). When on, live
                  HH:MM:SS.
                </span>
              </span>
            </label>
          </div>
        </section>

        <section className="settings-card">
          <div className="settings-card-header">
            <h3>Jira Cloud</h3>
            <p className="hint">
              Optionally log tracked time to Jira worklogs (hourly and on
              stop/switch).
            </p>
          </div>
          <div className="settings-card-fields">
            <label className="settings-check">
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
                    Create a token at id.atlassian.com — stored only in your
                    local database.
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
                    http://host:port or http://user:pass@host:port. Leave empty
                    for a direct connection.
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
                        const next = await api.updateSettings(settings);
                        commitSaved(next);

                        unlisten = await listen<JiraConnectionTestEvent>(
                          "jira-connection-test",
                          (e) => {
                            const { phase, message: status } = e.payload;
                            setJiraTestPhase(phase);
                            setJiraTestStatus(status);
                            if (phase === "succeeded" || phase === "failed") {
                              void api
                                .getSettings()
                                .then((fresh) => {
                                  setSettings(fresh);
                                  setSaved(fresh);
                                })
                                .catch(() => {});
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
                        if (refreshed) commitSaved(refreshed);
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
        </section>

        <section className="settings-card">
          <div className="settings-card-header">
            <h3>GitHub</h3>
            <p className="hint">
              Shared host, token, and proxy for Copilot quota tracking and the
              GitHub activity page (open PRs and recent commits). Both features
              are off by default.
            </p>
          </div>
          <div className="settings-card-fields">
            <label className="settings-check">
              <input
                type="checkbox"
                checked={settings.copilotEnabled}
                onChange={(e) =>
                  setSettings({ ...settings, copilotEnabled: e.target.checked })
                }
              />
              Enable Copilot usage tracking
            </label>
            <label className="settings-check">
              <input
                type="checkbox"
                checked={settings.githubActivityEnabled}
                onChange={(e) =>
                  setSettings({
                    ...settings,
                    githubActivityEnabled: e.target.checked,
                  })
                }
              />
              Enable GitHub activity page
            </label>
            {(settings.copilotEnabled || settings.githubActivityEnabled) && (
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
                    For Copilot: fine-grained PAT with Copilot Requests, or an
                    OAuth/`gh` token. For the activity page: also need Pull
                    requests, Contents, Metadata, and Checks read (classic
                    `repo` covers private repos). Stored only in your local
                    database.
                  </span>
                </label>
                <label>
                  Proxy URL (optional)
                  <input
                    value={settings.copilotProxyUrl}
                    placeholder="http://host:port"
                    autoComplete="off"
                    onChange={(e) =>
                      setSettings({
                        ...settings,
                        copilotProxyUrl: e.target.value,
                      })
                    }
                  />
                  <span className="hint">
                    HTTP CONNECT proxy if required by your network — e.g.
                    http://host:port or http://user:pass@host:port. Leave empty
                    for a direct connection.
                  </span>
                </label>
                {settings.copilotEnabled && (
                  <>
                    <label>
                      Copilot poll interval (minutes)
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
                        Used only when the API has no usable entitlement (or
                        reports unlimited). Leave empty to trust the API.
                      </span>
                    </label>
                  </>
                )}
                {settings.githubActivityEnabled && (
                  <label>
                    Activity poll interval (minutes)
                    <input
                      type="number"
                      min={5}
                      max={120}
                      value={settings.githubActivityPollIntervalMins}
                      onChange={(e) =>
                        setSettings({
                          ...settings,
                          githubActivityPollIntervalMins: Math.max(
                            5,
                            Number(e.target.value) || 30,
                          ),
                        })
                      }
                    />
                  </label>
                )}
                {settings.copilotEnabled && (
                  <>
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
                            const next = await api.updateSettings(settings);
                            commitSaved(next);

                            unlisten = await listen<CopilotConnectionTestEvent>(
                              "copilot-connection-test",
                              (e) => {
                                const { phase, message: status } = e.payload;
                                setCopilotTestPhase(phase);
                                setCopilotTestStatus(status);
                                if (phase === "succeeded" || phase === "failed") {
                                  void api
                                    .getSettings()
                                    .then((fresh) => {
                                      setSettings(fresh);
                                      setSaved(fresh);
                                    })
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
                            const refreshed = await api
                              .getSettings()
                              .catch(() => null);
                            if (refreshed) commitSaved(refreshed);
                          }
                        }}
                      >
                        {copilotTesting ? "Testing…" : "Test Copilot connection"}
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
                    {(settings.copilotLastError ||
                      settings.copilotLastSyncedAt) && (
                      <p className="hint">
                        {settings.copilotLastError
                          ? `Copilot last error: ${settings.copilotLastError}`
                          : null}
                        {settings.copilotLastError &&
                        settings.copilotLastSyncedAt
                          ? " · "
                          : null}
                        {settings.copilotLastSyncedAt
                          ? `Copilot last synced: ${new Date(settings.copilotLastSyncedAt).toLocaleString()}`
                          : null}
                      </p>
                    )}
                  </>
                )}
                {settings.githubActivityEnabled &&
                  (settings.githubActivityLastError ||
                    settings.githubActivityLastSyncedAt) && (
                    <p className="hint">
                      {settings.githubActivityLastError
                        ? `Activity last error: ${settings.githubActivityLastError}`
                        : null}
                      {settings.githubActivityLastError &&
                      settings.githubActivityLastSyncedAt
                        ? " · "
                        : null}
                      {settings.githubActivityLastSyncedAt
                        ? `Activity last synced: ${new Date(settings.githubActivityLastSyncedAt).toLocaleString()}`
                        : null}
                    </p>
                  )}
              </>
            )}
          </div>
        </section>

        <section className="settings-card">
          <div className="settings-card-header">
            <h3>Database</h3>
            <p className="hint">Local SQLite file used by TimeTrove.</p>
          </div>
          <div className="settings-card-fields">
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
                    !confirm(
                      "Replace the current database? This cannot be undone.",
                    )
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
        </section>
      </div>
    </div>
  );
}
