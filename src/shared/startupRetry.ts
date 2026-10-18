import { listen } from "@tauri-apps/api/event";

/** Tauri returns this when a command runs before `app.manage()` finishes. */
export function isUnmanagedStateError(error: unknown): boolean {
  const msg = String(error).toLowerCase();
  return (
    msg.includes("state not managed") || msg.includes("not managed for field")
  );
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** Default unmanaged retry: ~4s (upgrade migrate can exceed the old 400ms window). */
const DEFAULT_ATTEMPTS = 80;
const DEFAULT_DELAY_MS = 50;
const ENGINE_READY_TIMEOUT_MS = 10_000;

let engineReadyPromise: Promise<void> | null = null;

/** Test seam: clear the shared ready promise between vitest cases. */
export function resetEngineReadyForTests(): void {
  engineReadyPromise = null;
}

/**
 * Resolve once Rust emits `engine-ready` (after `app.manage()`), or after timeout.
 * Shared across callers so parallel invokes wait on one listener.
 */
export function waitForEngineReady(
  timeoutMs = ENGINE_READY_TIMEOUT_MS,
): Promise<void> {
  if (!engineReadyPromise) {
    engineReadyPromise = new Promise((resolve) => {
      let settled = false;
      const done = () => {
        if (settled) return;
        settled = true;
        resolve();
      };

      const timer = setTimeout(done, timeoutMs);

      listen("engine-ready", () => {
        clearTimeout(timer);
        done();
      })
        .then((unlisten) => {
          // If we already timed out, drop the listener.
          if (settled) {
            unlisten();
          }
        })
        .catch(() => {
          // listen unavailable (unit tests / non-Tauri) — do not block.
          clearTimeout(timer);
          done();
        });
    });
  }
  return engineReadyPromise;
}

/** Retry while setup is still registering managed engine state. */
export async function withStartupRetry<T>(
  fn: () => Promise<T>,
  options?: { attempts?: number; delayMs?: number },
): Promise<T> {
  const attempts = options?.attempts ?? DEFAULT_ATTEMPTS;
  const delayMs = options?.delayMs ?? DEFAULT_DELAY_MS;
  let last: unknown;
  for (let i = 0; i < attempts; i++) {
    try {
      return await fn();
    } catch (e) {
      last = e;
      if (!isUnmanagedStateError(e) || i === attempts - 1) {
        throw e;
      }
      await sleep(delayMs);
    }
  }
  throw last;
}
