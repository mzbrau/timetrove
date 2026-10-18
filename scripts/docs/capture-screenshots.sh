#!/usr/bin/env bash
# Capture documentation screenshots from the real app (macOS only).
#
# Seeds a demo database (your real timetrove.db is never touched), launches
# `pnpm tauri dev` in demo mode, and lets the dev-only shot runner capture each
# window listed in docs/shots.json into website/static/img/screenshots/.
#
# Shots use `screencapture` (native title bar and rounded corners), which needs
# Screen Recording permission for the terminal. Without it, set
# SHOT_CAPTURE=webview for WebKit snapshots (no chrome; the landing-page crops in
# website/src/pages/index.tsx assume the title bar and will sit 28pt too low).
# SHOT_OUT / SHOT_PLAN overrides must be absolute (the app runs from src-tauri/).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DEMO_DIR="$ROOT/src-tauri/target/demo"
DB="$DEMO_DIR/timetrove-demo.db"
OUT="${SHOT_OUT:-$ROOT/website/static/img/screenshots}"
PLAN="${SHOT_PLAN:-$ROOT/docs/shots.json}"

if [[ "$(uname)" != "Darwin" ]]; then
  echo "Screenshot capture drives the macOS WebKit webviews; run this on a Mac." >&2
  exit 1
fi

# The single-instance plugin would hand our launch to an already-running TimeTrove.
if pgrep -x TimeTrove >/dev/null || pgrep -x time-trove >/dev/null; then
  echo "TimeTrove is running. Quit it (tray > Quit) and re-run." >&2
  exit 1
fi

cd "$ROOT"
echo "Building app..."
cargo build --manifest-path src-tauri/Cargo.toml --examples --bins

echo "Seeding demo database..."
mkdir -p "$DEMO_DIR" "$OUT"
cargo run -q --manifest-path src-tauri/Cargo.toml --example seed_demo -- "$DB"

echo "Launching TimeTrove in demo mode..."
TIMETROVE_DEMO=1 \
TIMETROVE_DB_PATH="$DB" \
TIMETROVE_DEMO_JIRA_FIXTURE="$DEMO_DIR/jira-assigned.json" \
TIMETROVE_SHOT_PLAN="$PLAN" \
TIMETROVE_SHOT_OUT="$OUT" \
TIMETROVE_SHOT_CAPTURE="${SHOT_CAPTURE:-window}" \
  pnpm tauri dev

echo
echo "Screenshots in $OUT:"
ls -1 "$OUT"
