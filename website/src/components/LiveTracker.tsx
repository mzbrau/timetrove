import React, { useEffect, useState } from "react";
import clsx from "clsx";
import styles from "./LiveTracker.module.css";

type Props = {
  task: string;
  color: string;
  /** Seconds already booked today when the page loads. */
  startSeconds: number;
  goalSeconds?: number;
  className?: string;
};

function hms(total: number) {
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  return [h, m, s].map((n) => String(n).padStart(2, "0")).join(":");
}

/** A live replica of the app's tracker bar: the clock ticks and turns gold at the daily goal. */
export default function LiveTracker({ task, color, startSeconds, goalSeconds = 8 * 3600, className }: Props) {
  const [elapsed, setElapsed] = useState(startSeconds);

  useEffect(() => {
    const began = Date.now();
    const id = window.setInterval(() => {
      setElapsed(startSeconds + Math.floor((Date.now() - began) / 1000));
    }, 1000);
    return () => window.clearInterval(id);
  }, [startSeconds]);

  const met = elapsed >= goalSeconds;

  return (
    <div className={clsx(styles.bar, className)} role="img" aria-label={`Tracking ${task}, ${hms(elapsed)} today`}>
      <span className={styles.dot} style={{ background: color, color }} />
      <span className={styles.name}>{task}</span>
      <span className={clsx(styles.total, met && styles.met)}>{hms(elapsed)}</span>
    </div>
  );
}
