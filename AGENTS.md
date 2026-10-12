# TimeTrove — agent guide

Personal, single-user, **local-only** time tracking for a developer’s machine. Goal: accurate day-to-day tracking with minimal friction, then a clean Monday-start weekly timesheet.

Inspired by Grindstone (work items + time slices + idle correction). Product requirements live in [`plan.md`](plan.md). Treat that doc as product intent; treat the code as runtime authority when they diverge.

## Non-goals (v1)

Do not add these unless the user explicitly expands scope:

- Charts / insights beyond the timesheet grid
- External timesheet / ERP submission (beyond optional Jira Cloud worklogs)
- Multi-user, cloud sync, or multi-machine
- In-app custom property engine
- Decimal-hour CSV / reporting formats
- Jira Server/DC, OAuth, multi-issue-per-task, or inbound Jira → TimeTrove sync

## Stack

- **Tauri 2** + **React** + **TypeScript** + **Vite**
- **SQLite** for all app data (no cloud dependency)
- Single UI codebase; OS shell differences limited to always-visible tracker chrome

| Role | Platform |
|------|----------|
| Product target | **Windows** — docked always-on-top bar above the taskbar near the tray + system tray fallback |
| Dev / test harness | **macOS** — floating always-on-top compact bar (draggable; can hide to menu bar / tray) |

## Identifiers

| What | Value |
|------|--------|
| Product name | `TimeTrove` |
| npm / Cargo package | `time-trove` |
| Rust lib crate | `time_trove_lib` |
| Tauri identifier | `com.mzbrau.timetrove` |
| Database file | `timetrove.db` (OS app-data directory) |

## Architecture

### Windows (webview labels)

One Vite entry ([`src/main.tsx`](src/main.tsx)); `getCurrentWebviewWindow().label` selects the root:

| Label | Role | UI |
|-------|------|-----|
| `main` | Tasks, timesheet, entries, settings, day timeline | [`src/windows/main/`](src/windows/main/) |
| `tracker` | Always-visible compact bar (color, task, daily total; default `HH:MM`) | [`src/windows/tracker/`](src/windows/tracker/) |
| `switcher` | Global-shortcut task search / create | [`src/windows/switcher/`](src/windows/switcher/) |
| `gap` | Idle / sleep / startup attribution prompt | [`src/windows/gap/`](src/windows/gap/) |

Default task-switch shortcut: `Cmd/Ctrl+Shift+T`.

### Rust (`src-tauri/src/`)

| Module | Responsibility |
|--------|----------------|
| [`engine/`](src-tauri/src/engine/) | Phase machine, tracking, gaps, checkpoints, midnight split, window show/position helpers |
| [`commands/`](src-tauri/src/commands/) | Thin `#[tauri::command]` wrappers → engine/db; emit `tracker-state` |
| [`db/`](src-tauri/src/db/) | SQLite migrate + repo |
| [`domain/`](src-tauri/src/domain/) | Models, time math, formatting, task colors |
| [`jira/`](src-tauri/src/jira/) | Optional Jira Cloud worklog client + watermark sync (hourly + stop/switch flush) |
| [`chrome/`](src-tauri/src/chrome/) | System tray / OS chrome |

Background loops (idle check, checkpoint, midnight split, Jira hourly sync, state emit) are wired from [`lib.rs`](src-tauri/src/lib.rs).

### Frontend (`src/`)

- [`src/windows/*`](src/windows/) — per-window UI
- [`src/shared/`](src/shared/) — `api.ts` (invoke), `types.ts`, `useTrackerState.ts`, `format.ts`, `datetime.ts`, `theme.ts`
- State push: events `tracker-state` and `db-error`

Prefer extending shared helpers over duplicating format or time math on either side of the FFI boundary.

## Domain model

### Task

Flat list (no projects): `name`, `color`, `description`, `notes`, `archived`, optional `jira_issue_key` / `jira_reported_ms`.

- Prefer unique task names
- Colors auto-assigned to stay distinct; user-editable
- Archived: hidden from switcher / start options; still appear on timesheets/history when they have time
- Optional Jira Cloud link: when sync is enabled in settings, a task may store one issue key; `jira_reported_ms` is the watermark of time already posted as worklogs

### Time slice

Contiguous stretch on one task: `start`, `end` (null while running), `task_id`.

- At most **one** running slice
- Stop ends the slice with no attribution prompt
- Switch closes current slice and opens a new one
- At **local midnight**, auto-split: end at 24:00, continue same task on a new slice

### Discard log

Discarded gap time is **not** attached to a task. Aggregate discard log for audit only; excluded from daily totals and timesheets.

## Engine phases

```text
Stopped  <->  Tracking { taskId, sliceId }
                 |
                 v
         NeedsAttribution { gapStart, previousTaskId }
```

Start / stop / switch are blocked while `NeedsAttribution` is pending.

### Gap attribution (idle / sleep / startup)

One shared flow:

1. Time up to gap start stays on the previous task
2. From gap start until the user answers = unattributed gap
3. User **must** choose; Escape does not dismiss

| Key | Action |
|-----|--------|
| `1` | Assign gap to **previous** task, resume previous |
| `2` | Assign gap to **another** task (search), then start it |
| `3` | **Discard** gap (log only), resume previous from now |

Gap duration UI uses short / medium / long color bands (defaults in one place for tuning).

### Minute checkpointing (`last_good_at`)

While tracking, persist `last_good_at` about once per minute (settings key via repo). On startup / sleep wake / crash:

- Prefer closing work at `last_good_at` (not slice start)
- Gap prompt covers only the unattributed period since the last checkpoint
- Cleared on stop, gap begin, and similar resets

See [`engine/mod.rs`](src-tauri/src/engine/mod.rs) (`checkpoint_if_due`, `reconcile_on_startup`).

## UX rules agents should preserve

- Always-visible tracker: color + task name when tracking (dot pulses while active); unmistakable **not tracking** when stopped; daily total defaults to `HH:MM`; color flips at daily goal (default 8h); hover reveals a stop control left of the time
- Main window daily total: live `HH:MM:SS` via client interpolation; reset `receivedAt` **during render** when tracker state identity changes (avoid the every-~5s jump/snap from a lagging `useEffect`)
- Tracker chrome defaults to `HH:MM` (server display / poll cadence); optional live `HH:MM:SS` via `trackerShowSeconds` setting (same client interpolation pattern as main)
- Gap dialog: large duration, short labels, keyboard-first
- Timeline gap/resize `<button>`s: reset WebKit appearance (`appearance: none`, no native bezel) in [`main.css`](src/windows/main/main.css)
- DB failures: one clear error/recovery path — no popup spam

## Assets & branding

- Logos/icons under `resources/`, `public/`, and `src-tauri/icons/` are Git LFS-tracked ([`.gitattributes`](.gitattributes))
- Regenerate desktop/mobile icons with `pnpm tauri icon` from a square ~1024 source (e.g. `resources/timetrove-logo.png`)

## Conventions for changes

- Keep OS-specific chrome (tray, docking, positioning) in Rust; keep shared product UI in React
- Serde / TS types: camelCase; engine phase uses tagged `kind`
- Near-adjacent same-task closed slices may be coalesced (`coalesce_near_adjacent`); don’t assume every day read re-runs it
- Gap/switcher windows are special-cased for visibility restore; main close typically hides rather than destroys
- Do not invent multi-user or general cloud sync; Jira Cloud worklog sync is the only optional outbound integration

## Verify

```bash
pnpm install
pnpm tauri dev          # run the app
pnpm test               # vitest (src/**/*.test.ts)
pnpm test:rust          # cargo test --manifest-path src-tauri/Cargo.toml
pnpm test:all           # rust then vitest
```

For deeper product acceptance criteria and post-v1 ideas, read [`plan.md`](plan.md).
