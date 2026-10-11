/// Floor milliseconds to HH:MM for display and CSV.
pub fn format_hhmm(ms: i64) -> String {
    let total_mins = (ms.max(0) / 60_000) as i64;
    let hours = total_mins / 60;
    let mins = total_mins % 60;
    format!("{:02}:{:02}", hours, mins)
}

pub fn gap_band(duration_ms: i64) -> &'static str {
    const SHORT: i64 = 20 * 60_000;
    const MEDIUM: i64 = 2 * 60 * 60_000;
    if duration_ms < SHORT {
        "short"
    } else if duration_ms < MEDIUM {
        "medium"
    } else {
        "long"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_hours_minutes() {
        assert_eq!(format_hhmm(0), "00:00");
        assert_eq!(format_hhmm(5 * 60_000 + 23_000), "00:05");
        assert_eq!(format_hhmm(5 * 3_600_000 + 23 * 60_000), "05:23");
    }

    #[test]
    fn bands_gap_duration() {
        assert_eq!(gap_band(10 * 60_000), "short");
        assert_eq!(gap_band(30 * 60_000), "medium");
        assert_eq!(gap_band(3 * 3_600_000), "long");
    }
}
