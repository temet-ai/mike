use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Duration, Utc};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq)]
pub struct TagEvent {
    pub closed_at: DateTime<Utc>,
    pub session: String,
    pub major: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct BacktestParams {
    pub min_turns: usize,
    pub min_sessions: usize,
    pub min_major: usize,
    pub window_days: i64,
}

impl Default for BacktestParams {
    fn default() -> Self {
        Self {
            min_turns: 4,
            min_sessions: 3,
            min_major: 2,
            window_days: 30,
        }
    }
}

pub fn qualifying_pairs(
    events: &BTreeMap<(String, String), Vec<TagEvent>>,
    p: &BacktestParams,
) -> BTreeMap<(String, String), DateTime<Utc>> {
    let window = Duration::days(p.window_days);
    let mut out = BTreeMap::new();
    for (key, evs) in events {
        let mut sorted = evs.clone();
        sorted.sort_by(|a, b| {
            a.closed_at
                .cmp(&b.closed_at)
                .then_with(|| a.session.cmp(&b.session))
        });
        for (i, start) in sorted.iter().enumerate() {
            let win: Vec<&TagEvent> = sorted[i..]
                .iter()
                .filter(|e| e.closed_at - start.closed_at < window)
                .collect();
            let sessions: BTreeSet<&str> = win.iter().map(|e| e.session.as_str()).collect();
            let majors = win.iter().filter(|e| e.major).count();
            if win.len() >= p.min_turns && sessions.len() >= p.min_sessions && majors >= p.min_major
            {
                out.insert(key.clone(), win[p.min_turns - 1].closed_at);
                break;
            }
        }
    }
    out
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BacktestSummary {
    pub qualifying: usize,
    pub in_no_project: usize,
    pub distinct_scopes: usize,
    pub distinct_tags: usize,
    pub tags_in_three_scopes: usize,
    pub by_tag: BTreeMap<String, usize>,
}

pub fn summarise(q: &BTreeMap<(String, String), DateTime<Utc>>) -> BacktestSummary {
    let mut by_tag: BTreeMap<String, usize> = BTreeMap::new();
    for (_, tag) in q.keys() {
        *by_tag.entry(tag.clone()).or_default() += 1;
    }
    let scopes: BTreeSet<&str> = q.keys().map(|(s, _)| s.as_str()).collect();
    BacktestSummary {
        qualifying: q.len(),
        in_no_project: q.keys().filter(|(s, _)| s == "no_project").count(),
        distinct_scopes: scopes.len(),
        distinct_tags: by_tag.len(),
        tags_in_three_scopes: by_tag.values().filter(|c| **c >= 3).count(),
        by_tag,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(day: i64, session: &str, major: bool) -> TagEvent {
        TagEvent {
            closed_at: DateTime::<Utc>::from_timestamp(1_790_000_000 + day * 86_400, 0)
                .unwrap_or_default(),
            session: session.to_owned(),
            major,
        }
    }

    #[test]
    fn qualifying_pairs_reproduces_the_reference_rule() {
        let mut events = BTreeMap::new();
        events.insert(
            ("project:/a".to_owned(), "unverified-claim".to_owned()),
            vec![
                ev(0, "s1", true),
                ev(1, "s2", false),
                ev(2, "s3", true),
                ev(3, "s1", false),
            ],
        );
        events.insert(
            ("project:/a".to_owned(), "wrong-file".to_owned()),
            vec![
                ev(0, "s1", true),
                ev(1, "s1", true),
                ev(2, "s1", true),
                ev(3, "s1", true),
            ],
        );
        events.insert(
            ("no_project".to_owned(), "stall".to_owned()),
            vec![
                ev(0, "s1", true),
                ev(10, "s2", true),
                ev(45, "s3", true),
                ev(46, "s4", true),
            ],
        );
        let q = qualifying_pairs(&events, &BacktestParams::default());
        assert_eq!(q.len(), 1);
        assert!(q.contains_key(&("project:/a".to_owned(), "unverified-claim".to_owned())));
        let s = summarise(&q);
        assert_eq!((s.qualifying, s.in_no_project, s.distinct_tags), (1, 0, 1));
    }
}
