mod client;
mod sync;

pub use client::{test_connection, JiraCredentials};
pub use sync::{spawn_flush_all, spawn_flush_task};

pub const MIN_WORKLOG_MS: i64 = 60_000;
pub const SYNC_INTERVAL_SECS: u64 = 3600;

#[cfg(test)]
mod tests {
    use super::client::normalize_site_url;
    use super::sync::unreported_ms;

    #[test]
    fn unreported_delta_and_floor() {
        assert_eq!(unreported_ms(120_000, 0), 120_000);
        assert_eq!(unreported_ms(120_000, 50_000), 70_000);
        assert_eq!(unreported_ms(50_000, 0), 0); // under 60s
        assert_eq!(unreported_ms(59_999, 0), 0);
        assert_eq!(unreported_ms(60_000, 0), 60_000);
        assert_eq!(unreported_ms(100_000, 150_000), 0); // over-reported
    }

    #[test]
    fn normalize_site_strips_slash() {
        assert_eq!(
            normalize_site_url("https://example.atlassian.net/").unwrap(),
            "https://example.atlassian.net"
        );
        assert_eq!(
            normalize_site_url("https://example.atlassian.net").unwrap(),
            "https://example.atlassian.net"
        );
        assert!(normalize_site_url("").is_err());
        assert!(normalize_site_url("example.atlassian.net").is_err());
    }
}
