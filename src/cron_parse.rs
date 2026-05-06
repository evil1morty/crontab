//! Thin wrapper over the `cron` crate that accepts classic 5-field
//! expressions and the common `@hourly` / `@daily` / `@weekly` / `@monthly`
//! / `@yearly` aliases. Internally we always pass the crate a 7-field
//! string so the parser is happy.

use chrono::{DateTime, Local};
use cron::Schedule;
use std::str::FromStr;

/// `cron` expects 6 or 7 fields with seconds first; users write the classic
/// 5-field form. Prepend a `0` for the seconds slot and append `*` for year
/// so we accept both flavors without surprising the user.
fn normalize(expr: &str) -> String {
    let trimmed = expr.trim();
    match trimmed {
        "@yearly" | "@annually" => "0 0 0 1 1 * *".to_string(),
        "@monthly" => "0 0 0 1 * * *".to_string(),
        "@weekly" => "0 0 0 * * 0 *".to_string(),
        "@daily" | "@midnight" => "0 0 0 * * * *".to_string(),
        "@hourly" => "0 0 * * * * *".to_string(),
        _ => {
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() == 5 {
                format!("0 {} *", trimmed)
            } else {
                trimmed.to_string()
            }
        }
    }
}

pub fn validate(expr: &str) -> Result<(), String> {
    let n = normalize(expr);
    Schedule::from_str(&n)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

pub fn next_run(expr: &str, after: DateTime<Local>) -> Option<DateTime<Local>> {
    let n = normalize(expr);
    let sched = Schedule::from_str(&n).ok()?;
    sched.after(&after).next()
}

/// Did the schedule fire in the minute starting at `minute_start` (any second within [start, start+60s))?
pub fn fired_in_minute(expr: &str, minute_start: DateTime<Local>) -> bool {
    let Some(next) = next_run(expr, minute_start - chrono::Duration::seconds(1)) else {
        return false;
    };
    let end = minute_start + chrono::Duration::seconds(60);
    next >= minute_start && next < end
}

pub fn human_until(target: DateTime<Local>, now: DateTime<Local>) -> String {
    let dur = target.signed_duration_since(now);
    let total = dur.num_seconds();
    if total < 0 {
        return "past".into();
    }
    if total < 60 {
        return format!("in {}s", total);
    }
    let m = total / 60;
    if m < 60 {
        return format!("in {}m", m);
    }
    let h = m / 60;
    if h < 48 {
        return format!("in {}h{}m", h, m % 60);
    }
    let d = h / 24;
    format!("in {}d{}h", d, h % 24)
}

pub const PRESETS: &[(&str, &str)] = &[
    ("Every minute", "* * * * *"),
    ("Every 5 min", "*/5 * * * *"),
    ("Every 15 min", "*/15 * * * *"),
    ("Every hour", "0 * * * *"),
    ("Every 2 hours", "0 */2 * * *"),
    ("Daily 9am", "0 9 * * *"),
    ("Weekdays 9am", "0 9 * * 1-5"),
    ("Daily midnight", "0 0 * * *"),
    ("Weekly Mon 8am", "0 8 * * 1"),
];
