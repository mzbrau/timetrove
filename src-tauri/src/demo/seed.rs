//! Deterministic 12-month demo database for documentation screenshots.

use crate::db::{open_db, Db};
use crate::domain::*;
use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, TimeZone, Timelike, Utc, Weekday};
use rusqlite::params;
use std::collections::HashMap;
use std::path::Path;
use uuid::Uuid;

/// Minutes booked today before the screenshot run starts (shows ~5:12 on the tracker).
const TODAY_BOOKED_MINS: i64 = 312;
const FEATURED_TASK: &str = "Data export service";
const JIRA_SITE: &str = "https://acme.atlassian.net";
const ASSIGNEE: &str = "Alex Morgan";
const GITHUB_USER: &str = "alexmorgan";
const COPILOT_ALLOWANCE: f64 = 300.0;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
    fn range(&mut self, lo: i64, hi: i64) -> i64 {
        lo + (self.next() % ((hi - lo + 1) as u64)) as i64
    }
    fn chance(&mut self, p: f64) -> bool {
        self.unit() < p
    }
    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[(self.next() % items.len() as u64) as usize]
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Kind {
    Standup,
    Planning,
    Review,
    Meeting,
    Support,
    Learning,
    Admin,
    Upkeep,
    Project,
}

struct Spec {
    name: &'static str,
    kind: Kind,
    description: &'static str,
    notes: &'static str,
    jira: Option<(&'static str, &'static str, i64)>,
    /// Weeks ago the task becomes active / stops (0 = still active).
    from_week: i64,
    to_week: i64,
    starred: bool,
    slice_notes: &'static [&'static str],
}

const fn recurring(
    name: &'static str,
    kind: Kind,
    description: &'static str,
    starred: bool,
    slice_notes: &'static [&'static str],
) -> Spec {
    Spec {
        name,
        kind,
        description,
        notes: "",
        jira: None,
        from_week: 60,
        to_week: 0,
        starred,
        slice_notes,
    }
}

const fn project(
    name: &'static str,
    key: &'static str,
    summary: &'static str,
    estimate_h: i64,
    from_week: i64,
    to_week: i64,
    description: &'static str,
    slice_notes: &'static [&'static str],
) -> Spec {
    Spec {
        name,
        kind: Kind::Project,
        description,
        notes: "",
        jira: Some((key, summary, estimate_h)),
        from_week,
        to_week,
        starred: false,
        slice_notes,
    }
}

fn specs() -> Vec<Spec> {
    let mut v = vec![
        recurring("Standup", Kind::Standup, "Daily team standup (15 min).", false, &[]),
        recurring(
            "Sprint planning",
            Kind::Planning,
            "Fortnightly planning, refinement and retro.",
            false,
            &["Retro + planning", "Backlog refinement"],
        ),
        recurring(
            "Code review",
            Kind::Review,
            "Reviewing teammates' pull requests.",
            true,
            &["Reviewed payments PRs", "Pairing on review feedback", "Big refactor PR"],
        ),
        recurring(
            "Meetings",
            Kind::Meeting,
            "1:1s, design reviews and cross-team syncs.",
            false,
            &["1:1 with Sam", "Architecture review", "Design sync", "Cross-team sync"],
        ),
        recurring(
            "Support / on-call",
            Kind::Support,
            "Production support, alerts and customer escalations.",
            false,
            &["Paged: elevated 5xx", "Customer escalation", "Triage support queue"],
        ),
        recurring(
            "Learning",
            Kind::Learning,
            "Friday learning time: talks, courses, reading.",
            false,
            &["Rust async course", "Conference talk", "Reading: DDIA ch. 7"],
        ),
        recurring("Email & admin", Kind::Admin, "Inbox, Slack catch-up, expenses.", false, &[]),
        recurring(
            "Dependency upgrades",
            Kind::Upkeep,
            "Keeping libraries and toolchains current.",
            false,
            &["Bumped React + Vite", "Renovate PR sweep"],
        ),
    ];
    v.extend([
        project("Payments API v2", "TT-101", "Payments API v2: idempotent charge endpoints", 120, 52, 40,
            "New versioned payments API with idempotency keys.",
            &["Idempotency key middleware", "Contract tests", "Rollout plan"]),
        project("Checkout redesign", "TT-118", "Checkout redesign: single-page flow", 160, 50, 36,
            "Single-page checkout with address autocomplete.",
            &["Address autocomplete", "A/B test wiring", "Fix Safari layout"]),
        project("Search indexing pipeline", "TT-131", "Search indexing pipeline on Kafka", 140, 44, 30,
            "Move product search indexing to an event-driven pipeline.",
            &["Kafka consumer backpressure", "Reindex job", "Benchmarks"]),
        project("SSO with Okta", "TT-145", "Enterprise SSO via Okta (SAML + SCIM)", 90, 38, 28,
            "SAML login and SCIM provisioning for enterprise customers.",
            &["SAML assertion parsing", "SCIM user sync"]),
        project("Billing data migration", "TT-152", "Migrate billing records to Postgres 16", 110, 34, 22,
            "Online migration of billing tables with dual writes.",
            &["Dual-write shim", "Backfill batch 3/5", "Verification queries"]),
        project("Observability dashboards", "TT-160", "Service dashboards + SLO alerts", 70, 30, 18,
            "Grafana dashboards and SLO burn-rate alerts per service.",
            &["SLO burn-rate alerts", "Trace sampling"]),
        project("Incident: cache stampede", "TT-188", "Postmortem: cache stampede on product pages", 16, 18, 17,
            "Fix and postmortem for the product page cache stampede.",
            &["Request coalescing fix", "Postmortem write-up"]),
        project("Mobile push notifications", "TT-172", "Push notifications for order updates", 100, 26, 14,
            "APNs / FCM push for order status updates.",
            &["APNs token refresh", "Notification preferences UI"]),
        project("Rate limiter service", "TT-181", "Shared rate limiter service (Redis)", 80, 20, 10,
            "Token-bucket rate limiting shared across gateways.",
            &["Token bucket in Lua", "Load test"]),
        project("Feature flag rollout", "TT-190", "Adopt feature flags across web + API", 60, 16, 6,
            "Migrate ad-hoc toggles to the feature flag service.",
            &["Flag SDK wrapper", "Cleanup stale flags"]),
        project("Accessibility audit", "TT-197", "WCAG 2.2 AA audit fixes", 50, 12, 3,
            "Fix findings from the external accessibility audit.",
            &["Focus order fixes", "Color contrast tokens"]),
        project("Onboarding flow revamp", "TT-211", "Onboarding flow revamp: guided setup", 45, 5, 0,
            "Guided first-run setup with progress checklist.",
            &["Checklist component", "Copy review with design"]),
        project("Performance budget", "TT-215", "Web performance budget + CI checks", 30, 3, 0,
            "Lighthouse budgets enforced in CI.",
            &["Lighthouse CI", "Bundle size report"]),
        project(FEATURED_TASK, "TT-204", "Customer data export service (CSV + Parquet)", 140, 8, 0,
            "Self-serve exports of orders and invoices, CSV and Parquet.",
            &["Streaming CSV writer", "Parquet schema", "S3 presigned downloads", "Export job retries"]),
    ]);
    for s in v.iter_mut() {
        match s.name {
            FEATURED_TASK => {
                s.starred = true;
                s.notes = "Next: retry policy for failed exports.\nAsk platform team about S3 lifecycle rules.\nParquet: confirm decimal precision with finance.";
            }
            "Onboarding flow revamp" => {
                s.starred = true;
                s.notes = "Design handoff in Figma (v3).\nTrack activation metric in Amplitude.";
            }
            "Support / on-call" => {
                s.notes = "Runbooks: wiki/oncall\nEscalation: #inc-response";
            }
            _ => {}
        }
    }
    v
}

struct Block {
    task: usize,
    start: i64,
    end: i64,
    note: String,
}

struct Ctx<'a> {
    rng: &'a mut Rng,
    specs: &'a [Spec],
    by_kind: HashMap<Kind, usize>,
}

impl Ctx<'_> {
    fn kind(&self, k: Kind) -> usize {
        self.by_kind[&k]
    }

    fn active_projects(&self, weeks_ago: i64) -> Vec<usize> {
        self.specs
            .iter()
            .enumerate()
            .filter(|(_, s)| s.kind == Kind::Project && s.from_week >= weeks_ago && s.to_week <= weeks_ago)
            .map(|(i, _)| i)
            .collect()
    }

    fn note_for(&mut self, task: usize) -> String {
        let notes = self.specs[task].slice_notes;
        if notes.is_empty() || !self.rng.chance(0.22) {
            return String::new();
        }
        self.rng.pick(notes).to_string()
    }

    /// Pick the next task for a block starting at `minute`; never repeats `prev`.
    fn next_task(&mut self, minute: i64, weekday: Weekday, projects: &[usize], prev: Option<usize>) -> (usize, i64) {
        loop {
            let afternoon = minute >= 13 * 60;
            let meeting_day = matches!(weekday, Weekday::Tue | Weekday::Thu);
            let r = self.rng.unit();
            let (task, dur) = if weekday == Weekday::Fri && afternoon && r < 0.22 {
                (self.kind(Kind::Learning), self.rng.range(45, 90))
            } else if r < (if meeting_day { 0.24 } else { 0.12 }) {
                (self.kind(Kind::Meeting), *self.rng.pick(&[30, 30, 45, 60]))
            } else if r < (if afternoon { 0.42 } else { 0.32 }) {
                (self.kind(Kind::Review), self.rng.range(20, 50))
            } else if r < (if afternoon { 0.52 } else { 0.40 }) {
                (self.kind(Kind::Support), self.rng.range(20, 60))
            } else if r < 0.44 {
                (self.kind(Kind::Upkeep), self.rng.range(30, 60))
            } else if projects.is_empty() {
                (self.kind(Kind::Review), self.rng.range(30, 60))
            } else {
                // Newest active project gets most focus time.
                let p = if self.rng.chance(0.35) {
                    projects[projects.len() - 1]
                } else {
                    *self.rng.pick(projects)
                };
                (p, self.rng.range(45, 130))
            };
            if Some(task) != prev {
                return (task, dur);
            }
        }
    }

    fn push(&mut self, blocks: &mut Vec<Block>, task: usize, start: i64, end: i64, booked: &mut i64) {
        if end <= start {
            return;
        }
        *booked += end - start;
        if let Some(last) = blocks.last_mut() {
            if last.task == task && last.end == start {
                last.end = end;
                return;
            }
        }
        let note = self.note_for(task);
        blocks.push(Block { task, start, end, note });
    }

    /// Generate one working day as minute offsets from local midnight.
    /// Returns blocks and optional discarded gap (start, end).
    fn work_day(&mut self, weekday: Weekday, weeks_ago: i64, target: i64) -> (Vec<Block>, Option<(i64, i64)>) {
        let projects = self.active_projects(weeks_ago);
        let mut blocks: Vec<Block> = Vec::new();
        let mut cursor = 8 * 60 + 15 + self.rng.range(0, 60);
        let lunch_at = 12 * 60 + self.rng.range(-15, 30);
        let lunch_len = self.rng.range(30, 45);
        let mut lunch_done = false;
        let mut booked = 0i64;
        let mut discard: Option<(i64, i64)> = None;
        let standup_at = 9 * 60 + 30;

        // Morning warm-up until standup.
        let first = if self.rng.chance(0.7) { self.kind(Kind::Admin) } else { self.kind(Kind::Review) };
        let warm = (cursor + self.rng.range(10, 25)).min(standup_at);
        self.push(&mut blocks, first, cursor, warm, &mut booked);
        cursor = warm;
        if cursor < standup_at {
            let (task, _) = self.next_task(cursor, weekday, &projects, Some(first));
            self.push(&mut blocks, task, cursor, standup_at, &mut booked);
            cursor = standup_at;
        }
        let standup = self.kind(Kind::Standup);
        self.push(&mut blocks, standup, cursor, cursor + 15, &mut booked);
        cursor += 15;
        if weekday == Weekday::Mon && weeks_ago % 2 == 0 {
            let len = *self.rng.pick(&[60, 75, 90]);
            let planning = self.kind(Kind::Planning);
            self.push(&mut blocks, planning, cursor, cursor + len, &mut booked);
            cursor += len;
        }

        let wants_discard = self.rng.chance(0.07);
        while booked < target {
            let prev = blocks.last().map(|b| b.task);
            let (task, mut dur) = self.next_task(cursor, weekday, &projects, prev);
            dur = dur.min(target - booked);
            let mut end = cursor + dur;
            if !lunch_done && end > lunch_at {
                end = lunch_at.max(cursor);
                self.push(&mut blocks, task, cursor, end, &mut booked);
                cursor = end + lunch_len;
                lunch_done = true;
                continue;
            }
            self.push(&mut blocks, task, cursor, end, &mut booked);
            cursor = end;
            if lunch_done && wants_discard && discard.is_none() && cursor > 14 * 60 {
                let len = self.rng.range(20, 40);
                discard = Some((cursor, cursor + len));
                cursor += len;
            } else if self.rng.chance(0.1) {
                cursor += self.rng.range(4, 12);
            }
        }
        (blocks, discard)
    }
}

fn local_at(day: NaiveDate, minute: i64) -> DateTime<Utc> {
    let naive = day.and_hms_opt(0, 0, 0).unwrap() + Duration::minutes(minute);
    Local
        .from_local_datetime(&naive)
        .earliest()
        .unwrap_or_else(|| Local.from_utc_datetime(&naive))
        .with_timezone(&Utc)
}

fn is_holiday(day: NaiveDate) -> bool {
    matches!((day.month(), day.day()), (1, 1) | (5, 1) | (12, 25) | (12, 26))
        || (day.month() == 12 && day.day() >= 27)
        || (day.month() == 1 && day.day() == 2)
}

/// Seed a fresh demo database at `path` (replaced if it exists).
/// Also writes `jira-assigned.json` next to it.
pub fn seed_demo_db(path: &Path) -> Result<(), String> {
    for suffix in ["", "-wal", "-shm"] {
        let p = path.with_file_name(format!(
            "{}{}",
            path.file_name().and_then(|n| n.to_str()).unwrap_or("timetrove-demo.db"),
            suffix
        ));
        let _ = std::fs::remove_file(p);
    }
    let conn = open_db(path)?;
    let db = Db::new(conn, path.to_path_buf());
    let specs = specs();
    let mut rng = Rng(0x7157_7a0e_2026_0001);

    // Tasks
    let mut task_ids = Vec::new();
    for spec in &specs {
        let task = db.create_task(CreateTaskInput {
            name: spec.name.to_string(),
            color: (spec.name == FEATURED_TASK).then(|| "#8b5cf6".to_string()),
            description: Some(spec.description.to_string()),
            notes: Some(spec.notes.to_string()),
            jira_issue_key: None,
        })?;
        task_ids.push(task.id);
    }
    let by_kind: HashMap<Kind, usize> = specs
        .iter()
        .enumerate()
        .filter(|(_, s)| s.kind != Kind::Project)
        .map(|(i, s)| (s.kind, i))
        .collect();
    let featured = specs.iter().position(|s| s.name == FEATURED_TASK).unwrap();

    let now_local = Local::now();
    let today = now_local.date_naive();
    let first_day = today - Duration::days(365);
    let this_monday = today - Duration::days(today.weekday().num_days_from_monday() as i64);
    let vacation_weeks = [11i64, 32, 33];
    let sick_days = [this_monday - Duration::days(7 * 7 - 2), this_monday - Duration::days(24 * 7 - 3)];
    let weekend_blips = [5i64, 19, 29, 41];

    let mut slices: Vec<(usize, DateTime<Utc>, Option<DateTime<Utc>>, String)> = Vec::new();
    let mut discards: Vec<(DateTime<Utc>, DateTime<Utc>)> = Vec::new();
    {
        let mut ctx = Ctx { rng: &mut rng, specs: &specs, by_kind };
        let mut day = first_day;
        while day < today {
            let monday = day - Duration::days(day.weekday().num_days_from_monday() as i64);
            let weeks_ago = (this_monday - monday).num_days() / 7;
            let weekday = day.weekday();
            let weekend = matches!(weekday, Weekday::Sat | Weekday::Sun);
            if weekend {
                if weekday == Weekday::Sat && weekend_blips.contains(&weeks_ago) {
                    let start = 10 * 60 + ctx.rng.range(0, 90);
                    let len = ctx.rng.range(50, 110);
                    let task = ctx.kind(Kind::Support);
                    slices.push((task, local_at(day, start), Some(local_at(day, start + len)), "Weekend incident".into()));
                }
            } else if !(is_holiday(day) || vacation_weeks.contains(&weeks_ago) || sick_days.contains(&day)) {
                let target = if weekday == Weekday::Fri && ctx.rng.chance(0.35) {
                    ctx.rng.range(400, 450)
                } else {
                    ctx.rng.range(465, 505)
                };
                let (blocks, discard) = ctx.work_day(weekday, weeks_ago, target);
                for b in blocks {
                    slices.push((b.task, local_at(day, b.start), Some(local_at(day, b.end)), b.note));
                }
                if let Some((s, e)) = discard {
                    discards.push((local_at(day, s), local_at(day, e)));
                }
            }
            day += Duration::days(1);
        }

        // Today: a shortened day shifted so it ends right now on the featured task.
        let (mut blocks, _) = ctx.work_day(Weekday::Wed, 0, TODAY_BOOKED_MINS - 52);
        if blocks.last().map(|b| b.task) == Some(featured) {
            blocks.last_mut().unwrap().task = ctx.kind(Kind::Review);
        }
        let last_end = blocks.last().map(|b| b.end).unwrap_or(9 * 60);
        let now_min = now_local.hour() as i64 * 60 + now_local.minute() as i64;
        let running_start = now_min - 52;
        let shift = running_start - last_end;
        for b in blocks {
            let (s, e) = (b.start + shift, b.end + shift);
            if e <= 0 {
                continue;
            }
            slices.push((b.task, local_at(today, s.max(1)), Some(local_at(today, e)), b.note));
        }
        slices.push((
            featured,
            local_at(today, running_start.max(2)),
            None,
            "Export job retries".into(),
        ));
    }

    // Booked totals (closed slices) for Jira watermarks.
    let mut booked_ms: HashMap<usize, i64> = HashMap::new();
    let mut last_use: HashMap<usize, DateTime<Utc>> = HashMap::new();
    for (task, start, end, _) in &slices {
        if let Some(end) = end {
            *booked_ms.entry(*task).or_default() += (*end - *start).num_milliseconds();
        }
        last_use.insert(*task, *start);
    }

    db.with_conn(|conn| {
        conn.execute_batch("BEGIN").map_err(|e| e.to_string())?;
        for (task, start, end, note) in &slices {
            conn.execute(
                "INSERT INTO time_slices (id, task_id, start, end, note) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    Uuid::new_v4().to_string(),
                    task_ids[*task],
                    start.to_rfc3339(),
                    end.map(|e| e.to_rfc3339()),
                    note
                ],
            )
            .map_err(|e| e.to_string())?;
        }
        for (start, end) in &discards {
            conn.execute(
                "INSERT INTO discard_log (id, start, end, duration_ms, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    Uuid::new_v4().to_string(),
                    start.to_rfc3339(),
                    end.to_rfc3339(),
                    (*end - *start).num_milliseconds(),
                    end.to_rfc3339()
                ],
            )
            .map_err(|e| e.to_string())?;
        }

        let now = Utc::now();
        for (i, spec) in specs.iter().enumerate() {
            let created = local_at(today - Duration::weeks(spec.from_week.min(52)), 9 * 60);
            let archived = spec.kind == Kind::Project && spec.to_week > 0;
            conn.execute(
                "UPDATE tasks SET archived = ?2, starred = ?3, created_at = ?4, updated_at = ?5 WHERE id = ?1",
                params![
                    task_ids[i],
                    archived as i64,
                    spec.starred as i64,
                    created.to_rfc3339(),
                    last_use.get(&i).copied().unwrap_or(created).to_rfc3339()
                ],
            )
            .map_err(|e| e.to_string())?;
            let Some((key, summary, estimate_h)) = spec.jira else { continue };
            let booked = booked_ms.get(&i).copied().unwrap_or(0);
            let reported = booked / 60_000 * 60_000;
            let baseline = match spec.name {
                FEATURED_TASK => 6 * 3_600_000 + 25 * 60_000,
                "Onboarding flow revamp" => 2 * 3_600_000 + 10 * 60_000,
                _ => 0,
            };
            let status = if archived {
                "Done"
            } else if spec.name == "Performance budget" {
                "In Review"
            } else {
                "In Progress"
            };
            let spent_s = (reported - baseline) / 1000 * 115 / 100;
            conn.execute(
                "UPDATE tasks SET jira_issue_key = ?2, jira_reported_ms = ?3, jira_link_baseline_ms = ?4,
                        jira_issue_summary = ?5, jira_issue_status = ?6, jira_issue_assignee = ?7,
                        jira_issue_estimate_s = ?8, jira_issue_total_spent_s = ?9, jira_issue_fetched_at = ?10
                 WHERE id = ?1",
                params![
                    task_ids[i],
                    key,
                    reported,
                    baseline,
                    summary,
                    status,
                    ASSIGNEE,
                    estimate_h * 3600,
                    spent_s,
                    now.to_rfc3339()
                ],
            )
            .map_err(|e| e.to_string())?;
            let history: &[(&str, &str, i64, i64)] = match spec.name {
                FEATURED_TASK => &[("TT-198", "Spike: export formats and limits", 6 * 3_600_000 + 25 * 60_000, 6)],
                "Onboarding flow revamp" => &[("TT-206", "Onboarding UX research", 2 * 3_600_000 + 10 * 60_000, 3)],
                _ => &[],
            };
            for (hkey, title, ms, weeks) in history {
                conn.execute(
                    "INSERT INTO task_jira_history (id, task_id, issue_key, title, booked_ms, unlinked_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        Uuid::new_v4().to_string(),
                        task_ids[i],
                        hkey,
                        title,
                        ms,
                        (now - Duration::weeks(*weeks)).to_rfc3339()
                    ],
                )
                .map_err(|e| e.to_string())?;
            }
        }
        conn.execute_batch("COMMIT").map_err(|e| e.to_string())?;
        Ok(())
    })?;

    // Settings: integrations on with placeholder credentials (demo mode never calls out).
    let mut settings = db.get_settings()?;
    settings.autostart = false;
    settings.jira_enabled = true;
    settings.jira_site_url = JIRA_SITE.into();
    settings.jira_email = "alex.morgan@acme.dev".into();
    settings.jira_api_token = "demo-jira-token".into();
    settings.copilot_enabled = true;
    settings.copilot_github_host = "github.com".into();
    settings.copilot_username = GITHUB_USER.into();
    settings.copilot_api_token = "demo-github-token".into();
    settings.github_activity_enabled = true;
    db.update_settings(&settings)?;
    let now = Utc::now();
    db.set_jira_last_synced_at(Some(now - Duration::minutes(23)))?;
    db.set_copilot_last_synced_at(Some(now - Duration::minutes(11)))?;
    db.set_github_activity_last_synced_at(Some(now - Duration::minutes(7)))?;
    db.set_last_good_at(Some(now))?;

    seed_copilot(&db, &mut rng, now)?;
    seed_github(&db, now)?;
    write_jira_fixture(path, &specs, &booked_ms)?;
    Ok(())
}

fn seed_copilot(db: &Db, rng: &mut Rng, now: DateTime<Utc>) -> Result<(), String> {
    let mut t = now - Duration::days(365);
    let mut month = (t.year(), t.month());
    let mut used = 0f64;
    let mut month_rate = rng.range(8, 17) as f64;
    let mut last = 0f64;
    while t <= now {
        if (t.year(), t.month()) != month {
            month = (t.year(), t.month());
            used = 0.0;
            month_rate = rng.range(8, 17) as f64;
        }
        let local = t.with_timezone(&Local);
        let workday = !matches!(local.weekday(), Weekday::Sat | Weekday::Sun);
        if workday && (9..18).contains(&local.hour()) {
            used += month_rate / 3.0 * (0.5 + rng.unit());
        }
        used = used.min(COPILOT_ALLOWANCE);
        let point = CopilotUsageHistoryPoint {
            id: Uuid::new_v4().to_string(),
            fetched_at: t,
            plan: "business".into(),
            entitlement: COPILOT_ALLOWANCE,
            used: used.round(),
            remaining: (COPILOT_ALLOWANCE - used).max(0.0).round(),
            percent_used: (used / COPILOT_ALLOWANCE * 100.0 * 10.0).round() / 10.0,
            unit: "requests".into(),
        };
        db.insert_copilot_usage_snapshot(&point, "{}")?;
        last = used.round();
        t += Duration::hours(3);
    }
    let next_month = if now.month() == 12 {
        NaiveDate::from_ymd_opt(now.year() + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(now.year(), now.month() + 1, 1)
    }
    .unwrap();
    let pool = |id: &str, entitlement: f64, used: f64, unlimited: bool| CopilotQuotaPool {
        id: id.into(),
        entitlement,
        remaining: if unlimited { 0.0 } else { (entitlement - used).max(0.0) },
        used,
        percent_remaining: if unlimited { 100.0 } else { 100.0 - used / entitlement * 100.0 },
        percent_used: if unlimited { 0.0 } else { used / entitlement * 100.0 },
        unlimited,
        overage_count: 0.0,
        overage_permitted: false,
    };
    let summary = CopilotUsageSummary {
        plan: "business".into(),
        username: GITHUB_USER.into(),
        unit: "requests".into(),
        entitlement: COPILOT_ALLOWANCE,
        used: last,
        remaining: (COPILOT_ALLOWANCE - last).max(0.0),
        percent_used: last / COPILOT_ALLOWANCE * 100.0,
        unlimited: false,
        overage_count: 0.0,
        overage_permitted: false,
        reset_date: Some(format!("{}T00:00:00.000Z", next_month)),
        pools: vec![
            pool("premium_interactions", COPILOT_ALLOWANCE, last, false),
            pool("chat", 0.0, 0.0, true),
            pool("completions", 0.0, 0.0, true),
        ],
        limit_source: "api".into(),
        fetched_at: Some((now - Duration::minutes(11)).to_rfc3339()),
        last_synced_at: None,
        last_error: None,
    };
    db.set_copilot_usage_cache(&serde_json::to_string(&summary).map_err(|e| e.to_string())?)
}

fn seed_github(db: &Db, now: DateTime<Utc>) -> Result<(), String> {
    let pr = |number: i32, title: &str, repo: &str, days: i64, check: &str, decision: &str, comments: u32, reviews: &str, needs: bool| {
        GithubPrSummary {
            number,
            title: title.into(),
            url: format!("https://github.com/{repo}/pull/{number}"),
            repo_full_name: repo.into(),
            created_at: (now - Duration::hours(days * 24 + 3)).to_rfc3339(),
            check_status: check.into(),
            review_decision: decision.into(),
            unresolved_comment_count: comments,
            reviews_summary: reviews.into(),
            needs_my_review: needs,
            requested_reviewer_logins: vec![],
            reviews_fingerprint: String::new(),
        }
    };
    let authored = vec![
        pr(1482, "Data export: stream CSV rows instead of buffering", "acme/exports", 1, "SUCCESS", "APPROVED", 0, "2 approvals", false),
        pr(1479, "Data export: Parquet writer behind feature flag", "acme/exports", 3, "PENDING", "REVIEW_REQUIRED", 2, "1 comment", false),
        pr(912, "Onboarding: guided setup checklist component", "acme/web", 2, "FAILURE", "CHANGES_REQUESTED", 4, "Changes requested by priya", false),
        pr(388, "CI: enforce Lighthouse performance budget", "acme/platform", 5, "SUCCESS", "REVIEW_REQUIRED", 0, "Awaiting review", false),
    ];
    let reviews = vec![
        pr(2210, "Payments: retry webhooks with exponential backoff", "acme/payments-api", 0, "SUCCESS", "REVIEW_REQUIRED", 0, "Review requested", true),
        pr(915, "Checkout: lazy-load address autocomplete", "acme/web", 1, "SUCCESS", "REVIEW_REQUIRED", 1, "1 approval", true),
        pr(1475, "Exports: S3 lifecycle rules for expired files", "acme/exports", 2, "PENDING", "REVIEW_REQUIRED", 0, "Review requested", true),
    ];
    let messages = [
        ("acme/exports", "Stream CSV rows through a bounded channel"),
        ("acme/exports", "Add retry policy for failed export jobs"),
        ("acme/exports", "Presign S3 download URLs with 15 min expiry"),
        ("acme/web", "Checklist: persist progress per workspace"),
        ("acme/exports", "Parquet: map decimals to fixed precision"),
        ("acme/web", "Fix focus trap in onboarding modal"),
        ("acme/platform", "Lighthouse CI budget for /checkout"),
        ("acme/exports", "Benchmark export of 1M orders"),
        ("acme/web", "Copy tweaks from design review"),
        ("acme/exports", "Handle cancelled exports gracefully"),
        ("acme/platform", "Bump Node to 22 in CI images"),
        ("acme/exports", "Add metrics for export duration"),
        ("acme/web", "Onboarding: skip step for invited users"),
        ("acme/exports", "Refactor job runner into crate"),
        ("acme/payments-api", "Review fix: clarify idempotency docs"),
        ("acme/exports", "Validate column selection input"),
        ("acme/web", "Storybook stories for checklist"),
        ("acme/exports", "Integration test against MinIO"),
        ("acme/platform", "Renovate: group minor updates"),
        ("acme/exports", "Initial export service skeleton"),
    ];
    let commits = messages
        .iter()
        .enumerate()
        .map(|(i, (repo, msg))| {
            let mut r = Rng(0x9e37_79b9_7f4a_7c15 ^ (i as u64 + 1));
            let sha = format!("{:016x}{:016x}{:08x}", r.next(), r.next(), r.next() as u32);
            GithubCommitSummary {
                url: format!("https://github.com/{repo}/commit/{sha}"),
                sha,
                message: msg.to_string(),
                repo_full_name: repo.to_string(),
                committed_at: (now - Duration::hours(i as i64 * 7 + 1)).to_rfc3339(),
            }
        })
        .collect();
    let digest = GithubActivityDigest {
        authored_prs: authored,
        review_prs: reviews,
        recent_commits: commits,
        username: GITHUB_USER.into(),
        fetched_at: Some((now - Duration::minutes(7)).to_rfc3339()),
        last_synced_at: None,
        last_error: None,
    };
    db.set_github_activity_cache(&serde_json::to_string(&digest).map_err(|e| e.to_string())?)
}

fn write_jira_fixture(db_path: &Path, specs: &[Spec], booked_ms: &HashMap<usize, i64>) -> Result<(), String> {
    let mut issues: Vec<JiraIssueDetails> = specs
        .iter()
        .enumerate()
        .filter(|(_, s)| s.kind == Kind::Project && s.to_week == 0)
        .filter_map(|(i, s)| {
            let (key, summary, est) = s.jira?;
            let booked_s = booked_ms.get(&i).copied().unwrap_or(0) / 1000;
            Some(JiraIssueDetails {
                issue_key: key.into(),
                summary: summary.into(),
                status: if s.name == "Performance budget" { "In Review" } else { "In Progress" }.into(),
                assignee: ASSIGNEE.into(),
                estimate_seconds: Some(est * 3600),
                total_spent_seconds: Some(booked_s * 115 / 100),
                comment_total: 0,
                latest_comment_id: String::new(),
            })
        })
        .collect();
    let extra = [
        ("TT-219", "Flaky e2e tests on checkout payment step", "To Do", Some(8), None),
        ("TT-222", "Upgrade services to Node 22 LTS", "To Do", Some(12), Some(2 * 3600)),
        ("TT-225", "CSV export uses UTC instead of account timezone", "In Progress", Some(4), Some(3600 + 1800)),
        ("TT-228", "Document export API rate limits", "To Do", None, None),
    ];
    for (key, summary, status, est, spent) in extra {
        issues.push(JiraIssueDetails {
            issue_key: key.into(),
            summary: summary.into(),
            status: status.into(),
            assignee: ASSIGNEE.into(),
            estimate_seconds: est.map(|h: i64| h * 3600),
            total_spent_seconds: spent,
                comment_total: 0,
            latest_comment_id: String::new(),
        });
    }
    let out = db_path.with_file_name("jira-assigned.json");
    std::fs::write(&out, serde_json::to_string_pretty(&issues).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}
