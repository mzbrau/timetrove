//! Seed a deterministic 12-month demo database for documentation screenshots.
//!
//! Usage: cargo run --example seed_demo -- target/demo/timetrove-demo.db

fn main() {
    #[cfg(debug_assertions)]
    {
        let out = std::env::args()
            .nth(1)
            .unwrap_or_else(|| "target/demo/timetrove-demo.db".into());
        let path = std::path::PathBuf::from(out);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create output dir");
        }
        match time_trove_lib::seed_demo_db(&path) {
            Ok(()) => println!("Seeded demo database at {}", path.display()),
            Err(e) => {
                eprintln!("Seeding failed: {e}");
                std::process::exit(1);
            }
        }
    }
    #[cfg(not(debug_assertions))]
    eprintln!("seed_demo is only available in debug builds");
}
