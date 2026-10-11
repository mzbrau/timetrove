import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { MainApp } from "./windows/main/MainApp";
import { TrackerApp } from "./windows/tracker/TrackerApp";
import { SwitcherApp } from "./windows/switcher/SwitcherApp";
import { GapApp } from "./windows/gap/GapApp";
import { applyTheme, loadAndApplyTheme } from "./shared/theme";
import "./styles/global.css";

applyTheme("dark");
void loadAndApplyTheme();

const label = getCurrentWebviewWindow().label;

function Root() {
  switch (label) {
    case "tracker":
      return <TrackerApp />;
    case "switcher":
      return <SwitcherApp />;
    case "gap":
      return <GapApp />;
    default:
      return <MainApp />;
  }
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <Root />
  </React.StrictMode>,
);
