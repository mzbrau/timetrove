use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, Utc};

pub fn local_day_bounds(day: NaiveDate) -> (DateTime<Utc>, DateTime<Utc>) {
    let start_local = day
        .and_hms_opt(0, 0, 0)
        .expect("midnight")
        .and_local_timezone(Local)
        .single()
        .expect("local midnight");
    let end_local = (day + Duration::days(1))
        .and_hms_opt(0, 0, 0)
        .expect("next midnight")
        .and_local_timezone(Local)
        .single()
        .expect("local next midnight");
    (start_local.with_timezone(&Utc), end_local.with_timezone(&Utc))
}

pub fn today_local() -> NaiveDate {
    Local::now().date_naive()
}

/// Monday-start week containing `day`.
pub fn week_start_monday(day: NaiveDate) -> NaiveDate {
    let wd = day.weekday();
    let offset = wd.num_days_from_monday() as i64;
    day - Duration::days(offset)
}

pub fn local_midnight_on(day: NaiveDate) -> DateTime<Utc> {
    local_day_bounds(day).0
}

/// Overlap of [start, end) with [window_start, window_end) in ms.
pub fn overlap_ms(
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    window_start: DateTime<Utc>,
    window_end: DateTime<Utc>,
) -> i64 {
    let s = start.max(window_start);
    let e = end.min(window_end);
    if e > s {
        (e - s).num_milliseconds()
    } else {
        0
    }
}

pub fn parse_day(s: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|e| e.to_string())
}

pub fn format_day(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Weekday};

    #[test]
    fn monday_week_start() {
        let wed = NaiveDate::from_ymd_opt(2026, 10, 7).unwrap(); // Wednesday
        assert_eq!(
            week_start_monday(wed),
            NaiveDate::from_ymd_opt(2026, 10, 5).unwrap()
        );
        assert_eq!(week_start_monday(wed).weekday(), Weekday::Mon);

        let mon = NaiveDate::from_ymd_opt(2026, 10, 5).unwrap();
        assert_eq!(week_start_monday(mon), mon);

        let sun = NaiveDate::from_ymd_opt(2026, 10, 11).unwrap();
        assert_eq!(
            week_start_monday(sun),
            NaiveDate::from_ymd_opt(2026, 10, 5).unwrap()
        );
    }

    #[test]
    fn overlap_partial() {
        let a = Utc.with_ymd_and_hms(2026, 1, 1, 10, 0, 0).unwrap();
        let b = Utc.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).unwrap();
        let w0 = Utc.with_ymd_and_hms(2026, 1, 1, 11, 0, 0).unwrap();
        let w1 = Utc.with_ymd_and_hms(2026, 1, 1, 13, 0, 0).unwrap();
        assert_eq!(overlap_ms(a, b, w0, w1), 3_600_000);
    }

    #[test]
    fn overlap_edges() {
        let a = Utc.with_ymd_and_hms(2026, 1, 1, 10, 0, 0).unwrap();
        let b = Utc.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).unwrap();
        // No overlap
        let w0 = Utc.with_ymd_and_hms(2026, 1, 1, 13, 0, 0).unwrap();
        let w1 = Utc.with_ymd_and_hms(2026, 1, 1, 14, 0, 0).unwrap();
        assert_eq!(overlap_ms(a, b, w0, w1), 0);
        // Exact touch at endpoint → 0
        let touch = Utc.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).unwrap();
        let touch_end = Utc.with_ymd_and_hms(2026, 1, 1, 13, 0, 0).unwrap();
        assert_eq!(overlap_ms(a, b, touch, touch_end), 0);
        // Full containment
        let outer0 = Utc.with_ymd_and_hms(2026, 1, 1, 9, 0, 0).unwrap();
        let outer1 = Utc.with_ymd_and_hms(2026, 1, 1, 15, 0, 0).unwrap();
        assert_eq!(overlap_ms(a, b, outer0, outer1), 2 * 3_600_000);
    }

    #[test]
    fn parse_format_day_round_trip() {
        let d = NaiveDate::from_ymd_opt(2026, 3, 15).unwrap();
        assert_eq!(format_day(d), "2026-03-15");
        assert_eq!(parse_day("2026-03-15").unwrap(), d);
        assert!(parse_day("not-a-date").is_err());
    }

    #[test]
    fn local_day_bounds_are_ordered() {
        let day = NaiveDate::from_ymd_opt(2026, 6, 15).unwrap();
        let (start, end) = local_day_bounds(day);
        assert!(end > start);
        let ms = (end - start).num_milliseconds();
        // Allow DST shortening/lengthening (23h–25h).
        assert!(ms >= 23 * 3_600_000 && ms <= 25 * 3_600_000);
        assert_eq!(local_midnight_on(day), start);
    }
}
