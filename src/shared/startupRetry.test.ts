import { beforeEach, describe, expect, it, vi } from "vitest";

const listen = vi.fn(
  (_event: string, handler: () => void) => {
    // Simulate a late webview that still receives engine-ready promptly.
    queueMicrotask(() => handler());
    return Promise.resolve(() => {});
  },
);

vi.mock("@tauri-apps/api/event", () => ({
  listen: (...args: unknown[]) =>
    listen(...(args as [string, () => void])),
}));

import {
  isUnmanagedStateError,
  resetEngineReadyForTests,
  withStartupRetry,
} from "./startupRetry";

describe("isUnmanagedStateError", () => {
  it("matches Tauri unmanaged-state messages", () => {
    expect(
      isUnmanagedStateError(
        "state not managed for field `engine` on command `list_tasks`. You must call `.manage()` before using this command.",
      ),
    ).toBe(true);
    expect(isUnmanagedStateError("State not managed")).toBe(true);
    expect(isUnmanagedStateError("not managed for field 'engine'")).toBe(true);
  });

  it("rejects unrelated errors", () => {
    expect(isUnmanagedStateError("database is locked")).toBe(false);
    expect(isUnmanagedStateError("Enable Jira sync first")).toBe(false);
    expect(isUnmanagedStateError("")).toBe(false);
  });
});

describe("withStartupRetry", () => {
  beforeEach(() => {
    resetEngineReadyForTests();
    listen.mockClear();
    listen.mockImplementation((_event: string, handler: () => void) => {
      queueMicrotask(() => handler());
      return Promise.resolve(() => {});
    });
  });

  it("returns on first success without waiting for engine-ready", async () => {
    const fn = vi.fn().mockResolvedValue(42);
    await expect(withStartupRetry(fn, { attempts: 3, delayMs: 1 })).resolves.toBe(
      42,
    );
    expect(fn).toHaveBeenCalledTimes(1);
    expect(listen).not.toHaveBeenCalled();
  });

  it("retries unmanaged errors then succeeds", async () => {
    const fn = vi
      .fn()
      .mockRejectedValueOnce(new Error("state not managed for field engine"))
      .mockRejectedValueOnce(new Error("state not managed"))
      .mockResolvedValue("ok");
    await expect(withStartupRetry(fn, { attempts: 5, delayMs: 1 })).resolves.toBe(
      "ok",
    );
    expect(fn).toHaveBeenCalledTimes(3);
    expect(listen).toHaveBeenCalled();
  });

  it("throws immediately for non-unmanaged errors", async () => {
    const fn = vi.fn().mockRejectedValue(new Error("database is locked"));
    await expect(withStartupRetry(fn, { attempts: 5, delayMs: 1 })).rejects.toThrow(
      "database is locked",
    );
    expect(fn).toHaveBeenCalledTimes(1);
    expect(listen).not.toHaveBeenCalled();
  });

  it("throws after exhausting unmanaged retries", async () => {
    const fn = vi
      .fn()
      .mockRejectedValue(new Error("state not managed for field engine"));
    await expect(withStartupRetry(fn, { attempts: 3, delayMs: 1 })).rejects.toThrow(
      "state not managed",
    );
    expect(fn).toHaveBeenCalledTimes(3);
    expect(listen).toHaveBeenCalled();
  });
});
