import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { api } from "./api";

export type Theme = "dark" | "light";

const WINDOW_BG: Record<Theme, [number, number, number, number]> = {
  dark: [18, 22, 26, 255],
  light: [238, 241, 244, 255],
};

export function applyTheme(theme: Theme) {
  document.documentElement.setAttribute("data-theme", theme);
  const meta = document.querySelector('meta[name="color-scheme"]');
  if (meta) {
    meta.setAttribute("content", theme);
  }

  const win = getCurrentWebviewWindow();
  // Overlay windows stay transparent; only paint the main window chrome.
  if (win.label === "main") {
    void win.setBackgroundColor(WINDOW_BG[theme]).catch(() => undefined);
  }
}

export async function loadAndApplyTheme() {
  try {
    const settings = await api.getSettings();
    applyTheme(settings.theme === "light" ? "light" : "dark");
  } catch {
    applyTheme("dark");
  }
}
