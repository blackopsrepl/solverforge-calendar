use anyhow::{Context, Result};
use chrono::{DateTime, Datelike, NaiveDate, Utc};
use rrule::RRuleSet;

/* Expand a recurring event's occurrences within a date range. */
/* /* `rrule_str` — the RFC 5545 RRULE string, e.g. `"RRULE:FREQ=WEEKLY;BYDAY=MO,WE,FR"` */ */
/* `dtstart`   — the event's start datetime (UTC) */
/* `from`      — range start (inclusive) */
/* `to`        — range end (inclusive) */
/* `limit`     — max occurrences to return (safety cap) */
pub fn expand_occurrences(
    rrule_str: &str,
    dtstart: DateTime<Utc>,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    limit: usize,
) -> Result<Vec<DateTime<Utc>>> {
    // Build the full iCal-style string the rrule crate expects
    let dtstart_str = dtstart.format("%Y%m%dT%H%M%SZ").to_string();
    let full = format!("DTSTART:{}\n{}", dtstart_str, rrule_str);

    let set: RRuleSet = full
        .parse::<RRuleSet>()
        .map_err(|e| anyhow::anyhow!("invalid RRULE '{}': {}", rrule_str, e))?;

    // rrule crate uses chrono_tz internally; we collect UTC occurrences
    let occurrences: Vec<DateTime<Utc>> = set
        .into_iter()
        .take(limit * 2) // over-fetch so we can filter by range
        .filter(|dt| {
            let utc = dt.with_timezone(&Utc);
            utc >= from && utc <= to
        })
        .take(limit)
        .map(|dt| dt.with_timezone(&Utc))
        .collect();

    Ok(occurrences)
}

/* Parse an RRULE string into a human-readable description. */
pub fn rrule_description(rrule_str: &str) -> String {
    // Simple parsing of common patterns
    if rrule_str.contains("FREQ=DAILY") {
        let interval = extract_param(rrule_str, "INTERVAL").unwrap_or(1);
        if interval == 1 {
            "Every day".to_string()
        } else {
            format!("Every {} days", interval)
        }
    } else if rrule_str.contains("FREQ=WEEKLY") {
        let interval = extract_param(rrule_str, "INTERVAL").unwrap_or(1);
        let days = extract_str_param(rrule_str, "BYDAY");
        let day_names = days.as_deref().map(format_days).unwrap_or_default();
        if interval == 1 {
            if day_names.is_empty() {
                "Every week".to_string()
            } else {
                format!("Every week on {}", day_names)
            }
        } else {
            format!("Every {} weeks", interval)
        }
    } else if rrule_str.contains("FREQ=MONTHLY") {
        let interval = extract_param(rrule_str, "INTERVAL").unwrap_or(1);
        if interval == 1 {
            "Every month".to_string()
        } else {
            format!("Every {} months", interval)
        }
    } else if rrule_str.contains("FREQ=YEARLY") {
        "Every year".to_string()
    } else {
        rrule_str.to_string()
    }
}

fn extract_param(rrule: &str, key: &str) -> Option<u32> {
    let prefix = format!("{}=", key);
    rrule
        .split(';')
        .find_map(|part| part.strip_prefix(&prefix)?.split(';').next()?.parse().ok())
}

fn extract_str_param(rrule: &str, key: &str) -> Option<String> {
    let prefix = format!("{}=", key);
    rrule
        .split(';')
        .find_map(|part| Some(part.strip_prefix(&prefix)?.split(';').next()?.to_string()))
}

fn format_days(days: &str) -> String {
    days.split(',')
        .map(|d| match d.trim() {
            "MO" => "Mon",
            "TU" => "Tue",
            "WE" => "Wed",
            "TH" => "Thu",
            "FR" => "Fri",
            "SA" => "Sat",
            "SU" => "Sun",
            other => other,
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/* Standard recurrence presets for the event form UI. */
#[derive(Debug, Clone, PartialEq)]
pub enum RecurrencePreset {
    None,
    Daily,
    WeeklyOnDay, // weekly on the same weekday as start
    Weekdays,    // Mon-Fri
    Weekly,
    BiWeekly,
    Monthly,
    Yearly,
    Custom(String), // raw RRULE
}

impl RecurrencePreset {
    pub fn to_rrule(&self, start: Option<DateTime<Utc>>) -> Option<String> {
        match self {
            Self::None => None,
            Self::Daily => Some("RRULE:FREQ=DAILY".to_string()),
            Self::WeeklyOnDay => {
                if let Some(dt) = start {
                    let days = ["MO", "TU", "WE", "TH", "FR", "SA", "SU"];
                    // Use Datelike trait to access weekday, then Weekday::num_days_from_monday
                    let dow =
                        chrono::Datelike::weekday(&dt.date_naive()).num_days_from_monday() as usize;
                    let day = days.get(dow % 7).copied().unwrap_or("MO");
                    Some(format!("RRULE:FREQ=WEEKLY;BYDAY={}", day))
                } else {
                    Some("RRULE:FREQ=WEEKLY".to_string())
                }
            }
            Self::Weekdays => Some("RRULE:FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR".to_string()),
            Self::Weekly => Some("RRULE:FREQ=WEEKLY".to_string()),
            Self::BiWeekly => Some("RRULE:FREQ=WEEKLY;INTERVAL=2".to_string()),
            Self::Monthly => Some("RRULE:FREQ=MONTHLY".to_string()),
            Self::Yearly => Some("RRULE:FREQ=YEARLY".to_string()),
            Self::Custom(s) => Some(s.clone()),
        }
    }

    pub fn label(&self) -> &str {
        match self {
            Self::None => "Does not repeat",
            Self::Daily => "Every day",
            Self::WeeklyOnDay => "Every week (same day)",
            Self::Weekdays => "Every weekday (Mon-Fri)",
            Self::Weekly => "Every week",
            Self::BiWeekly => "Every 2 weeks",
            Self::Monthly => "Every month",
            Self::Yearly => "Every year",
            Self::Custom(_) => "Custom…",
        }
    }

    pub fn all() -> Vec<Self> {
        vec![
            Self::None,
            Self::Daily,
            Self::WeeklyOnDay,
            Self::Weekdays,
            Self::Weekly,
            Self::BiWeekly,
            Self::Monthly,
            Self::Yearly,
            Self::Custom(String::new()),
        ]
    }
}
