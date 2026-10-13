use crate::domain::models::{
    DiscardLogEntry, InsightsReport, InsightsTaskTotal, InsightsWeekTotal, Task, TimeSlice,
};
use crate::domain::time_math::{
    format_day, local_day_bounds, overlap_ms, week_start_monday,
};
use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, Timelike, Utc};
use std::collections::{BTreeMap, HashMap};

const QUALIFYING_MS: i64 = 2 * 60 * 60 * 1000; // 2 hours
const LUNCH_START_HOUR: u32 = 10;
const LUNCH_END_HOUR: u32 = 14;

/// Aggregate insights for an inclusive local-date range.
pub fn compute_insights(
    range_start: NaiveDate,
    range_end: NaiveDate,
    slices: &[TimeSlice],
    discards: &[DiscardLogEntry],
    tasks: &HashMap<String, Task>,
    daily_goal_mins: u32,
    now: DateTime<Utc>,
) -> InsightsReport {
    let (window_start, _) = local_day_bounds(range_start);
    let (_, window_end) = local_day_bounds(range_end);
    let goal_ms = daily_goal_mins as i64 * 60_000;

    let mut day_totals: BTreeMap<NaiveDate, i64> = BTreeMap::new();
    let mut task_totals: HashMap<String, i64> = HashMap::new();
    let mut by_hour_ms = vec![0i64; 24];
    let mut week_totals: BTreeMap<NaiveDate, i64> = BTreeMap::new();

    let mut day = range_start;
    while day <= range_end {
        day_totals.insert(day, 0);
        week_totals.entry(week_start_monday(day)).or_insert(0);
        day += Duration::days(1);
    }

    for slice in slices {
        let end = slice.end.unwrap_or(now);
        let range_overlap = overlap_ms(slice.start, end, window_start, window_end);
        if range_overlap <= 0 {
            continue;
        }
        *task_totals.entry(slice.task_id.clone()).or_insert(0) += range_overlap;

        let mut d = range_start;
        while d <= range_end {
            let (d0, d1) = local_day_bounds(d);
            let day_ms = overlap_ms(slice.start, end, d0, d1);
            if day_ms > 0 {
                *day_totals.entry(d).or_insert(0) += day_ms;
                *week_totals.entry(week_start_monday(d)).or_insert(0) += day_ms;
            }
            for hour in 0..24u32 {
                let (h0, h1) = local_hour_bounds(d, hour);
                by_hour_ms[hour as usize] += overlap_ms(slice.start, end, h0, h1);
            }
            d += Duration::days(1);
        }
    }

    let total_ms: i64 = day_totals.values().sum();
    let max_day_ms = day_totals.values().copied().max().unwrap_or(0);

    let qualifying: Vec<(NaiveDate, i64)> = day_totals
        .iter()
        .filter(|(_, ms)| **ms >= QUALIFYING_MS)
        .map(|(d, ms)| (*d, *ms))
        .collect();
    let qualifying_days = qualifying.len() as u32;

    let avg_day_ms = if qualifying.is_empty() {
        None
    } else {
        Some(qualifying.iter().map(|(_, ms)| *ms).sum::<i64>() / qualifying.len() as i64)
    };

    let mut weekday_sums = [0i64; 7];
    let mut weekday_counts = [0u32; 7];
    let mut goal_hits = 0u32;
    for (d, ms) in &qualifying {
        let idx = d.weekday().num_days_from_monday() as usize;
        weekday_sums[idx] += ms;
        weekday_counts[idx] += 1;
        if *ms >= goal_ms {
            goal_hits += 1;
        }
    }
    let avg_by_weekday_ms: Vec<Option<i64>> = (0..7)
        .map(|i| {
            if weekday_counts[i] == 0 {
                None
            } else {
                Some(weekday_sums[i] / weekday_counts[i] as i64)
            }
        })
        .collect();

    let goal_hit_rate = if qualifying_days == 0 {
        None
    } else {
        Some(goal_hits as f64 / qualifying_days as f64)
    };

    // Schedule stats + lunch on qualifying days
    let mut first_starts: Vec<u32> = Vec::new();
    let mut last_finishes: Vec<u32> = Vec::new();
    let mut lunch_days: Vec<i64> = Vec::new();

    for (d, _) in &qualifying {
        let (d0, d1) = local_day_bounds(*d);
        let mut segments: Vec<(DateTime<Utc>, DateTime<Utc>)> = Vec::new();
        for slice in slices {
            let end = slice.end.unwrap_or(now);
            let s = slice.start.max(d0);
            let e = end.min(d1);
            if e > s {
                segments.push((s, e));
            }
        }
        segments.sort_by_key(|(s, _)| *s);
        if let Some((first, _)) = segments.first() {
            first_starts.push(local_mins_from_midnight(*first));
        }
        if let Some((_, last)) = segments.last() {
            last_finishes.push(local_mins_from_midnight(*last));
        }

        let lunch_start = local_clock(*d, LUNCH_START_HOUR, 0);
        let lunch_end = local_clock(*d, LUNCH_END_HOUR, 0);
        let mut lunch_ms = 0i64;
        for w in segments.windows(2) {
            let gap_start = w[0].1;
            let gap_end = w[1].0;
            if gap_end > gap_start {
                lunch_ms += overlap_ms(gap_start, gap_end, lunch_start, lunch_end);
            }
        }
        if lunch_ms > 0 {
            lunch_days.push(lunch_ms);
        }
    }

    let earliest_start_mins = first_starts.iter().copied().min();
    let latest_finish_mins = last_finishes.iter().copied().max();
    let avg_start_mins = mean_mins(&first_starts);
    let avg_finish_mins = mean_mins(&last_finishes);
    let avg_lunch_ms = if lunch_days.is_empty() {
        None
    } else {
        Some(lunch_days.iter().sum::<i64>() / lunch_days.len() as i64)
    };

    // Session lengths: slices overlapping the window
    let mut session_ms: Vec<i64> = Vec::new();
    for slice in slices {
        let end = slice.end.unwrap_or(now);
        if overlap_ms(slice.start, end, window_start, window_end) > 0 {
            session_ms.push((end - slice.start).num_milliseconds().max(0));
        }
    }
    let avg_session_ms = if session_ms.is_empty() {
        None
    } else {
        Some(session_ms.iter().sum::<i64>() / session_ms.len() as i64)
    };

    let discarded_ms: i64 = discards
        .iter()
        .map(|d| overlap_ms(d.start, d.end, window_start, window_end))
        .sum();

    let mut task_rows: Vec<InsightsTaskTotal> = task_totals
        .into_iter()
        .filter(|(_, ms)| *ms > 0)
        .map(|(id, total_ms)| {
            let (name, color) = tasks
                .get(&id)
                .map(|t| (t.name.clone(), t.color.clone()))
                .unwrap_or_else(|| (id.clone(), "#0f766e".into()));
            InsightsTaskTotal {
                task_id: id,
                name,
                color,
                total_ms,
            }
        })
        .collect();
    task_rows.sort_by(|a, b| b.total_ms.cmp(&a.total_ms).then_with(|| a.name.cmp(&b.name)));

    let weekly: Vec<InsightsWeekTotal> = week_totals
        .into_iter()
        .map(|(week_start, total_ms)| InsightsWeekTotal {
            week_start: format_day(week_start),
            total_ms,
        })
        .collect();

    InsightsReport {
        range_start: format_day(range_start),
        range_end: format_day(range_end),
        total_ms,
        max_day_ms,
        avg_day_ms,
        avg_by_weekday_ms,
        earliest_start_mins,
        latest_finish_mins,
        avg_start_mins,
        avg_finish_mins,
        avg_lunch_ms,
        qualifying_days,
        goal_hit_rate,
        avg_session_ms,
        discarded_ms,
        tasks: task_rows,
        weekly,
        by_hour_ms,
    }
}

fn mean_mins(values: &[u32]) -> Option<u32> {
    if values.is_empty() {
        None
    } else {
        let sum: u64 = values.iter().map(|v| *v as u64).sum();
        Some((sum / values.len() as u64) as u32)
    }
}

fn local_mins_from_midnight(dt: DateTime<Utc>) -> u32 {
    let local = dt.with_timezone(&Local);
    local.hour() * 60 + local.minute()
}

fn local_clock(day: NaiveDate, hour: u32, min: u32) -> DateTime<Utc> {
    let naive = day.and_hms_opt(hour, min, 0).expect("valid clock");
    // DST spring-forward gaps yield None; fall-back yields Ambiguous — prefer earliest.
    let local = match naive.and_local_timezone(Local) {
        chrono::LocalResult::Single(dt) => dt,
        chrono::LocalResult::Ambiguous(earliest, _) => earliest,
        chrono::LocalResult::None => {
            // Skip forward through the missing hour until a valid local time exists.
            let mut t = naive;
            for _ in 0..180 {
                t += Duration::minutes(1);
                match t.and_local_timezone(Local) {
                    chrono::LocalResult::Single(dt) => return dt.with_timezone(&Utc),
                    chrono::LocalResult::Ambiguous(earliest, _) => {
                        return earliest.with_timezone(&Utc);
                    }
                    chrono::LocalResult::None => {}
                }
            }
            panic!("local clock: no valid local time near {naive}");
        }
    };
    local.with_timezone(&Utc)
}

fn local_hour_bounds(day: NaiveDate, hour: u32) -> (DateTime<Utc>, DateTime<Utc>) {
    let start = local_clock(day, hour, 0);
    let end = if hour == 23 {
        local_day_bounds(day).1
    } else {
        local_clock(day, hour + 1, 0)
    };
    (start, end)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(id: &str, name: &str, color: &str) -> Task {
        Task {
            id: id.into(),
            name: name.into(),
            color: color.into(),
            description: String::new(),
            notes: String::new(),
            archived: false,
            starred: false,
            jira_issue_key: String::new(),
            jira_reported_ms: 0,
            jira_pending_ms: 0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn slice(id: &str, task_id: &str, start: DateTime<Utc>, end: DateTime<Utc>) -> TimeSlice {
        TimeSlice {
            id: id.into(),
            task_id: task_id.into(),
            start,
            end: Some(end),
            note: String::new(),
        }
    }

    /// Local wall-clock → Utc for tests (DST-safe via Local).
    fn local_dt(day: NaiveDate, hour: u32, min: u32) -> DateTime<Utc> {
        local_clock(day, hour, min)
    }

    #[test]
    fn empty_range_zeros() {
        let day = NaiveDate::from_ymd_opt(2026, 3, 1).unwrap();
        let report = compute_insights(
            day,
            day,
            &[],
            &[],
            &HashMap::new(),
            480,
            Utc::now(),
        );
        assert_eq!(report.total_ms, 0);
        assert_eq!(report.max_day_ms, 0);
        assert!(report.avg_day_ms.is_none());
        assert_eq!(report.qualifying_days, 0);
        assert!(report.goal_hit_rate.is_none());
        assert!(report.avg_lunch_ms.is_none());
        assert!(report.tasks.is_empty());
        assert_eq!(report.by_hour_ms.len(), 24);
        assert!(report.by_hour_ms.iter().all(|v| *v == 0));
        assert_eq!(report.discarded_ms, 0);
    }

    #[test]
    fn qualifying_threshold_and_weekday_avg() {
        let mon = NaiveDate::from_ymd_opt(2026, 3, 2).unwrap(); // Monday
        let tue = NaiveDate::from_ymd_opt(2026, 3, 3).unwrap();
        let mut tasks = HashMap::new();
        tasks.insert("t1".into(), task("t1", "Alpha", "#E4572E"));

        // Mon: 3h (qualifying), Tue: 1h (not qualifying)
        let slices = vec![
            slice(
                "s1",
                "t1",
                local_dt(mon, 9, 0),
                local_dt(mon, 12, 0),
            ),
            slice(
                "s2",
                "t1",
                local_dt(tue, 9, 0),
                local_dt(tue, 10, 0),
            ),
        ];
        let report = compute_insights(mon, tue, &slices, &[], &tasks, 480, Utc::now());
        assert_eq!(report.qualifying_days, 1);
        assert_eq!(report.avg_day_ms, Some(3 * 3_600_000));
        assert_eq!(report.avg_by_weekday_ms[0], Some(3 * 3_600_000)); // Monday
        assert!(report.avg_by_weekday_ms[1].is_none()); // Tuesday not qualifying
        assert_eq!(report.total_ms, 4 * 3_600_000);
        assert_eq!(report.max_day_ms, 3 * 3_600_000);
        assert_eq!(report.tasks.len(), 1);
        assert_eq!(report.tasks[0].total_ms, 4 * 3_600_000);
    }

    #[test]
    fn lunch_window_clips_midday_gap() {
        let day = NaiveDate::from_ymd_opt(2026, 3, 4).unwrap(); // Wednesday
        let mut tasks = HashMap::new();
        tasks.insert("t1".into(), task("t1", "Work", "#17BEBB"));

        // 9–11 and 12:30–15 → 2.5h + 2.5h = 5h qualifying; lunch gap 11–12:30 clipped to 11–12 = 1h
        // Wait: lunch window is 10–14. Gap is 11:00–12:30 → full 1.5h inside lunch window.
        let slices = vec![
            slice("a", "t1", local_dt(day, 9, 0), local_dt(day, 11, 0)),
            slice("b", "t1", local_dt(day, 12, 30), local_dt(day, 15, 0)),
        ];
        let report = compute_insights(day, day, &slices, &[], &tasks, 480, Utc::now());
        assert_eq!(report.qualifying_days, 1);
        assert_eq!(report.avg_lunch_ms, Some(90 * 60_000)); // 11:00–12:30
    }

    #[test]
    fn lunch_excludes_zero_and_outside_window() {
        let day = NaiveDate::from_ymd_opt(2026, 3, 5).unwrap();
        let mut tasks = HashMap::new();
        tasks.insert("t1".into(), task("t1", "Work", "#17BEBB"));

        // Continuous 8–16: no gap → no lunch
        let slices = vec![slice(
            "a",
            "t1",
            local_dt(day, 8, 0),
            local_dt(day, 16, 0),
        )];
        let report = compute_insights(day, day, &slices, &[], &tasks, 480, Utc::now());
        assert!(report.avg_lunch_ms.is_none());

        // Gap only before 10:00 (8:00–8:30 work, 9:00–12:00 work) — gap 8:30–9:00 outside lunch
        let day2 = NaiveDate::from_ymd_opt(2026, 3, 6).unwrap();
        let slices2 = vec![
            slice("a", "t1", local_dt(day2, 8, 0), local_dt(day2, 8, 30)),
            slice("b", "t1", local_dt(day2, 9, 0), local_dt(day2, 12, 0)),
        ];
        // Only 3.5h — qualifying. Gap 8:30–9:00 has no overlap with [10,14)
        let report2 = compute_insights(day2, day2, &slices2, &[], &tasks, 480, Utc::now());
        assert_eq!(report2.qualifying_days, 1);
        assert!(report2.avg_lunch_ms.is_none());
    }

    #[test]
    fn hour_buckets_and_start_finish() {
        let day = NaiveDate::from_ymd_opt(2026, 3, 9).unwrap(); // Monday
        let mut tasks = HashMap::new();
        tasks.insert("t1".into(), task("t1", "Focus", "#E4572E"));

        let slices = vec![slice(
            "s",
            "t1",
            local_dt(day, 9, 0),
            local_dt(day, 11, 0),
        )];
        let report = compute_insights(day, day, &slices, &[], &tasks, 480, Utc::now());
        assert_eq!(report.by_hour_ms[9], 3_600_000);
        assert_eq!(report.by_hour_ms[10], 3_600_000);
        assert_eq!(report.by_hour_ms[8], 0);
        assert_eq!(report.earliest_start_mins, Some(9 * 60));
        assert_eq!(report.latest_finish_mins, Some(11 * 60));
        assert_eq!(report.avg_start_mins, Some(9 * 60));
        assert_eq!(report.avg_finish_mins, Some(11 * 60));
        // 2h exactly → qualifying
        assert_eq!(report.qualifying_days, 1);
    }

    #[test]
    fn discarded_overlap_and_goal_rate() {
        let day = NaiveDate::from_ymd_opt(2026, 3, 10).unwrap();
        let mut tasks = HashMap::new();
        tasks.insert("t1".into(), task("t1", "A", "#111111"));

        let slices = vec![slice(
            "s",
            "t1",
            local_dt(day, 8, 0),
            local_dt(day, 16, 0), // 8h ≥ goal 8h
        )];
        let discards = vec![DiscardLogEntry {
            id: "d1".into(),
            start: local_dt(day, 12, 0),
            end: local_dt(day, 12, 30),
            duration_ms: 30 * 60_000,
            created_at: Utc::now(),
        }];
        let report = compute_insights(day, day, &slices, &discards, &tasks, 480, Utc::now());
        assert_eq!(report.discarded_ms, 30 * 60_000);
        assert_eq!(report.goal_hit_rate, Some(1.0));
        assert_eq!(report.avg_session_ms, Some(8 * 3_600_000));
    }

    #[test]
    fn weekly_buckets_span_weeks() {
        let mon = NaiveDate::from_ymd_opt(2026, 3, 2).unwrap();
        let next_mon = NaiveDate::from_ymd_opt(2026, 3, 9).unwrap();
        let mut tasks = HashMap::new();
        tasks.insert("t1".into(), task("t1", "A", "#111111"));
        let slices = vec![
            slice("a", "t1", local_dt(mon, 9, 0), local_dt(mon, 10, 0)),
            slice(
                "b",
                "t1",
                local_dt(next_mon, 9, 0),
                local_dt(next_mon, 11, 0),
            ),
        ];
        let report = compute_insights(mon, next_mon, &slices, &[], &tasks, 480, Utc::now());
        assert_eq!(report.weekly.len(), 2);
        assert_eq!(report.weekly[0].week_start, format_day(mon));
        assert_eq!(report.weekly[0].total_ms, 3_600_000);
        assert_eq!(report.weekly[1].week_start, format_day(next_mon));
        assert_eq!(report.weekly[1].total_ms, 2 * 3_600_000);
    }

    #[test]
    fn under_two_hours_not_qualifying() {
        let day = NaiveDate::from_ymd_opt(2026, 3, 11).unwrap();
        let mut tasks = HashMap::new();
        tasks.insert("t1".into(), task("t1", "A", "#111111"));
        let slices = vec![slice(
            "s",
            "t1",
            local_dt(day, 9, 0),
            local_dt(day, 10, 59), // 1h59m
        )];
        let report = compute_insights(day, day, &slices, &[], &tasks, 480, Utc::now());
        assert_eq!(report.qualifying_days, 0);
        assert!(report.avg_day_ms.is_none());
        assert!(report.earliest_start_mins.is_none());
        assert_eq!(report.total_ms, (1 * 3600 + 59 * 60) * 1000);
    }
}
