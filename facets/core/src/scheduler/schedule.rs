//! When recurring work is due: cron expressions, fixed intervals and retry backoff.

use chrono::{DateTime, Utc};
use croner::Cron;

/// Shortest interval a task may repeat at. Anything faster belongs in a plugin's own loop.
pub const MIN_EVERY_SECS: i64 = 10;
/// Longest wait before a failed job is tried again.
pub const MAX_BACKOFF_SECS: i64 = 6 * 3600;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ScheduleError {
    #[error("`{0}` is not a cron expression (five fields: minute hour day month weekday)")]
    Cron(String),
    #[error("`{0}` is not an interval; write it like 30s, 15m, 6h or 1d")]
    Every(String),
    #[error("an interval must be at least {MIN_EVERY_SECS} seconds")]
    TooShort,
    #[error("`{0}` is not a time zone name such as Africa/Lagos or UTC")]
    TimeZone(String),
    #[error("give either `cron` or `every`, not both and not neither")]
    NeedOne,
}

/// How often a task repeats.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Recurrence {
    Cron { expression: String, timezone: Option<String> },
    Every { seconds: i64 },
}

impl Recurrence {
    /// Build from the fields a manifest or a plugin gives, checking them.
    pub fn parse(cron: Option<&str>, every: Option<&str>, timezone: Option<&str>) -> Result<Self, ScheduleError> {
        match (cron.map(str::trim).filter(|c| !c.is_empty()), every.map(str::trim).filter(|e| !e.is_empty())) {
            (Some(expression), None) => {
                Cron::new(expression).parse().map_err(|_| ScheduleError::Cron(expression.to_string()))?;
                if let Some(zone) = timezone {
                    zone.parse::<chrono_tz::Tz>().map_err(|_| ScheduleError::TimeZone(zone.to_string()))?;
                }
                Ok(Self::Cron { expression: expression.to_string(), timezone: timezone.map(str::to_string) })
            }
            (None, Some(every)) => Ok(Self::Every { seconds: parse_interval(every)? }),
            _ => Err(ScheduleError::NeedOne),
        }
    }

    /// The first run strictly after `after`.
    pub fn next_after(&self, after: DateTime<Utc>) -> Option<DateTime<Utc>> {
        match self {
            Self::Every { seconds } => Some(after + chrono::Duration::seconds(*seconds)),
            Self::Cron { expression, timezone } => {
                let cron = Cron::new(expression).parse().ok()?;
                match timezone.as_deref().and_then(|zone| zone.parse::<chrono_tz::Tz>().ok()) {
                    Some(zone) => cron
                        .find_next_occurrence(&after.with_timezone(&zone), false)
                        .ok()
                        .map(|next| next.with_timezone(&Utc)),
                    None => cron.find_next_occurrence(&after, false).ok(),
                }
            }
        }
    }

    /// Seconds between runs for an interval, or an estimate for cron, used to decide how late
    /// a run may start before it counts as missed.
    pub fn period_hint_secs(&self, from: DateTime<Utc>) -> i64 {
        match self {
            Self::Every { seconds } => *seconds,
            Self::Cron { .. } => match (self.next_after(from), self.next_after(from).and_then(|n| self.next_after(n))) {
                (Some(first), Some(second)) => (second - first).num_seconds().max(MIN_EVERY_SECS),
                _ => 3600,
            },
        }
    }
}

/// `30s`, `15m`, `6h`, `1d` as seconds.
pub fn parse_interval(text: &str) -> Result<i64, ScheduleError> {
    let text = text.trim();
    let bad = || ScheduleError::Every(text.to_string());
    let (digits, unit) = text.split_at(text.find(|c: char| !c.is_ascii_digit()).ok_or_else(bad)?);
    let amount: i64 = digits.parse().map_err(|_| bad())?;
    let seconds = match unit {
        "s" => amount,
        "m" => amount.checked_mul(60).ok_or_else(bad)?,
        "h" => amount.checked_mul(3600).ok_or_else(bad)?,
        "d" => amount.checked_mul(86_400).ok_or_else(bad)?,
        _ => return Err(bad()),
    };
    if seconds < MIN_EVERY_SECS {
        return Err(ScheduleError::TooShort);
    }
    Ok(seconds)
}

/// How long to wait before attempt number `attempt + 1`, after `attempt` attempts have failed:
/// `base` doubling each time, capped at [`MAX_BACKOFF_SECS`].
pub fn backoff_secs(base: i64, attempt: i64) -> i64 {
    let doublings = (attempt - 1).clamp(0, 20) as u32;
    base.max(1).saturating_mul(1_i64 << doublings).min(MAX_BACKOFF_SECS)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(text).unwrap().with_timezone(&Utc)
    }

    #[test]
    fn intervals_are_parsed_and_bounded() {
        assert_eq!(parse_interval("30s"), Ok(30));
        assert_eq!(parse_interval("15m"), Ok(900));
        assert_eq!(parse_interval("6h"), Ok(21_600));
        assert_eq!(parse_interval("1d"), Ok(86_400));
        assert_eq!(parse_interval("5s"), Err(ScheduleError::TooShort));
        for bad in ["", "m", "10", "10x", "-5m", "1.5h", "99999999999999999999d"] {
            assert!(parse_interval(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn exactly_one_of_cron_and_every() {
        assert_eq!(Recurrence::parse(None, None, None), Err(ScheduleError::NeedOne));
        assert_eq!(Recurrence::parse(Some("* * * * *"), Some("5m"), None), Err(ScheduleError::NeedOne));
        assert!(Recurrence::parse(Some("not cron"), None, None).is_err());
        assert!(matches!(Recurrence::parse(Some("0 9 * * *"), None, Some("Nowhere/City")), Err(ScheduleError::TimeZone(_))));
    }

    #[test]
    fn cron_finds_the_next_slot_in_utc_and_in_a_zone() {
        let daily = Recurrence::parse(Some("0 9 * * *"), None, None).unwrap();
        assert_eq!(daily.next_after(at("2026-10-05T08:00:00Z")), Some(at("2026-10-05T09:00:00Z")));
        assert_eq!(daily.next_after(at("2026-10-05T09:00:00Z")), Some(at("2026-10-06T09:00:00Z")), "strictly after");
        // 09:00 in Lagos (UTC+1) is 08:00 UTC.
        let lagos = Recurrence::parse(Some("0 9 * * *"), None, Some("Africa/Lagos")).unwrap();
        assert_eq!(lagos.next_after(at("2026-10-05T00:00:00Z")), Some(at("2026-10-05T08:00:00Z")));
    }

    #[test]
    fn an_interval_counts_from_the_given_moment() {
        let every = Recurrence::parse(None, Some("15m"), None).unwrap();
        assert_eq!(every.next_after(at("2026-10-05T10:00:00Z")), Some(at("2026-10-05T10:15:00Z")));
        assert_eq!(every.period_hint_secs(at("2026-10-05T10:00:00Z")), 900);
        let hourly = Recurrence::parse(Some("0 * * * *"), None, None).unwrap();
        assert_eq!(hourly.period_hint_secs(at("2026-10-05T10:30:00Z")), 3600);
    }

    #[test]
    fn backoff_doubles_and_is_capped() {
        assert_eq!(backoff_secs(30, 1), 30);
        assert_eq!(backoff_secs(30, 2), 60);
        assert_eq!(backoff_secs(30, 3), 120);
        assert_eq!(backoff_secs(30, 40), MAX_BACKOFF_SECS);
        assert_eq!(backoff_secs(0, 1), 1);
    }
}
