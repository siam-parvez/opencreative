//! Claude plan usage, read from the snapshot that OpenCUT's `ocutd` writes
//! (`$XDG_RUNTIME_DIR/opencut/usage.json`, schema `opencut/usage@1`).
//!
//! OpenCreative never talks to Anthropic or reads credentials itself; it only
//! reads this file, so OpenCUT stays the single source of the numbers.

use std::path::PathBuf;

use serde::Deserialize;

/// Snapshots older than this are treated as missing (ocutd writes every 30 s).
const MAX_AGE_SECS: i64 = 120;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ClaudeUsage {
    /// The 5-hour session window.
    pub session: Option<Window>,
    /// The weekly window across all models.
    pub weekly: Option<Window>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Window {
    pub pct: f32,
    /// Seconds until the window resets, as of the time of reading.
    pub resets_in_secs: Option<u64>,
}

#[derive(Deserialize)]
struct File {
    schema: String,
    generated_at: String,
    windows: Windows,
}

#[derive(Deserialize)]
struct Windows {
    session_5h: Option<RawWindow>,
    weekly_all: Option<RawWindow>,
}

#[derive(Deserialize)]
struct RawWindow {
    pct: Option<f32>,
    seconds_to_reset: Option<u64>,
}

fn snapshot_path() -> Option<PathBuf> {
    let dir = std::env::var_os("XDG_RUNTIME_DIR")?;
    Some(PathBuf::from(dir).join("opencut").join("usage.json"))
}

/// The current usage, or `None` when OpenCUT isn't running or its snapshot
/// is missing, unreadable or stale.
pub fn read() -> Option<ClaudeUsage> {
    let text = std::fs::read_to_string(snapshot_path()?).ok()?;
    parse(&text, chrono::Utc::now())
}

fn parse(text: &str, now: chrono::DateTime<chrono::Utc>) -> Option<ClaudeUsage> {
    let file: File = serde_json::from_str(text).ok()?;
    if !file.schema.starts_with("opencut/usage@1") {
        return None;
    }
    let generated = chrono::DateTime::parse_from_rfc3339(&file.generated_at).ok()?;
    let age = (now - generated.with_timezone(&chrono::Utc)).num_seconds();
    if !(0..=MAX_AGE_SECS).contains(&age) {
        return None;
    }
    let window = |raw: Option<RawWindow>| {
        let raw = raw?;
        Some(Window {
            pct: raw.pct?,
            resets_in_secs: raw.seconds_to_reset.map(|s| s.saturating_sub(age as u64)),
        })
    };
    Some(ClaudeUsage {
        session: window(file.windows.session_5h),
        weekly: window(file.windows.weekly_all),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
        "schema": "opencut/usage@1",
        "generated_at": "2026-10-11T12:00:00Z",
        "status": "ok",
        "windows": {
            "session_5h": {"pct": 78.0, "seconds_to_reset": 15794, "stale": false},
            "weekly_all": {"pct": 55.0, "seconds_to_reset": 513794, "stale": false},
            "weekly_opus": null
        }
    }"#;

    fn at(secs_after: i64) -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::parse_from_rfc3339("2026-10-11T12:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc)
            + chrono::Duration::seconds(secs_after)
    }

    #[test]
    fn reads_both_windows_and_ages_the_countdown() {
        let usage = parse(SAMPLE, at(10)).unwrap();
        let session = usage.session.unwrap();
        assert_eq!(session.pct, 78.0);
        assert_eq!(session.resets_in_secs, Some(15784));
        assert_eq!(usage.weekly.unwrap().pct, 55.0);
    }

    #[test]
    fn stale_or_foreign_snapshots_are_ignored() {
        assert_eq!(parse(SAMPLE, at(MAX_AGE_SECS + 1)), None);
        assert_eq!(parse("{}", at(0)), None);
        let other = SAMPLE.replace("opencut/usage@1", "something/else@1");
        assert_eq!(parse(&other, at(0)), None);
    }

    #[test]
    fn a_closed_window_is_none() {
        let closed = SAMPLE.replace(
            r#""session_5h": {"pct": 78.0, "seconds_to_reset": 15794, "stale": false}"#,
            r#""session_5h": null"#,
        );
        let usage = parse(&closed, at(0)).unwrap();
        assert_eq!(usage.session, None);
        assert!(usage.weekly.is_some());
    }
}
