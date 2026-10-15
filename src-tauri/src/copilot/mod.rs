mod client;
mod sync;

pub use client::{test_connection, CopilotCredentials};
pub use sync::{load_cached_summary, poll_interval_secs, poll_once, spawn_poll};
