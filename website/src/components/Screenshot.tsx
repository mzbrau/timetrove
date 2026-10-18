import React, { useCallback, useEffect, useRef, useState } from "react";
import clsx from "clsx";
import useBaseUrl from "@docusaurus/useBaseUrl";
import styles from "./Screenshot.module.css";

type Props = {
  /** File name under static/img/screenshots, without extension. */
  name: string;
  alt: string;
  caption?: React.ReactNode;
  /** Display width in CSS px; defaults to the column width. Shots are 2x, so half the PNG width is crisp. */
  width?: number;
  /** "bar" renders the floating tracker without a window frame. */
  variant?: "window" | "bar";
};

export default function Screenshot({ name, alt, caption, width, variant = "window" }: Props) {
  const src = useBaseUrl(`/img/screenshots/${name}.png`);
  const [open, setOpen] = useState(false);
  const dialogRef = useRef<HTMLDialogElement>(null);

  useEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog) return;
    if (open && !dialog.open) dialog.showModal();
    if (!open && dialog.open) dialog.close();
  }, [open]);

  const close = useCallback(() => setOpen(false), []);

  return (
    <figure className={clsx(styles.figure, variant === "bar" && styles.bar)}>
      <button
        type="button"
        className={styles.frame}
        style={width ? { maxWidth: width } : undefined}
        onClick={() => setOpen(true)}
        aria-label={`Enlarge screenshot: ${alt}`}
      >
        <img src={src} alt={alt} loading="lazy" decoding="async" />
      </button>
      {caption && <figcaption className={styles.caption}>{caption}</figcaption>}
      <dialog ref={dialogRef} className={styles.dialog} onClose={close} onClick={close}>
        <img src={src} alt={alt} />
      </dialog>
    </figure>
  );
}
