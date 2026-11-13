use chrono::{DateTime, Days, NaiveDate, Utc};

const DATE_SHAPE: &str = "dddd-dd-dd";

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SearchWindow {
    pub since: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Edge {
    Since,
    Until,
}

impl Edge {
    fn name(self) -> &'static str {
        match self {
            Edge::Since => "since",
            Edge::Until => "until",
        }
    }
}

impl SearchWindow {
    pub fn parse(since: Option<&str>, until: Option<&str>) -> Result<Self, String> {
        let window = Self {
            since: bound(Edge::Since, since)?,
            until: bound(Edge::Until, until)?,
        };
        match (window.since, window.until) {
            (Some(from), Some(to)) if from >= to => Err(format!(
                "since ({}) must be before until ({}); a date in until counts the whole day",
                since.unwrap_or_default().trim(),
                until.unwrap_or_default().trim()
            )),
            _ => Ok(window),
        }
    }
}

fn bound(edge: Edge, raw: Option<&str>) -> Result<Option<DateTime<Utc>>, String> {
    let Some(text) = raw.map(str::trim).filter(|text| !text.is_empty()) else {
        return Ok(None);
    };
    if let Ok(moment) = DateTime::parse_from_rfc3339(text) {
        return Ok(Some(moment.with_timezone(&Utc)));
    }
    let day = NaiveDate::parse_from_str(text, "%Y-%m-%d").map_err(|_| refusal(edge, text))?;
    let day = match edge {
        Edge::Since => Some(day),
        Edge::Until => day.checked_add_days(Days::new(1)),
    };
    Ok(day
        .and_then(|day| day.and_hms_opt(0, 0, 0))
        .map(|start| start.and_utc()))
}

fn refusal(edge: Edge, text: &str) -> String {
    format!(
        "{} {text:?} is not a date: character {} does not fit; give YYYY-MM-DD (UTC, whole day) or an RFC 3339 time such as 2026-11-04T09:00:00Z",
        edge.name(),
        first_misfit(text) + 1
    )
}

fn first_misfit(text: &str) -> usize {
    text.chars()
        .zip(DATE_SHAPE.chars())
        .position(|(given, shape)| match shape {
            'd' => !given.is_ascii_digit(),
            other => given != other,
        })
        .unwrap_or_else(|| text.chars().count().min(DATE_SHAPE.len()))
}

#[cfg(test)]
#[path = "window_tests.rs"]
pub mod window_tests;
