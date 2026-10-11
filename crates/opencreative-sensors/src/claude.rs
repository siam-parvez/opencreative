//! Claude plan usage (the 5-hour and weekly limits), read straight from
//! Anthropic with the login Claude Code already keeps on this computer.
//!
//! Opt-in: nothing is read or sent until [`set_enabled`] turns it on. The
//! token comes from Claude Code's credentials (`~/.claude/.credentials.json`)
//! and is only ever sent to [`USAGE_URL`]; it is never
//! refreshed or stored by us. A background thread polls every
//! [`POLL_EVERY`]; results are cached in a file so the daemon and the app
//! share one poll instead of each making their own.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, Once};
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const BETA: &str = "oauth-2025-04-20";
/// How often usage is fetched. The endpoint is rate limited; stay polite.
const POLL_EVERY: Duration = Duration::from_secs(90);
/// After a failure, wait this long before trying again.
const RETRY_AFTER: Duration = Duration::from_secs(300);
/// Readings older than this are treated as missing.
const MAX_AGE_SECS: i64 = 15 * 60;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ClaudeUsage {
    /// The 5-hour limit.
    pub session: Option<Window>,
    /// The weekly limit across all models.
    pub weekly: Option<Window>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Window {
    pub pct: f32,
    /// Seconds until the limit resets, as of the time of reading.
    pub resets_in_secs: Option<u64>,
}

/// Why there is no usage to show, for the app to explain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Status {
    /// Turned off, or not polled yet.
    #[default]
    Off,
    /// Polled; usage is available.
    Ok,
    /// Claude Code isn't signed in on this computer.
    NotSignedIn,
    /// The saved login expired. Running `claude` once renews it.
    Expired,
    /// The request failed (offline, rate limited, server error).
    Unreachable,
}

static ENABLED: AtomicBool = AtomicBool::new(false);
static START: Once = Once::new();
static STATE: Mutex<(Status, Option<Cached>)> = Mutex::new((Status::Off, None));

/// Turns polling on or off. The first time it is turned on a background
/// thread starts; turning it off stops all requests.
pub fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
    if on {
        START.call_once(|| {
            let spawned = std::thread::Builder::new()
                .name("claude-usage".into())
                .spawn(poll_loop);
            if spawned.is_err() {
                set_state(Status::Unreachable, None);
            }
        });
    }
}

/// The latest usage, or `None` when off, signed out or out of date.
pub fn read() -> Option<ClaudeUsage> {
    if !ENABLED.load(Ordering::Relaxed) {
        return None;
    }
    let cached = STATE.lock().ok()?.1.clone()?;
    cached.usage(Utc::now())
}

pub fn status() -> Status {
    if !ENABLED.load(Ordering::Relaxed) {
        return Status::Off;
    }
    STATE.lock().map(|s| s.0).unwrap_or_default()
}

fn set_state(status: Status, cached: Option<Cached>) {
    if let Ok(mut state) = STATE.lock() {
        state.0 = status;
        if cached.is_some() {
            state.1 = cached;
        }
    }
}

/// A fetch result with absolute times, as kept in memory and on disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Cached {
    fetched_at: DateTime<Utc>,
    session: Option<CachedWindow>,
    weekly: Option<CachedWindow>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct CachedWindow {
    pct: f32,
    resets_at: Option<DateTime<Utc>>,
}

impl Cached {
    fn usage(&self, now: DateTime<Utc>) -> Option<ClaudeUsage> {
        if (now - self.fetched_at).num_seconds() > MAX_AGE_SECS {
            return None;
        }
        let window = |w: Option<CachedWindow>| {
            let w = w?;
            Some(Window {
                pct: w.pct,
                resets_in_secs: w.resets_at.map(|t| (t - now).num_seconds().max(0) as u64),
            })
        };
        Some(ClaudeUsage {
            session: window(self.session),
            weekly: window(self.weekly),
        })
    }
}

fn poll_loop() {
    loop {
        let wait = if ENABLED.load(Ordering::Relaxed) {
            poll_once()
        } else {
            POLL_EVERY
        };
        std::thread::sleep(wait);
    }
}

/// Uses a fresh shared cache when there is one, else fetches. Returns how
/// long to wait before the next poll.
fn poll_once() -> Duration {
    let now = Utc::now();
    let cache = cache_path();
    if let Some(cached) = cache.as_deref().and_then(read_cache) {
        let age = (now - cached.fetched_at).to_std().unwrap_or_default();
        if age < POLL_EVERY {
            set_state(Status::Ok, Some(cached));
            return POLL_EVERY - age + Duration::from_secs(1);
        }
    }
    let token = match token() {
        Ok(token) => token,
        Err(status) => {
            set_state(status, None);
            return RETRY_AFTER;
        }
    };
    match fetch(&token, now) {
        Ok(cached) => {
            if let Some(path) = &cache {
                write_cache(path, &cached);
            }
            set_state(Status::Ok, Some(cached));
            POLL_EVERY
        }
        Err(status) => {
            set_state(status, None);
            RETRY_AFTER
        }
    }
}

fn cache_path() -> Option<PathBuf> {
    let dir = dirs::runtime_dir().or_else(dirs::cache_dir)?;
    Some(dir.join("opencreative").join("claude-usage.json"))
}

fn read_cache(path: &Path) -> Option<Cached> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

fn write_cache(path: &Path, cached: &Cached) {
    let Some(dir) = path.parent() else { return };
    let Ok(json) = serde_json::to_string(cached) else {
        return;
    };
    // Write then rename, so a reader never sees half a file.
    let tmp = dir.join(format!(".claude-usage.{}", std::process::id()));
    let written = std::fs::create_dir_all(dir)
        .and_then(|()| std::fs::write(&tmp, json))
        .and_then(|()| std::fs::rename(&tmp, path));
    if written.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
}

/// Claude Code's OAuth access token, unless missing or expired.
fn token() -> Result<String, Status> {
    let json = credentials_json().ok_or(Status::NotSignedIn)?;
    parse_token(&json, Utc::now())
}

fn parse_token(json: &str, now: DateTime<Utc>) -> Result<String, Status> {
    let root: Value = serde_json::from_str(json).map_err(|_| Status::NotSignedIn)?;
    let oauth = root.get("claudeAiOauth").ok_or(Status::NotSignedIn)?;
    let token = oauth
        .get("accessToken")
        .and_then(Value::as_str)
        .filter(|t| !t.is_empty())
        .ok_or(Status::NotSignedIn)?;
    // `expiresAt` is epoch milliseconds.
    let expired = oauth
        .get("expiresAt")
        .and_then(Value::as_i64)
        .and_then(DateTime::from_timestamp_millis)
        .is_some_and(|t| t <= now);
    if expired {
        return Err(Status::Expired);
    }
    Ok(token.to_owned())
}

fn credentials_json() -> Option<String> {
    let dir = std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".claude")))?;
    std::fs::read_to_string(dir.join(".credentials.json")).ok()
}

fn fetch(token: &str, now: DateTime<Utc>) -> Result<Cached, Status> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(15)))
        // Never follow a redirect, so the token only reaches USAGE_URL.
        .max_redirects(0)
        .http_status_as_error(false)
        .user_agent(concat!("opencreative/", env!("CARGO_PKG_VERSION")))
        .build()
        .into();
    let mut response = agent
        .get(USAGE_URL)
        .header("Authorization", &format!("Bearer {token}"))
        .header("anthropic-beta", BETA)
        .header("Accept", "application/json")
        .call()
        .map_err(|_| Status::Unreachable)?;
    match response.status().as_u16() {
        200 => {}
        401 => return Err(Status::Expired),
        _ => return Err(Status::Unreachable),
    }
    let body = response
        .body_mut()
        .read_to_string()
        .map_err(|_| Status::Unreachable)?;
    parse_response(&body, now).ok_or(Status::Unreachable)
}

/// Parses `{"five_hour": {"utilization", "resets_at"}, "seven_day": ...}`.
/// Also accepts a `limits: [{kind, percent, resets_at}]` array.
fn parse_response(body: &str, now: DateTime<Utc>) -> Option<Cached> {
    let root: Value = serde_json::from_str(body).ok()?;
    let mut session = None;
    let mut weekly = None;
    let mut known = false;
    for (key, slot) in [("five_hour", &mut session), ("seven_day", &mut weekly)] {
        if let Some(v) = root.get(key) {
            known = true;
            *slot = window(v, "utilization");
        }
    }
    for entry in root
        .get("limits")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        known = true;
        let slot = match entry.get("kind").and_then(Value::as_str) {
            Some("session" | "five_hour") => &mut session,
            Some("weekly_all" | "seven_day") => &mut weekly,
            _ => continue,
        };
        *slot = window(entry, "percent").or(*slot);
    }
    known.then_some(Cached {
        fetched_at: now,
        session,
        weekly,
    })
}

fn window(v: &Value, pct_key: &str) -> Option<CachedWindow> {
    let pct = v
        .get(pct_key)
        .or_else(|| v.get("utilization"))
        .and_then(Value::as_f64)?;
    Some(CachedWindow {
        pct: pct as f32,
        resets_at: v.get("resets_at").and_then(timestamp),
    })
}

/// ISO 8601 or epoch seconds.
fn timestamp(v: &Value) -> Option<DateTime<Utc>> {
    match v {
        Value::String(s) => DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|t| t.with_timezone(&Utc)),
        Value::Number(n) => DateTime::from_timestamp(n.as_i64()?, 0),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    const NOW: &str = "2026-10-11T12:00:00Z";
    const BODY: &str = r#"{
        "five_hour": {"utilization": 78.0, "resets_at": "2026-10-11T16:23:14Z"},
        "seven_day": {"utilization": 55.0, "resets_at": "2026-10-17T10:43:14+00:00"},
        "seven_day_opus": null,
        "extra_usage": {"is_enabled": false}
    }"#;

    #[test]
    fn reads_both_limits_and_counts_down_to_reset() {
        let cached = parse_response(BODY, at(NOW)).unwrap();
        let usage = cached.usage(at("2026-10-11T12:00:10Z")).unwrap();
        let session = usage.session.unwrap();
        assert_eq!(session.pct, 78.0);
        assert_eq!(session.resets_in_secs, Some(15784));
        assert_eq!(usage.weekly.unwrap().pct, 55.0);
    }

    #[test]
    fn a_closed_window_is_none_and_unknown_bodies_fail() {
        let closed = r#"{"five_hour": null, "seven_day": {"utilization": 5, "resets_at": null}}"#;
        let usage = parse_response(closed, at(NOW))
            .unwrap()
            .usage(at(NOW))
            .unwrap();
        assert_eq!(usage.session, None);
        assert_eq!(usage.weekly.unwrap().resets_in_secs, None);
        assert!(parse_response("{}", at(NOW)).is_none());
        assert!(parse_response("not json", at(NOW)).is_none());
    }

    #[test]
    fn the_limits_array_shape_also_parses() {
        let body = r#"{"limits": [
            {"kind": "session", "percent": 12.5, "resets_at": 1791720000},
            {"kind": "weekly_scoped", "model": "opus", "percent": 90}
        ]}"#;
        let usage = parse_response(body, at(NOW))
            .unwrap()
            .usage(at(NOW))
            .unwrap();
        assert_eq!(usage.session.unwrap().pct, 12.5);
        assert_eq!(usage.weekly, None);
    }

    #[test]
    fn old_readings_are_dropped() {
        let cached = parse_response(BODY, at(NOW)).unwrap();
        assert!(cached.usage(at("2026-10-11T12:16:00Z")).is_none());
    }

    #[test]
    fn tokens_are_read_and_expiry_is_respected() {
        let creds = |expires: i64| {
            format!(
                r#"{{"claudeAiOauth": {{"accessToken": "sk-ant-oat-x", "expiresAt": {expires}}}}}"#
            )
        };
        let now = at(NOW);
        let later = now.timestamp_millis() + 60_000;
        assert_eq!(parse_token(&creds(later), now).unwrap(), "sk-ant-oat-x");
        assert_eq!(
            parse_token(&creds(later - 120_000), now),
            Err(Status::Expired)
        );
        assert_eq!(parse_token("{}", now), Err(Status::NotSignedIn));
    }
}
