import { listen } from "@tauri-apps/api/event";
import { useEffect, useRef } from "react";

/** UI state requested by the dev-only screenshot runner (`demo-shot` event). */
export type DemoShot = {
  tab?: string;
  task?: string;
  showArchived?: boolean;
  weekOffset?: number;
  /** Entries day: `"lastWorkday"` = most recent Mon–Fri before today. */
  entriesDay?: "today" | "lastWorkday";
  insightsPreset?: string;
  scrollTo?: string;
  query?: string;
  /** Force the switcher create-confirmation banner (after an unmatched query). */
  confirmCreate?: boolean;
  gapMode?: "choose" | "search";
  hover?: boolean;
};

type Handler = (shot: DemoShot) => void;

let lastShot: DemoShot | null = null;
const handlers = new Set<Handler>();

if (import.meta.env.DEV) {
  // Capture runs often happen with the app behind other windows; hidden pages pause
  // requestAnimationFrame, which would leave chart animations stuck at frame zero.
  const nativeRaf = window.requestAnimationFrame.bind(window);
  const nativeCancel = window.cancelAnimationFrame.bind(window);
  // Fallback frames use setTimeout; keep their IDs separate so cancelAnimationFrame
  // does not clear unrelated timers that share the same numeric pool.
  const fallbackTimeouts = new Set<number>();
  // Hidden pages also throttle timers, so each fallback frame advances a virtual clock
  // by 250ms to finish animations in a handful of ticks.
  let virtualNow = 0;
  window.requestAnimationFrame = (cb) => {
    if (!(lastShot && document.hidden)) return nativeRaf(cb);
    const id = window.setTimeout(() => {
      fallbackTimeouts.delete(id);
      virtualNow = Math.max(virtualNow + 250, performance.now());
      cb(virtualNow);
    }, 16);
    fallbackTimeouts.add(id);
    return id;
  };
  window.cancelAnimationFrame = (id) => {
    if (fallbackTimeouts.has(id)) {
      fallbackTimeouts.delete(id);
      window.clearTimeout(id);
      return;
    }
    nativeCancel(id);
  };

  void listen<DemoShot>("demo-shot", (e) => {
    if (!lastShot) {
      // CSS transitions also stall on hidden pages (e.g. the tracker's hover stop button).
      const style = document.createElement("style");
      style.textContent = "*, *::before, *::after { transition: none !important; }";
      document.head.appendChild(style);
    }
    lastShot = e.payload ?? {};
    handlers.forEach((h) => h(lastShot!));
  });
}

/**
 * Dev builds only: run `handler` for each screenshot step, and once on mount with the
 * latest step so panels mounted by a tab change still pick up their state.
 */
export function useDemoShot(handler: Handler) {
  const ref = useRef(handler);
  ref.current = handler;
  useEffect(() => {
    if (!import.meta.env.DEV) return;
    const h: Handler = (shot) => ref.current(shot);
    handlers.add(h);
    if (lastShot) h(lastShot);
    return () => {
      handlers.delete(h);
    };
  }, []);
}

/** Scroll the first heading or field label whose text starts with `text` to the top of its scroller. */
export function scrollToHeading(text: string) {
  const headings = Array.from(document.querySelectorAll("h2, h3, h4, label"));
  const target = headings.find((h) =>
    (h.textContent ?? "").trim().toLowerCase().startsWith(text.toLowerCase()),
  );
  target?.scrollIntoView({ block: "start" });
}
