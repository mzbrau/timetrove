import { useEffect, useState } from "react";
import { api } from "../../shared/api";
import { fromLocalInput, toLocalInput } from "../../shared/datetime";
import "./snooze.css";

type Mode = "duration" | "end";

function defaultEndLocal(): string {
  const d = new Date();
  d.setHours(d.getHours() + 1, 0, 0, 0);
  return toLocalInput(d.toISOString());
}

export function SnoozeApp() {
  const [mode, setMode] = useState<Mode>("duration");
  const [hours, setHours] = useState(1);
  const [minutes, setMinutes] = useState(0);
  const [endLocal, setEndLocal] = useState(defaultEndLocal);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const onFocus = () => {
      setMode("duration");
      setHours(1);
      setMinutes(0);
      setEndLocal(defaultEndLocal());
      setError(null);
    };
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, []);

  const resolveUntil = (): string | null => {
    if (mode === "duration") {
      const totalMins = Math.max(0, hours) * 60 + Math.max(0, minutes);
      if (totalMins <= 0) {
        setError("Duration must be greater than zero");
        return null;
      }
      return new Date(Date.now() + totalMins * 60_000).toISOString();
    }
    const iso = fromLocalInput(endLocal);
    if (Number.isNaN(Date.parse(iso)) || Date.parse(iso) <= Date.now()) {
      setError("End time must be in the future");
      return null;
    }
    return iso;
  };

  const submit = async () => {
    setError(null);
    const until = resolveUntil();
    if (!until) return;
    try {
      await api.startSnooze(until);
    } catch (e) {
      setError(String(e));
    }
  };

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        void api.closeSnoozePicker();
      } else if (e.key === "Enter") {
        e.preventDefault();
        void submit();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  });

  return (
    <div className="snooze">
      <h1>Snooze prompts</h1>
      <p className="hint">Keep timing on the current task without gap prompts.</p>

      <div className="mode-tabs" role="tablist">
        <button
          type="button"
          className={mode === "duration" ? "active" : ""}
          onClick={() => setMode("duration")}
        >
          Duration
        </button>
        <button
          type="button"
          className={mode === "end" ? "active" : ""}
          onClick={() => setMode("end")}
        >
          End time
        </button>
      </div>

      {mode === "duration" ? (
        <div className="fields">
          <label>
            Hours
            <input
              type="number"
              min={0}
              max={24}
              value={hours}
              onChange={(e) => setHours(Number(e.target.value) || 0)}
            />
          </label>
          <label>
            Minutes
            <input
              type="number"
              min={0}
              max={59}
              value={minutes}
              onChange={(e) => setMinutes(Number(e.target.value) || 0)}
            />
          </label>
        </div>
      ) : (
        <div className="fields single">
          <label>
            Until
            <input
              type="datetime-local"
              value={endLocal}
              onChange={(e) => setEndLocal(e.target.value)}
            />
          </label>
        </div>
      )}

      {error && <p className="error">{error}</p>}

      <div className="footer">
        <button type="button" className="ghost" onClick={() => void api.closeSnoozePicker()}>
          Cancel
        </button>
        <button type="button" className="primary" onClick={() => void submit()}>
          Snooze
        </button>
      </div>
    </div>
  );
}
