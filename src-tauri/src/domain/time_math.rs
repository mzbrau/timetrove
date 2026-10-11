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
    }

    #[test]
    fn overlap_partial() {
        let a = Utc.with_ymd_and_hms(2026, 1, 1, 10, 0, 0).unwrap();
        let b = Utc.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).unwrap();
        let w0 = Utc.with_ymd_and_hms(2026, 1, 1, 11, 0, 0).unwrap();
        let w1 = Utc.with_ymd_and_hms(2026, 1, 1, 13, 0, 0).unwrap();
        assert_eq!(overlap_ms(a, b, w0, w1), 3_600_000);
    }
}
