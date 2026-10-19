# TimeTrove

Personal, single-user, local-only time tracking for a developer’s machine. Goal: accurate day-to-day tracking with minimal friction, then a clean weekly timesheet.

Inspired by [Grindstone](https://www.epiforge.com/grindstone/) (work items + time slices + idle correction). TimeTrove keeps that core loop and improves polish: colors, keyboard-first switching, a compact gap dialog, an always-visible daily total, a built-in timesheet, and a codebase we can extend (Jira, charts, etc. later).

---

## Goals

- Notice work/idle/startup gaps and attribute time with a fast, readable prompt
- Make the current task and “hours today” always visible
- Switch or create tasks with a global shortcut and almost no clicking
- Produce a Monday-start weekly timesheet (task × day) with totals
- Insights dashboard over selectable periods (task charts, schedule averages, trends)
- Store everything locally in SQLite; backup/export without a cloud dependency

### Non-goals (v1)

- External timesheet / ERP submission (beyond optional Jira Cloud worklogs and Copilot usage reads)
- Multi-user, sync, or multiple machines
- In-app custom property engine (add fields in code later if needed)
- Decimal-hour CSV / reporting formats
- Jira Server/DC, OAuth, concurrent multi-issue booking, or inbound Jira → TimeTrove sync

---

## Platforms

| Platform | Role | Chrome |
|---|---|---|
| **Windows** | Product target | True taskbar embedding is not available on Windows 11. v1 uses a **docked always-on-top bar** pinned above the taskbar near the tray (color dot, task name, daily `HH:MM`) plus a **system tray** icon/menu as fallback when the bar is hidden. Not-tracking state must be obvious (no task name). |
| **macOS** | Dev / test harness | Same app logic; **floating always-on-top** compact bar (draggable; can hide to menu bar / tray). No taskbar embedding. |

Main window (both OS): tasks, timesheet, insights dashboard, manual time editing, settings.

Auto-start at login: on for Windows; Mac login item for convenience while developing.

---

## Stack

- **Tauri 2** + **React** + **TypeScript** + **Vite**
- **SQLite** for all app data
- Windows taskbar embedding behind a **native/OS-specific capability**; Mac builds never pretend to have it
- Single UI codebase; shell differences limited to the always-visible tracker chrome

---

## Domain model

### Task

Flat list (no projects).

| Field | Notes |
|---|---|
| name | Required; unique enough for search (enforce uniqueness or warn — prefer unique names) |
| color | Auto-assigned unique default; user-editable |
| description | Static “what is this task” |
| notes | Ongoing scratchpad while working |
| archived | Soft-hide from timing switcher; **unarchive** supported |
| jira_issue_key | Optional active Jira Cloud issue key (e.g. `THING-123`) when worklog sync is enabled |
| jira_reported_ms | Watermark of ms successfully posted for the current issue key |
| jira_link_baseline_ms | Task total at link time; TimeTrove posted = reported − baseline |
| previous tickets | History of prior keys (id, title, TimeTrove booked ms) — display only; only the active key receives new worklogs |

Archived tasks:

- Do **not** appear as options when starting/switching timing
- **Do** appear on timesheets and history if they have booked time

### Time slice

A contiguous stretch on one task: `start`, `end` (null while running), `task_id`.

Rules:

- At most **one** running slice at a time
- Start / stop are explicit; stop ends the slice with no attribution prompt
- Switching tasks closes the current slice and opens a new one
- At **local midnight**, auto-split: end slice at 24:00, continue same task with a new slice (keeps “hours today” and timesheets correct)

### Discard log

When the user discards a gap:

- Do **not** attach discarded time to a task
- Append to an aggregate discard log (duration + timestamp range) for audit only
- Excluded from daily totals and timesheets

### Optional Jira Cloud worklog sync

Disabled by default. Settings: site URL, email, API token. When enabled, tasks may link one active issue key (validated via Add against Jira); TimeTrove posts unreported time as Jira worklogs hourly and on stop/switch. Relinking archives the prior key into previous tickets and baselines so time is not double-posted. Does not delete/edit remote worklogs if local history is shortened.

### Optional GitHub Copilot usage tracking

Disabled by default. Settings: GitHub host (`github.com` or Enterprise Cloud host), optional username, API token, poll interval (default 30 minutes), optional monthly limit override. Polls the same per-user quota API VS Code uses (`GET /copilot_internal/user`, with `/copilot_internal/v2/token` fallback). When enabled: feature-gated Copilot main-window tab (KPIs, pools, history charts) and a usage-% bubble on the tracker bar (GitHub icon + tooltip). Limit prefers API entitlement; override applies only when the API has no usable entitlement.

---

## Core behaviors

### Always-visible tracker

Shows:

- Color dot + current task name when tracking
- Clear **not tracking** state when stopped
- Daily total as `HH:MM` (e.g. `05:23`)
- Daily total color: under goal vs at/over goal (default goal **8 hours**, configurable)

Mouse on the task name / “Not tracking” area:

- Single-click → open the task switcher
- Double-click → open the main window
- Click-and-drag → move the bar

The daily-total (clock) control also opens the switcher.

### Task switcher (global shortcut)

1. Shortcut, tracker task-name click, or clock click opens a compact search palette (filtered list below)
2. Typing filters by task name (active/non-archived only)
3. Enter on a match → switch now (close current slice, start chosen task)
4. Enter on no match → **confirmation** (“Create task …?”), then create with auto color + start
5. Clicking a filtered match also switches

### Gap attribution (idle / sleep / startup)

One shared flow for:

- Idle past threshold (default **10 minutes**)
- Wake from sleep
- App or OS start when time passed since last activity

**Timing rules:**

- Time up to the **start of idle** (last user activity) stays on the then-current task
- From last activity until the user answers = **unattributed gap**
- Prompt appears after the idle threshold; the gap may be much longer (e.g. overnight)
- Do **not** assign gap time until the user chooses
- User **must** choose; Escape does not dismiss

**Prompt actions (short labels, keyboard-first):**

| Key | Action | Afterward |
|---|---|---|
| `1` (default focus) | Assign gap to **previous** task | Attribute + resume previous |
| `2` | Assign gap to **another** task (search) | Attribute + start chosen |
| `3` | **Discard** gap | Log discard tally + **resume previous from now** |

**Duration display:** large, color-banded by length so overnight is obvious:

| Band | Example threshold | Intent |
|---|---|---|
| Short | &lt; 20 minutes | Neutral / calm |
| Medium | 20 minutes – 2 hours | Attention |
| Long | &gt; 2 hours | Strong “probably discard” |

Thresholds are v1 defaults; keep them in one place for later tuning.

### Manual time surgery (v1)

From the main UI:

- Add a time entry
- Edit start / end
- Split an entry
- Delete an entry
- Move an entry to another task

Validation: no overlapping running nonsense; edits should not silently corrupt day totals (reject or warn on invalid ranges).

### Timesheet (v1)

- Week starts **Monday**
- Rows: tasks with any time that week (including archived)
- Columns: days Mon–Sun
- Cells: `HH:MM`; row and column sums
- Discarded gaps: invisible
- Export: **CSV** + **copy table** (same `HH:MM` format; no decimal hours in v1)

### Settings (v1)

| Setting | Default |
|---|---|
| Idle threshold before prompt | 10 minutes |
| Daily goal | 8 hours |
| Task-switch shortcut | Sensible OS default (user-rebindable) |
| Start at login | On |

Week start fixed to Monday (not configurable in v1). No sound on prompt in v1.

### Data

- SQLite in the OS app-data directory
- Settings: **Reveal database**, **Export backup**, **Import backup**

---

## UX / visual requirements

- Modern, attractive, colorful — color carries meaning (task identity, gap length, goal progress)
- Task color auto-picked to stay distinct from existing tasks; user can override
- Gap dialog: scannable in under a second — large duration, short action labels, no wordy paragraphs
- Prefer keyboard paths for switcher and gap prompt; mouse still fully supported
- Not-tracking state must be unmistakable in the taskbar / floating bar

---

## Why not just Grindstone?

Grindstone already covers most of the core. Gaps that justify a custom tool:

- Weak use of color for status / identity
- Too many clicks for common actions (including timesheet-style views)
- Idle/away dialog too wordy to parse quickly
- No global keyboard task switch
- No clear daily goal / hours-today signal in the always-visible chrome
- Poor behavior when the DB connection fails (error popup spam) — TimeTrove should surface **one** clear error/recovery path
- Cannot iterate on annoyances or add features freely

---

## Post-v1 (ordered roughly)

1. **Richer Insights** — streaks, weekend vs weekday, switches/day, stddev
2. **Richer Jira** — concurrent multi-issue links, issue picker, Server/DC
3. Richer export / external timesheet systems
4. Optional custom path for the DB (Dropbox/iCloud-friendly)
5. Hard-coded extra task fields if a real need appears (still not a user-facing property builder)

---

## Acceptance checklist (v1 “done”)

- [ ] Track time on a flat task list with start/stop and one active timer
- [ ] Global shortcut switches tasks; create-with-confirm works
- [ ] Idle / sleep / startup gaps use one attribution dialog (`1` / `2` / `3`)
- [ ] Gap duration color bands; long gaps visually scream “discard?”
- [ ] Discard logs aggregate only; totals/timesheet ignore discards; tracking resumes previous
- [ ] Daily `HH:MM` in always-visible chrome; color flips at goal
- [ ] Midnight auto-split keeps day boundaries correct
- [ ] Archive/unarchive; archived hidden from switcher, visible on timesheet when relevant
- [ ] Full manual time edit (add / edit / split / delete / move)
- [ ] Monday-start timesheet with sums; CSV + copy
- [ ] Insights tab with period filter, KPIs, and task-colored charts
- [ ] SQLite in app data; reveal + export/import backup
- [ ] Settings: idle threshold, daily goal, shortcut, start at login
- [ ] Mac: floating bar usable for daily testing
- [ ] Windows: docked tracker bar above the taskbar + tray fallback with same information density
- [ ] DB failure shows a single clear error, not popup spam
