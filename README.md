# TimeTrove

Personal, local-only time tracking for developers. Built with Tauri 2 + React + TypeScript.

**Documentation:** [mzbrau.github.io/timetrove](https://mzbrau.github.io/timetrove/)

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

SQLite database lives in the OS app data directory (`timetrove.db`). Reveal / export / import from Settings.

## Releases

Push a version tag to trigger installer builds and a GitHub Release:

```bash
git tag v1.0.0
git push origin v1.0.0
```

The workflow syncs the app version from the tag (`v1.0.0` → `1.0.0`), then builds:

- macOS `.dmg` (Apple Silicon and Intel)
- Windows NSIS setup `.exe`

Installing or upgrading replaces the app binary only. `timetrove.db` stays in the OS app-data directory, so existing tasks and time entries are preserved.

Builds are currently unsigned (macOS uses ad-hoc signing). Expect Gatekeeper / SmartScreen warnings until code signing is configured.

## Documentation site

The user docs live in [`website/`](website/) (Docusaurus) and deploy to GitHub Pages from `main` via [`.github/workflows/docs.yml`](.github/workflows/docs.yml). One-time setup: in the repo's **Settings → Pages**, set the source to **GitHub Actions**.

```bash
pnpm docs:dev    # local preview with hot reload
pnpm docs:build  # production build (fails on broken links)
pnpm docs:shots  # macOS: recapture screenshots from the real app
```

`pnpm docs:shots` seeds a 12-month demo database in `src-tauri/target/demo/` (your real `timetrove.db` is never touched), launches the app in a dev-only demo mode with no network access, and captures every shot listed in [`docs/shots.json`](docs/shots.json) into `website/static/img/screenshots/`. Quit any installed TimeTrove first. Screenshots are stored with Git LFS.

See [plan.md](plan.md) for full product requirements.
