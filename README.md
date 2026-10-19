# TimeTrove

Local-first time tracking for developers. A small always-on bar, honest idle prompts, and a Monday-start timesheet — everything stays in one SQLite file on your machine.

**[Website](https://www.timetrove.dev/)** · **[Docs](https://www.timetrove.dev/docs/intro)** · **[Download](https://github.com/mzbrau/timetrove/releases)**

## Screenshots

<p align="center">
  <img src="website/static/img/screenshots/tasks-overview.png" alt="TimeTrove main window — tasks overview" width="720" />
  <br />
  <em>Tasks, timesheet, insights, and settings in one window</em>
</p>

<p align="center">
  <img src="website/static/img/screenshots/tracker-tracking.png" alt="Tracker bar while timing a task" width="360" />
  &nbsp;&nbsp;
  <img src="website/static/img/screenshots/gap-prompt.png" alt="Gap attribution prompt" width="280" />
  <br />
  <em>Always-visible tracker · Gap attribution when you return</em>
</p>

## Features

- **Always in view** — compact tracker bar with task colour, name, and daily total
- **Gap attribution** — idle, sleep, and startup gaps ask where the time should go (keep, move, or discard)
- **Quick switch** — `Cmd/Ctrl+Shift+T`, or click the task name on the tracker bar, to search or create a task
- **Timesheet** — Monday-start weekly grid with CSV export
- **Insights** — week, month, and year views of how your time was spent
- **Local only** — no account, no cloud sync; one `timetrove.db` you can back up or delete
- **Optional integrations** — Jira Cloud worklogs, GitHub activity, Copilot usage (all off until you add a token)

## Develop

Requirements: Node.js 20+, pnpm, Rust (stable). Day-to-day development on macOS; Windows is the product target for the docked tracker bar.

```bash
pnpm install
pnpm tauri dev
```

| Window   | Role                                      |
|----------|-------------------------------------------|
| main     | Tasks, timesheet, entries, settings       |
| tracker  | Always-visible compact bar                |
| switcher | Global shortcut task search               |
| gap      | Idle / sleep / startup attribution prompt |

## Releases

Push a version tag to build installers and publish a GitHub Release:

```bash
git tag v1.0.0
git push origin v1.0.0
```

Produces a macOS `.dmg` (Apple Silicon and Intel) and a Windows NSIS `.exe`. Installing replaces the app binary only; `timetrove.db` stays in the OS app-data directory. Builds are currently unsigned.

## Documentation site

User docs live in [`website/`](website/) (Docusaurus) and deploy from `main` to [www.timetrove.dev](https://www.timetrove.dev/).

```bash
pnpm docs:dev    # local preview
pnpm docs:build  # production build (fails on broken links)
pnpm docs:shots  # macOS: recapture screenshots from the real app
```

`pnpm docs:shots` seeds a demo database under `src-tauri/target/demo/` (your real `timetrove.db` is never touched) and captures the shots listed in [`docs/shots.json`](docs/shots.json). Quit any running TimeTrove first. Screenshots are stored with Git LFS.
