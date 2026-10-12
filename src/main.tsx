import React, { Component, type ErrorInfo, type ReactNode } from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { applyTheme, loadAndApplyTheme } from "./shared/theme";
import "./styles/global.css";

applyTheme("dark");
void loadAndApplyTheme();

const label = getCurrentWebviewWindow().label;

type BootErrorProps = { children: ReactNode };
type BootErrorState = { error: Error | null };

class BootErrorBoundary extends Component<BootErrorProps, BootErrorState> {
  state: BootErrorState = { error: null };

  static getDerivedStateFromError(error: Error): BootErrorState {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("TimeTrove UI crashed", error, info.componentStack);
  }

  render() {
    if (this.state.error) {
      return (
        <div
          style={{
            padding: 24,
            color: "#e8eef2",
            fontFamily: "system-ui, sans-serif",
            whiteSpace: "pre-wrap",
          }}
        >
          <strong>TimeTrove failed to render</strong>
          <p style={{ marginTop: 12, color: "#e07a6a" }}>
            {this.state.error.message}
          </p>
        </div>
      );
    }
    return this.props.children;
  }
}

function loadWindowApp(windowLabel: string) {
  switch (windowLabel) {
    case "tracker":
      return import("./windows/tracker/TrackerApp").then((m) => (
        <m.TrackerApp />
      ));
    case "switcher":
      return import("./windows/switcher/SwitcherApp").then((m) => (
        <m.SwitcherApp />
      ));
    case "gap":
      return import("./windows/gap/GapApp").then((m) => <m.GapApp />);
    case "snooze":
      return import("./windows/snooze/SnoozeApp").then((m) => <m.SnoozeApp />);
    default:
      return import("./windows/main/MainApp").then((m) => <m.MainApp />);
  }
}

const rootEl = document.getElementById("root");
if (!rootEl) {
  throw new Error("Missing #root element");
}

const root = ReactDOM.createRoot(rootEl);

root.render(
  <React.StrictMode>
    <BootErrorBoundary>
      <div style={{ padding: 16, color: "#8b9aab", fontFamily: "system-ui" }}>
        Loading…
      </div>
    </BootErrorBoundary>
  </React.StrictMode>,
);

void loadWindowApp(label)
  .then((app) => {
    root.render(
      <React.StrictMode>
        <BootErrorBoundary>{app}</BootErrorBoundary>
      </React.StrictMode>,
    );
  })
  .catch((err: unknown) => {
    const message = err instanceof Error ? err.message : String(err);
    root.render(
      <React.StrictMode>
        <BootErrorBoundary>
          <div
            style={{
              padding: 24,
              color: "#e8eef2",
              fontFamily: "system-ui, sans-serif",
            }}
          >
            <strong>TimeTrove failed to load</strong>
            <p style={{ marginTop: 12, color: "#e07a6a" }}>{message}</p>
          </div>
        </BootErrorBoundary>
      </React.StrictMode>,
    );
  });
