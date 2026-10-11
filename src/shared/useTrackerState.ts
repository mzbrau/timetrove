import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "./api";
import type { TrackerState } from "./types";

export function useTrackerState() {
  const [state, setState] = useState<TrackerState | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let unlistenState: (() => void) | undefined;
    let unlistenError: (() => void) | undefined;

    api
      .getTrackerState()
      .then(setState)
      .catch((e) => setError(String(e)));

    listen<TrackerState>("tracker-state", (e) => {
      setState(e.payload);
      if (e.payload.dbError) setError(e.payload.dbError);
    }).then((fn) => {
      unlistenState = fn;
    });

    listen<string>("db-error", (e) => {
      setError(e.payload);
    }).then((fn) => {
      unlistenError = fn;
    });

    return () => {
      unlistenState?.();
      unlistenError?.();
    };
  }, []);

  return { state, error, setError };
}
