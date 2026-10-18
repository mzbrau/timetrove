import React from "react";

/** Renders a key combo like "Mod+Shift+T" as separate keys; "Mod" reads as Cmd / Ctrl. */
export default function Kbd({ children }: { children: string }) {
  const keys = children.split("+").map((k) => (k === "Mod" ? "Cmd/Ctrl" : k));
  return (
    <span className="tt-kbd">
      {keys.map((k, i) => (
        <React.Fragment key={i}>
          {i > 0 && <span className="tt-kbd-plus">+</span>}
          <kbd>{k}</kbd>
        </React.Fragment>
      ))}
    </span>
  );
}
