use std::collections::{BTreeMap, BTreeSet};

use serde_json::json;

use super::ingest::IngestOutcome;
use super::ingest::transcript::{Transcript, connection, replay};
use super::test_support::notification;
use super::*;

const LIVE: &str = include_str!("fixtures/children_live.jsonl");
const RECONNECT: &str = include_str!("fixtures/children_reconnect.jsonl");

struct Run {
    parent: String,
    first: String,
    late: String,
    effects: Vec<ProjectionEffect>,
    issues: Vec<ProjectionIssue>,
}

fn run(text: &str) -> anyhow::Result<Run> {
    let transcript = Transcript::parse(text)?;
    let children: Vec<String> = serde_json::from_value(
        transcript
            .header
            .get("child_thread_ids")
            .cloned()
            .unwrap_or_default(),
    )?;
    let [first, late] = children.as_slice() else {
        anyhow::bail!("expected two recorded children, got {children:?}");
    };
    let mut connection = connection(&transcript)?;
    let mut recorded = Run {
        parent: transcript.header_text("thread_id")?,
        first: first.clone(),
        late: late.clone(),
        effects: Vec::new(),
        issues: Vec::new(),
    };
    for outcome in replay(&transcript, &mut connection)? {
        if let IngestOutcome::Capture(frame) = outcome {
            let batch = decode_observation(&frame.observation, frame.intake_context());
            recorded.effects.extend(batch.effects);
            recorded.issues.extend(batch.issues);
        }
    }
    Ok(recorded)
}

fn graph(effects: &[ProjectionEffect]) -> ChildGraph {
    let mut graph = ChildGraph::default();
    for effect in effects {
        graph.apply(effect);
    }
    graph
}

fn kinds_to(graph: &ChildGraph, child: &str) -> Vec<EdgeKind> {
    graph
        .edges()
        .iter()
        .filter(|edge| edge.receiver_thread_id.as_deref() == Some(child))
        .map(|edge| edge.kind)
        .collect()
}

#[test]
fn children_live_transcript_records_every_lifecycle_edge_with_provenance() -> anyhow::Result<()> {
    let run = run(LIVE)?;
    let graph = graph(&run.effects);
    use EdgeKind::{Completion, FollowUp, Interrupt, Spawn};
    assert_eq!(
        kinds_to(&graph, &run.first),
        vec![Spawn, Completion, FollowUp, Completion, Interrupt]
    );
    assert_eq!(kinds_to(&graph, &run.late), vec![Spawn, Interrupt]);
    let waits: Vec<_> = graph
        .edges()
        .iter()
        .filter(|edge| edge.kind == EdgeKind::Wait)
        .collect();
    assert_eq!(waits.len(), 2);
    for wait in &waits {
        assert_eq!(wait.sender_thread_id, run.parent);
        assert_eq!(wait.receiver_thread_id, None);
        assert_eq!(wait.prompt, Field::Null);
        assert_eq!(wait.status, Field::Value("completed".into()));
    }
    for edge in graph.edges() {
        assert_eq!(
            edge.evidence.len(),
            2,
            "started and completed frames: {edge:?}"
        );
        assert!(
            edge.evidence
                .iter()
                .all(|evidence| evidence.origin == EvidenceOrigin::LiveFrame)
        );
        assert_eq!(edge.sender_thread_id, run.parent);
    }
    let spawn = graph
        .edges()
        .iter()
        .find(|edge| edge.kind == EdgeKind::Spawn)
        .ok_or_else(|| anyhow::anyhow!("no spawn edge"))?;
    assert_eq!(spawn.prompt, Field::Missing);
    assert_eq!(spawn.agent_path, Field::Value("/root/child".into()));
    assert!(matches!(spawn.model, Field::Value(_)));
    assert!(graph.unrecognised().is_empty());
    Ok(())
}

#[test]
fn children_live_children_stay_descendants_of_the_seat_thread() -> anyhow::Result<()> {
    let run = run(LIVE)?;
    let graph = graph(&run.effects);
    for child in [&run.first, &run.late] {
        let link = graph
            .parent(child)
            .ok_or_else(|| anyhow::anyhow!("{child} has no parent"))?;
        assert_eq!(link.parent_thread_id, run.parent);
        assert_eq!(link.evidence.origin, EvidenceOrigin::LiveFrame);
        assert_eq!(graph.owning_root(child), run.parent);
    }
    let descendants: BTreeSet<&str> = graph.descendants(&run.parent).into_iter().collect();
    assert_eq!(
        descendants,
        BTreeSet::from([run.first.as_str(), run.late.as_str()])
    );
    assert_eq!(graph.owning_root(&run.parent), run.parent);
    assert_eq!(graph.state(&run.first), Some(ChildState::Interrupted));
    assert_eq!(graph.state(&run.late), Some(ChildState::Interrupted));
    assert!(graph.running_children().is_empty());
    Ok(())
}

#[test]
fn children_live_lifecycle_transitions_follow_emitted_order() -> anyhow::Result<()> {
    let run = run(LIVE)?;
    let mut graph = ChildGraph::default();
    let mut states = Vec::new();
    for effect in &run.effects {
        graph.apply(effect);
        let state = graph.state(&run.first);
        if states.last() != Some(&state) {
            states.push(state);
        }
    }
    use ChildState::{Completed, Interrupted, Running};
    assert_eq!(
        states,
        vec![
            None,
            Some(Running),
            Some(Completed),
            Some(Running),
            Some(Completed),
            Some(Interrupted)
        ]
    );
    Ok(())
}

#[test]
fn children_reconnect_recovers_edges_parentage_and_child_history() -> anyhow::Result<()> {
    let live = run(LIVE)?;
    let run = run(RECONNECT)?;
    let graph = graph(&run.effects);
    let recovered: BTreeSet<_> = graph
        .edges()
        .iter()
        .filter(|edge| {
            edge.evidence.first().map(|evidence| evidence.origin)
                == Some(EvidenceOrigin::RpcResponse)
        })
        .map(|edge| {
            (
                edge.kind,
                edge.receiver_thread_id.clone(),
                edge.item_id.clone(),
            )
        })
        .collect();
    let original: BTreeSet<_> = self::graph(&live.effects)
        .edges()
        .iter()
        .map(|edge| {
            (
                edge.kind,
                edge.receiver_thread_id.clone(),
                edge.item_id.clone(),
            )
        })
        .collect();
    assert_eq!(recovered, original);
    use EdgeKind::{Completion, FollowUp, Interrupt, Resume, Spawn};
    assert_eq!(
        kinds_to(&graph, &run.first),
        vec![
            Spawn, Completion, FollowUp, Completion, Interrupt, Resume, Completion
        ]
    );
    for child in [&run.first, &run.late] {
        let explicit = run.effects.iter().any(|effect| {
            matches!(effect, ProjectionEffect::Lifecycle(lifecycle)
                if lifecycle.thread_id == *child
                    && lifecycle.thread.as_ref().map(|thread| &thread.parent_thread_id)
                        == Some(&Field::Value(run.parent.clone())))
        });
        assert!(explicit, "thread/read names {child}'s parent");
        assert_eq!(graph.owning_root(child), run.parent);
    }
    let history = |child: &str| {
        run.effects
            .iter()
            .filter(|effect| {
                matches!(effect, ProjectionEffect::Item(item) if item.key.thread_id == child)
            })
            .count()
    };
    assert_eq!(history(&run.first), 4);
    assert_eq!(history(&run.late), 0);
    Ok(())
}

#[test]
fn children_reconnect_refused_direct_child_input_and_unknown_child_change_nothing()
-> anyhow::Result<()> {
    let run = run(RECONNECT)?;
    let refused = run
        .issues
        .iter()
        .filter(|issue| issue.kind == IssueKind::RpcError)
        .count();
    assert_eq!(refused, 2);
    let graph = graph(&run.effects);
    assert!(graph.parent("no-such-child").is_none());
    assert_eq!(graph.descendants(&run.parent).len(), 2);
    Ok(())
}

#[test]
fn children_usage_parent_total_excludes_children_so_seat_spend_adds_them_once() -> anyhow::Result<()>
{
    let live = run(LIVE)?;
    let mut last_total = None;
    for effect in &live.effects {
        let ProjectionEffect::Usage(usage) = effect else {
            continue;
        };
        if usage.thread_id != live.parent {
            continue;
        }
        let read = |breakdown: &Field<TokenBreakdown>| {
            breakdown
                .value()
                .and_then(|breakdown| breakdown.total_tokens.value().copied())
        };
        let (Some(total), Some(last)) = (read(&usage.total), read(&usage.last)) else {
            anyhow::bail!("parent sample without totals");
        };
        assert_eq!(total - last_total.unwrap_or(0), last);
        last_total = Some(total);
    }
    let reconnect = run(RECONNECT)?;
    let effects: Vec<_> = live
        .effects
        .iter()
        .chain(&reconnect.effects)
        .cloned()
        .collect();
    let graph = graph(&effects);
    let mut ledger = UsageLedger::default();
    for effect in &effects {
        ledger.apply(effect);
    }
    let own: BTreeMap<String, i64> = ledger
        .spend()
        .into_iter()
        .map(|spend| (spend.thread_id, spend.own))
        .collect();
    assert_eq!(
        own,
        BTreeMap::from([(live.parent.clone(), 225_946), (live.first.clone(), 89_898)])
    );
    assert_eq!(
        spend_by_root(&graph, &ledger),
        BTreeMap::from([(live.parent.clone(), 225_946 + 89_898)])
    );
    Ok(())
}

fn spawned(child: &str, item: &str) -> anyhow::Result<Vec<ProjectionEffect>> {
    let event = notification(
        "item/completed",
        json!({"threadId":"seat","turnId":"u","completedAtMs":1,"item":{
            "type":"subAgentActivity","id":item,"kind":"started",
            "agentThreadId":child,"agentPath":"/root/x"
        }}),
    )?;
    Ok(decode_observation(&event, IntakeContext::new(None)).effects)
}

#[test]
fn children_concurrency_counts_seats_and_children_and_interrupts_newest_first() -> anyhow::Result<()>
{
    let mut graph = ChildGraph::default();
    for (child, item) in [("a", "i1"), ("b", "i2")] {
        for effect in spawned(child, item)? {
            graph.apply(&effect);
        }
    }
    assert_eq!(graph.running_children(), vec!["a", "b"]);
    let policy = ConcurrencyPolicy { max_active: 2 };
    let decision = policy.evaluate(1, &graph);
    assert_eq!(decision.active_children, 2);
    assert!(!decision.admits_another);
    assert_eq!(decision.interrupt, vec!["b".to_owned()]);
    assert_eq!(policy.evaluate(3, &graph).interrupt, vec!["b", "a"]);
    let roomy = ConcurrencyPolicy { max_active: 4 }.evaluate(1, &graph);
    assert!(roomy.admits_another);
    assert!(roomy.interrupt.is_empty());
    Ok(())
}

#[test]
fn children_concurrency_over_the_live_transcript_never_exceeds_one_child() -> anyhow::Result<()> {
    let run = run(LIVE)?;
    let policy = ConcurrencyPolicy { max_active: 2 };
    let mut graph = ChildGraph::default();
    let mut most = 0;
    for effect in &run.effects {
        graph.apply(effect);
        let decision = policy.evaluate(1, &graph);
        most = most.max(decision.active_children);
        assert!(decision.interrupt.is_empty());
        if decision.active_children == 1 {
            assert!(!decision.admits_another);
            assert_eq!(policy.evaluate(2, &graph).interrupt.len(), 1);
        }
    }
    assert_eq!(most, 1);
    Ok(())
}

#[test]
fn children_unknown_collaboration_tool_is_kept_as_unrecognised() -> anyhow::Result<()> {
    let event = notification(
        "item/completed",
        json!({"threadId":"seat","turnId":"u","completedAtMs":1,"item":{
            "type":"collabAgentToolCall","id":"i","tool":"teleportAgent","status":"completed",
            "senderThreadId":"seat","receiverThreadIds":["c"],"agentsStates":{}
        }}),
    )?;
    let mut graph = ChildGraph::default();
    for effect in decode_observation(&event, IntakeContext::new(None)).effects {
        graph.apply(&effect);
    }
    assert!(graph.edges().is_empty());
    assert_eq!(graph.unrecognised().len(), 1);
    Ok(())
}
