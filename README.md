# Time Keeper

Personal, local-only time tracking for developers. Built with Tauri 2 + React + TypeScript.

## Requirements

- Node.js 20+
- pnpm
- Rust (stable)
- macOS for day-to-day development (floating tracker bar)
- Windows for the product target (docked bar above the taskbar + system tray)

## Setup

```bash
pnpm install
pnpm tauri dev
```

## Windows (labels)

| Window   | Role                                      |
|----------|-------------------------------------------|
| main     | Tasks, timesheet, entries, settings       |
| tracker  | Always-visible compact bar                |
| switcher | Global shortcut task search               |
| gap      | Idle / sleep / startup attribution prompt |

Default switch shortcut: `Cmd/Ctrl+Shift+T`.

## Data

SQLite database lives in the OS app data directory (`time-keeper.db`). Reveal / export / import from Settings.

See [plan.md](plan.md) for full product requirements.
