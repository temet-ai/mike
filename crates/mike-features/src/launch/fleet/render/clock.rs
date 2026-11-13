use chrono::{DateTime, Utc};

const AGO_SECONDS_UNTIL_MINUTES: i64 = 90;
const AGO_SECONDS_UNTIL_HOURS: i64 = 5400;

pub fn ago(moment: Option<DateTime<Utc>>, now: DateTime<Utc>) -> String {
    let Some(moment) = moment else {
        return "never".to_string();
    };
    let seconds = (now - moment).num_seconds().max(0);
    if seconds < AGO_SECONDS_UNTIL_MINUTES {
        format!("{seconds}s ago")
    } else if seconds < AGO_SECONDS_UNTIL_HOURS {
        format!("{}m ago", seconds / 60)
    } else {
        format!("{} ago", hours_and_minutes(seconds))
    }
}

pub fn elapsed(since: DateTime<Utc>, now: DateTime<Utc>) -> String {
    hours_and_minutes((now - since).num_seconds().max(0))
}

pub fn day_and_time(moment: DateTime<Utc>) -> String {
    moment.format("%d %b %H:%M UTC").to_string()
}

pub fn hour_and_minute(moment: DateTime<Utc>) -> String {
    moment.format("%H:%M").to_string()
}

fn hours_and_minutes(seconds: i64) -> String {
    format!("{}h{:02}m", seconds / 3600, (seconds % 3600) / 60)
}
