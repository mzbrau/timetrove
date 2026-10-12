/** Tauri returns this when a command runs before `app.manage()` finishes. */
export function isUnmanagedStateError(error: unknown): boolean {
  const msg = String(error).toLowerCase();
  return msg.includes("state not managed") || msg.includes("not managed for field");
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** Retry briefly while setup is still registering managed engine state. */
export async function withStartupRetry<T>(
  fn: () => Promise<T>,
  options?: { attempts?: number; delayMs?: number },
): Promise<T> {
  const attempts = options?.attempts ?? 10;
  const delayMs = options?.delayMs ?? 40;
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
