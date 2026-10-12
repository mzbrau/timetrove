import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "./api";
import { isUnmanagedStateError, withStartupRetry } from "./startupRetry";
import type { TrackerState } from "./types";

export function useTrackerState() {
  const [state, setState] = useState<TrackerState | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let disposed = false;
    let unlistenState: (() => void) | undefined;
    let unlistenError: (() => void) | undefined;

    withStartupRetry(() => api.getTrackerState())
      .then((s) => {
        if (!disposed) {
          setState(s);
          setError(null);
        }
      })
      .catch((e) => {
        if (!disposed) setError(String(e));
      });

    listen<TrackerState>("tracker-state", (e) => {
      setState(e.payload);
      if (e.payload.dbError) {
        setError(e.payload.dbError);
      } else {
        // Clear sticky startup race errors once engine is emitting.
        setError((prev) => (prev && isUnmanagedStateError(prev) ? null : prev));
      }
    })
      .then((fn) => {
        if (disposed) {
          fn();
          return;
        }
        unlistenState = fn;
      })
      .catch((e) => {
        if (!disposed) setError(String(e));
      });

    listen<string>("db-error", (e) => {
      setError(e.payload);
    })
      .then((fn) => {
        if (disposed) {
          fn();
          return;
        }
        unlistenError = fn;
      })
      .catch((e) => {
        if (!disposed) setError(String(e));
      });

    return () => {
      disposed = true;
      unlistenState?.();
      unlistenError?.();
    };
  }, []);

  return { state, error, setError };
}
