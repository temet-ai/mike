use chrono::{DateTime, Utc};

use crate::dossier::steering_facts::observation::Observation;
use crate::dossier::steering_facts::wait_backing::BackingRef;
use crate::dossier::steering_facts::{ClassFact, SteeringFacts};

fn at(t: Option<DateTime<Utc>>) -> String {
    t.map_or_else(|| "?".to_owned(), |t| t.format("%H:%M:%S").to_string())
}

fn backing_ref(r: &BackingRef) -> String {
    let mut parts = vec![r.source.clone()];
    if let Some(id) = r.id {
        parts.push(id.to_string());
    }
    parts.extend(
        [&r.kind, &r.state, &r.peer_session_uuid, &r.message_uuid]
            .into_iter()
            .flatten()
            .cloned(),
    );
    if let Some(directive) = r.directive_id {
        parts.push(format!("directive {directive}"));
    }
    parts.join(" ")
}

fn wait_line(facts: &SteeringFacts) -> String {
    let evidence = if facts.wait_backing_refs.is_empty() {
        "no running work item, scheduled wake, live monitor, awaited peer reply or operator request found".to_owned()
    } else {
        facts
            .wait_backing_refs
            .iter()
            .map(backing_ref)
            .collect::<Vec<_>>()
            .join("; ")
    };
    format!("- wait_backing: {} ({evidence})", facts.wait_backing.code())
}

fn observation_line(o: &Observation) -> String {
    match o.observed_after_change {
        None => "- observed_after_change: not applicable (no mutating call in the turn)".to_owned(),
        Some(true) => format!(
            "- observed_after_change: true (last change tool {} at {}; {} call {} at {} after it)",
            o.last_change_use_id.unwrap_or(0),
            at(o.last_change_at),
            o.observation_kind.unwrap_or("?"),
            o.last_observation_use_id.unwrap_or(0),
            at(o.last_observation_at)
        ),
        Some(false) => format!(
            "- observed_after_change: false (last change tool {} at {}; no test, build, lint, run, http, browser or render call after it)",
            o.last_change_use_id.unwrap_or(0),
            at(o.last_change_at)
        ),
    }
}

fn class_line(class: Option<&ClassFact>) -> String {
    let Some(c) = class else {
        return "- session_class: unclassified (no class inputs as of the moment)".to_owned();
    };
    let mut evidence = vec![format!("rule {}", c.rule)];
    if let Some(launch) = c.launch_id {
        evidence.push(format!("launch {launch}"));
    }
    if let Some(run) = &c.run_id {
        evidence.push(format!("run {run}"));
    }
    if let Some(seat) = &c.seat {
        evidence.push(format!("seat {seat}"));
    }
    if c.fork {
        evidence.push("fork".to_owned());
    }
    format!("- session_class: {} ({})", c.class, evidence.join("; "))
}

pub fn fact_lines(facts: Option<&SteeringFacts>) -> Vec<String> {
    let Some(facts) = facts else {
        return Vec::new();
    };
    vec![
        wait_line(facts),
        observation_line(&facts.observation),
        class_line(facts.session_class.as_ref()),
    ]
}

#[cfg(test)]
pub mod tests {
    use chrono::Utc;

    use super::*;
    use crate::dossier::steering_facts::STEERING_FACTS_VERSION;
    use crate::dossier::steering_facts::wait_backing::WaitBacking;

    fn facts(observed: Option<bool>) -> SteeringFacts {
        SteeringFacts {
            wait_backing: WaitBacking::None,
            wait_backing_refs: Vec::new(),
            observation: Observation {
                observed_after_change: observed,
                last_change_use_id: observed.map(|_| 88213),
                ..Observation::default()
            },
            session_class: None,
            as_of: Utc::now(),
            version: STEERING_FACTS_VERSION,
        }
    }

    #[test]
    fn no_facts_render_no_lines() {
        assert!(fact_lines(None).is_empty());
    }

    #[test]
    fn unobserved_change_names_the_last_change_tool() {
        let lines = fact_lines(Some(&facts(Some(false))));
        assert_eq!(lines.len(), 3);
        assert!(lines[0].starts_with("- wait_backing: none ("));
        assert!(
            lines[1].contains("false (last change tool 88213"),
            "{}",
            lines[1]
        );
    }
}
