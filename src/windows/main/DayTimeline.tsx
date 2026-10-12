import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
  type PointerEvent as ReactPointerEvent,
} from "react";
import { api } from "../../shared/api";
import { formatHm, normalizeHexColor } from "../../shared/format";
import type { Task, TimeSlice } from "../../shared/types";
import { TaskPicker } from "./TaskPicker";
import {
  bookedExtent,
  dayDurationMs,
  dayMsToIso,
  fitScrollRatio,
  fitZoom,
  formatDayClock,
  gapsFromRanges,
  hourLabels,
  localDayStart,
  msInDay,
  nextZoomLevel,
  prevZoomLevel,
  resizeNeighborBounds,
  snapMs,
  zoomSnapMins,
} from "./timelineMath";

type PendingCreate = {
  startMs: number;
  endMs: number;
  taskId: string;
};

type DragState =
  | {
      kind: "resize";
      sliceId: string;
      edge: "start" | "end";
      otherMs: number;
      previewMs: number;
      minBound: number;
      maxBound: number;
    }
  | {
      kind: "gap";
      gapStart: number;
      gapEnd: number;
      originMs: number;
      currentMs: number;
      moved: boolean;
    };

type Props = {
  day: string;
  slices: TimeSlice[];
  tasks: Task[];
  nowMs: number;
  onChanged: () => Promise<void>;
  onError: (message: string) => void;
};

function taskById(tasks: Task[], id: string) {
  return tasks.find((t) => t.id === id);
}

export function DayTimeline({
  day,
  slices,
  tasks,
  nowMs,
  onChanged,
  onError,
}: Props) {
  const trackRef = useRef<HTMLDivElement>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const dragRef = useRef<DragState | null>(null);
  const gapPointerRef = useRef(false);
  const rangesRef = useRef<
    { slice: TimeSlice; startMs: number; endMs: number }[]
  >([]);
  const pendingScrollRatioRef = useRef<number | null>(null);
  const [zoom, setZoom] = useState(1);
  const [drag, setDrag] = useState<DragState | null>(null);
  const [pending, setPending] = useState<PendingCreate | null>(null);
  const [tooltip, setTooltip] = useState<{
    x: number;
    y: number;
    text: string;
  } | null>(null);
  const [pointerPos, setPointerPos] = useState<{ x: number; y: number } | null>(
    null,
  );

  const dayLength = dayDurationMs(day);
  const dayLengthRef = useRef(dayLength);
  dayLengthRef.current = dayLength;

  const activeTasks = useMemo(
    () => tasks.filter((t) => !t.archived),
    [tasks],
  );

  const stepMins = zoomSnapMins(zoom);
  const stepMinsRef = useRef(stepMins);
  stepMinsRef.current = stepMins;

  const ranges = useMemo(() => {
    return slices.map((slice) => {
      const startMs = msInDay(day, slice.start);
      const endMs = slice.end
        ? msInDay(day, slice.end)
        : Math.max(
            startMs,
            Math.min(dayLength, nowMs - localDayStart(day).getTime()),
          );
      return { slice, startMs, endMs };
    });
  }, [slices, day, nowMs, dayLength]);
  rangesRef.current = ranges;

  const clampResizePreview = (
    edge: "start" | "end",
    previewMs: number,
    otherMs: number,
    minBound: number,
    maxBound: number,
  ) => {
    const step = stepMinsRef.current;
    if (edge === "start") {
      const maxStart = otherMs - step * 60_000;
      return Math.max(minBound, Math.min(maxStart, previewMs));
    }
    const minEnd = otherMs + step * 60_000;
    return Math.max(minEnd, Math.min(maxBound, previewMs));
  };

  const displayRanges = useMemo(() => {
    if (!drag || drag.kind !== "resize") return ranges;
    return ranges.map((r) => {
      if (r.slice.id !== drag.sliceId) return r;
      if (drag.edge === "start") {
        return {
          ...r,
          startMs: clampResizePreview(
            "start",
            drag.previewMs,
            drag.otherMs,
            drag.minBound,
            drag.maxBound,
          ),
        };
      }
      return {
        ...r,
        endMs: clampResizePreview(
          "end",
          drag.previewMs,
          drag.otherMs,
          drag.minBound,
          drag.maxBound,
        ),
      };
    });
  }, [ranges, drag]);

  const gaps = useMemo(
    () =>
      gapsFromRanges(
        displayRanges.map((r) => ({ startMs: r.startMs, endMs: r.endMs })),
        dayLength,
      ),
    [displayRanges, dayLength],
  );

  const labels = hourLabels(zoom);

  const resizeTooltip = useMemo(() => {
    if (!drag || drag.kind !== "resize" || !pointerPos) return null;
    const startMs =
      drag.edge === "start"
        ? clampResizePreview(
            "start",
            drag.previewMs,
            drag.otherMs,
            drag.minBound,
            drag.maxBound,
          )
        : drag.otherMs;
    const endMs =
      drag.edge === "end"
        ? clampResizePreview(
            "end",
            drag.previewMs,
            drag.otherMs,
            drag.minBound,
            drag.maxBound,
          )
        : drag.otherMs;
    const slice = slices.find((s) => s.id === drag.sliceId);
    const name = taskById(tasks, slice?.taskId ?? "")?.name ?? "Task";
    const edgeLabel =
      drag.edge === "start"
        ? `Start ${formatDayClock(day, startMs)}`
        : `End ${formatDayClock(day, endMs)}`;
    return {
      x: pointerPos.x,
      y: pointerPos.y,
      text: `${name} · ${formatHm(endMs - startMs)} · ${edgeLabel}`,
    };
  }, [drag, day, pointerPos, slices, tasks]);

  const clientXToMs = (clientX: number) => {
    const el = trackRef.current;
    if (!el) return 0;
    const rect = el.getBoundingClientRect();
    const length = dayLengthRef.current;
    const ratio = (clientX - rect.left) / rect.width;
    return snapMs(
      Math.max(0, Math.min(length, ratio * length)),
      stepMinsRef.current,
    );
  };

  const updateDrag = (next: DragState | null) => {
    dragRef.current = next;
    setDrag(next);
  };

  useEffect(() => {
    dragRef.current = null;
    setDrag(null);
    setPending(null);
    setTooltip(null);
    setPointerPos(null);
    setZoom(1);
    pendingScrollRatioRef.current = null;
    if (scrollRef.current) scrollRef.current.scrollLeft = 0;
  }, [day]);

  useEffect(() => {
    const ratio = pendingScrollRatioRef.current;
    if (ratio == null) return;
    const el = scrollRef.current;
    if (!el) return;
    const apply = () => {
      pendingScrollRatioRef.current = null;
      el.scrollLeft = ratio * el.scrollWidth;
    };
    const id = requestAnimationFrame(apply);
    return () => cancelAnimationFrame(id);
  }, [zoom]);

  useEffect(() => {
    if (!drag) return;

    const onMove = (e: PointerEvent) => {
      const current = dragRef.current;
      if (!current) return;
      const ms = clientXToMs(e.clientX);
      const step = stepMinsRef.current;
      if (current.kind === "resize") {
        setPointerPos({ x: e.clientX, y: e.clientY });
        updateDrag({
          ...current,
          previewMs: clampResizePreview(
            current.edge,
            ms,
            current.otherMs,
            current.minBound,
            current.maxBound,
          ),
        });
        return;
      }
      const clamped = Math.max(current.gapStart, Math.min(current.gapEnd, ms));
      const moved =
        current.moved || Math.abs(clamped - current.originMs) >= step * 60_000;
      updateDrag({ ...current, currentMs: clamped, moved });
    };

    const onUp = () => {
      const current = dragRef.current;
      updateDrag(null);
      setPointerPos(null);
      if (!current) return;
      void finishDrag(current);
    };

    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    window.addEventListener("pointercancel", onUp);
    return () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("pointercancel", onUp);
    };
  }, [drag, day, activeTasks]);

  const finishDrag = async (current: DragState) => {
    const step = stepMinsRef.current;
    if (current.kind === "resize") {
      const startMs =
        current.edge === "start"
          ? clampResizePreview(
              "start",
              current.previewMs,
              current.otherMs,
              current.minBound,
              current.maxBound,
            )
          : current.otherMs;
      const endMs =
        current.edge === "end"
          ? clampResizePreview(
              "end",
              current.previewMs,
              current.otherMs,
              current.minBound,
              current.maxBound,
            )
          : current.otherMs;
      const snappedStart = Math.max(
        current.minBound,
        snapMs(startMs, step),
      );
      const snappedEnd = Math.min(current.maxBound, snapMs(endMs, step));
      if (snappedEnd <= snappedStart) return;
      try {
        await api.updateSlice({
          id: current.sliceId,
          start: dayMsToIso(day, snappedStart),
          end: dayMsToIso(day, snappedEnd),
        });
        await onChanged();
      } catch (err) {
        onError(String(err));
      }
      return;
    }

    const first = activeTasks[0]?.id ?? "";
    if (!current.moved) {
      setPending({
        startMs: current.gapStart,
        endMs: current.gapEnd,
        taskId: first,
      });
      return;
    }

    let a = current.originMs;
    let b = current.currentMs;
    if (b < a) [a, b] = [b, a];
    a = Math.max(current.gapStart, snapMs(a, step));
    b = Math.min(current.gapEnd, snapMs(b, step));
    if (b - a < step * 60_000) {
      b = Math.min(current.gapEnd, a + step * 60_000);
    }
    if (b <= a) return;
    setPending({ startMs: a, endMs: b, taskId: first });
  };

  const beginResize = (
    e: ReactPointerEvent,
    sliceId: string,
    edge: "start" | "end",
    startMs: number,
    endMs: number,
  ) => {
    e.preventDefault();
    e.stopPropagation();
    setTooltip(null);
    setPointerPos({ x: e.clientX, y: e.clientY });
    const { minBound, maxBound } = resizeNeighborBounds(
      rangesRef.current.map((r) => ({
        id: r.slice.id,
        startMs: r.startMs,
        endMs: r.endMs,
      })),
      sliceId,
      startMs,
      endMs,
      dayLengthRef.current,
    );
    updateDrag({
      kind: "resize",
      sliceId,
      edge,
      otherMs: edge === "start" ? endMs : startMs,
      previewMs: edge === "start" ? startMs : endMs,
      minBound,
      maxBound,
    });
  };

  const beginGap = (e: ReactPointerEvent, gapStart: number, gapEnd: number) => {
    if (pending) return;
    e.preventDefault();
    const ms = clientXToMs(e.clientX);
    const clamped = Math.max(gapStart, Math.min(gapEnd, ms));
    updateDrag({
      kind: "gap",
      gapStart,
      gapEnd,
      originMs: clamped,
      currentMs: clamped,
      moved: false,
    });
  };

  const openGapPending = (gapStart: number, gapEnd: number) => {
    if (pending || drag) return;
    setPending({
      startMs: gapStart,
      endMs: gapEnd,
      taskId: activeTasks[0]?.id ?? "",
    });
  };

  const nudgeEdge = async (
    sliceId: string,
    edge: "start" | "end",
    startMs: number,
    endMs: number,
    direction: -1 | 1,
  ) => {
    const step = stepMinsRef.current * 60_000;
    const { minBound, maxBound } = resizeNeighborBounds(
      rangesRef.current.map((r) => ({
        id: r.slice.id,
        startMs: r.startMs,
        endMs: r.endMs,
      })),
      sliceId,
      startMs,
      endMs,
      dayLengthRef.current,
    );
    let nextStart = startMs;
    let nextEnd = endMs;
    if (edge === "start") {
      nextStart = clampResizePreview(
        "start",
        startMs + direction * step,
        endMs,
        minBound,
        maxBound,
      );
    } else {
      nextEnd = clampResizePreview(
        "end",
        endMs + direction * step,
        startMs,
        minBound,
        maxBound,
      );
    }
    if (nextStart === startMs && nextEnd === endMs) return;
    try {
      await api.updateSlice({
        id: sliceId,
        start: dayMsToIso(day, nextStart),
        end: dayMsToIso(day, nextEnd),
      });
      await onChanged();
    } catch (err) {
      onError(String(err));
    }
  };

  const onHandleKeyDown = (
    e: ReactKeyboardEvent,
    sliceId: string,
    edge: "start" | "end",
    startMs: number,
    endMs: number,
  ) => {
    if (e.key !== "ArrowLeft" && e.key !== "ArrowRight") return;
    e.preventDefault();
    void nudgeEdge(
      sliceId,
      edge,
      startMs,
      endMs,
      e.key === "ArrowLeft" ? -1 : 1,
    );
  };

  const extent = useMemo(
    () =>
      bookedExtent(
        displayRanges.map((r) => ({ startMs: r.startMs, endMs: r.endMs })),
      ),
    [displayRanges],
  );

  const zoomIn = () => {
    const next = nextZoomLevel(zoom);
    if (next != null) setZoom(next);
  };
  const zoomOut = () => {
    const prev = prevZoomLevel(zoom);
    if (prev != null) setZoom(prev);
  };
  const fitToBooked = () => {
    if (!extent) return;
    const nextZoom = fitZoom(dayLength, extent);
    const ratio = fitScrollRatio(extent, dayLength);
    pendingScrollRatioRef.current = ratio;
    if (nextZoom === zoom) {
      requestAnimationFrame(() => {
        const el = scrollRef.current;
        if (!el) return;
        pendingScrollRatioRef.current = null;
        el.scrollLeft = ratio * el.scrollWidth;
      });
      return;
    }
    setZoom(nextZoom);
  };

  const zoomLabel =
    Number.isInteger(zoom) || Math.abs(zoom - Math.round(zoom)) < 0.05
      ? String(Math.round(zoom))
      : zoom.toFixed(1);

  const draftGap =
    drag?.kind === "gap" && drag.moved
      ? {
          startMs: Math.min(drag.originMs, drag.currentMs),
          endMs: Math.max(drag.originMs, drag.currentMs),
        }
      : null;

  const activeTip = resizeTooltip ?? tooltip;

  return (
    <div className="day-timeline">
      <div className="day-timeline-toolbar">
        <span className="muted">Day timeline</span>
        <div className="row-actions">
          <button
            type="button"
            onClick={zoomOut}
            disabled={prevZoomLevel(zoom) == null}
          >
            −
          </button>
          <span className="mono">{zoomLabel}×</span>
          <button
            type="button"
            onClick={zoomIn}
            disabled={nextZoomLevel(zoom) == null}
          >
            +
          </button>
          <button
            type="button"
            onClick={fitToBooked}
            disabled={!extent}
            title="Zoom to booked time"
          >
            Fit
          </button>
        </div>
      </div>

      <div ref={scrollRef} className="day-timeline-scroll">
        <div
          className="day-timeline-inner"
          style={{ width: `${zoom * 100}%` }}
        >
          <div className="day-timeline-labels">
            {labels.map((h) => (
              <span
                key={h}
                className="day-timeline-label"
                style={{ left: `${(h / 24) * 100}%` }}
              >
                {String(h).padStart(2, "0")}
              </span>
            ))}
          </div>
          <div ref={trackRef} className="day-timeline-track">
            {gaps.map((gap) => (
              <button
                key={`gap-${gap.startMs}-${gap.endMs}`}
                type="button"
                className="day-timeline-gap"
                style={{
                  left: `${(gap.startMs / dayLength) * 100}%`,
                  width: `${((gap.endMs - gap.startMs) / dayLength) * 100}%`,
                }}
                aria-label="Empty time"
                onPointerDown={(e) => {
                  gapPointerRef.current = true;
                  beginGap(e, gap.startMs, gap.endMs);
                }}
                onClick={() => {
                  // Pointer path already handled via drag finish; click is for keyboard.
                  if (gapPointerRef.current) {
                    gapPointerRef.current = false;
                    return;
                  }
                  openGapPending(gap.startMs, gap.endMs);
                }}
              />
            ))}

            {displayRanges.map(({ slice, startMs, endMs }) => {
              const task = taskById(tasks, slice.taskId);
              const color = normalizeHexColor(task?.color ?? "#0f766e");
              const dur = endMs - startMs;
              const running = !slice.end;
              return (
                <div
                  key={slice.id}
                  className="day-timeline-seg"
                  style={{
                    left: `${(startMs / dayLength) * 100}%`,
                    width: `${(Math.max(dur, 60_000) / dayLength) * 100}%`,
                    backgroundColor: color,
                  }}
                  onMouseEnter={(e) =>
                    setTooltip({
                      x: e.clientX,
                      y: e.clientY,
                      text: `${task?.name ?? "Task"} · ${formatHm(dur)}${
                        running ? " (running)" : ""
                      }`,
                    })
                  }
                  onMouseMove={(e) =>
                    setTooltip((t) =>
                      t ? { ...t, x: e.clientX, y: e.clientY } : t,
                    )
                  }
                  onMouseLeave={() => setTooltip(null)}
                >
                  {!running && (
                    <>
                      <button
                        type="button"
                        className="day-timeline-handle start"
                        aria-label="Resize start"
                        onPointerDown={(e) =>
                          beginResize(e, slice.id, "start", startMs, endMs)
                        }
                        onKeyDown={(e) =>
                          onHandleKeyDown(e, slice.id, "start", startMs, endMs)
                        }
                      />
                      <button
                        type="button"
                        className="day-timeline-handle end"
                        aria-label="Resize end"
                        onPointerDown={(e) =>
                          beginResize(e, slice.id, "end", startMs, endMs)
                        }
                        onKeyDown={(e) =>
                          onHandleKeyDown(e, slice.id, "end", startMs, endMs)
                        }
                      />
                    </>
                  )}
                </div>
              );
            })}

            {draftGap && (
              <div
                className="day-timeline-draft"
                style={{
                  left: `${(draftGap.startMs / dayLength) * 100}%`,
                  width: `${(
                    (draftGap.endMs - draftGap.startMs) /
                    dayLength
                  ) * 100}%`,
                }}
              />
            )}
          </div>
        </div>
      </div>

      {pending && (
        <div className="day-timeline-pending inline-action">
          <span className="muted">
            New entry {formatDayClock(day, pending.startMs)} –{" "}
            {formatDayClock(day, pending.endMs)} ·{" "}
            {formatHm(pending.endMs - pending.startMs)}
          </span>
          <TaskPicker
            tasks={activeTasks}
            value={pending.taskId}
            onChange={(taskId) => setPending({ ...pending, taskId })}
          />
          <div className="row-actions">
            <button
              type="button"
              className="btn"
              disabled={!pending.taskId || pending.endMs <= pending.startMs}
              onClick={async () => {
                try {
                  await api.createSlice({
                    taskId: pending.taskId,
                    start: dayMsToIso(day, pending.startMs),
                    end: dayMsToIso(day, pending.endMs),
                  });
                  setPending(null);
                  await onChanged();
                } catch (err) {
                  onError(String(err));
                }
              }}
            >
              Add
            </button>
            <button type="button" onClick={() => setPending(null)}>
              Cancel
            </button>
          </div>
        </div>
      )}

      {activeTip && (
        <div
          className="day-timeline-tooltip"
          style={{ left: activeTip.x + 12, top: activeTip.y + 12 }}
        >
          {activeTip.text}
        </div>
      )}
    </div>
  );
}
