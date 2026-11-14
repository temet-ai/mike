use chrono::{DateTime, Duration, Utc};
use serde::Serialize;

use super::Question;
use super::exempt::{FLEET_SEAT_CLASS, fleet_seat_released};
use super::holdback::{Arm, ArmAnchor, assign_arm};
use super::policy::{GuardSettings, QuestionMode, QuestionPolicy};
use crate::steerer::note_policy::NoteRefusal;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "plan", rename_all = "snake_case")]
pub enum DeliveryPlan {
    Append,
    Wake { deliver_after: DateTime<Utc> },
    CappedCorrection,
}

impl DeliveryPlan {
    pub fn channel(self) -> Option<&'static str> {
        match self {
            Self::Append | Self::CappedCorrection => Some("append"),
            Self::Wake { .. } => Some("wake"),
        }
    }

    pub fn wake_state(self) -> Option<&'static str> {
        match self {
            Self::Wake { .. } => Some("pending"),
            Self::CappedCorrection => Some("capped_correction"),
            _ => None,
        }
    }

    pub fn deliver_after(self) -> Option<DateTime<Utc>> {
        match self {
            Self::Wake { deliver_after } => Some(deliver_after),
            _ => None,
        }
    }

    pub fn enqueues(self) -> bool {
        matches!(self, Self::Append)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum LiveDecision {
    ShadowOnly,
    NotDelivered {
        superseded_by: Question,
    },
    Holdback {
        draw: f64,
        anchor_turn_id: i64,
        due_at: Option<DateTime<Utc>>,
    },
    Deliver {
        draw: f64,
        anchor_turn_id: i64,
        plan: DeliveryPlan,
    },
    Exempt {
        reason: &'static str,
    },
    Held {
        draw: f64,
        anchor_turn_id: i64,
    },
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct AnchorHistory {
    pub treated_pending_or_delivered: usize,
    pub same_change_fires: usize,
}

pub struct LiveContext<'a> {
    pub question: Question,
    pub steerer_id: i64,
    pub anchor_turn_id: i64,
    pub policy: &'a QuestionPolicy,
    pub session_class: &'a str,
    pub note_refusal: Option<NoteRefusal>,
    pub higher_live: Option<Question>,
    pub prior: &'a AnchorHistory,
    pub turn_end_at: DateTime<Utc>,
    pub now: DateTime<Utc>,
}

fn due(ctx: &LiveContext<'_>, settings: &GuardSettings) -> DateTime<Utc> {
    (ctx.turn_end_at + Duration::seconds(settings.grace_secs(ctx.question))).max(ctx.now)
}

fn repeat_plan(ctx: &LiveContext<'_>, planned: DeliveryPlan) -> DeliveryPlan {
    if ctx.question == Question::UnobservedDone && ctx.prior.same_change_fires > 0 {
        return DeliveryPlan::CappedCorrection;
    }
    match (ctx.question, ctx.prior.treated_pending_or_delivered) {
        (_, 0) => planned,
        (Question::Relapse, 1) => DeliveryPlan::Append,
        _ => DeliveryPlan::CappedCorrection,
    }
}

pub fn decide_live_fire(ctx: &LiveContext<'_>, settings: &GuardSettings) -> LiveDecision {
    if !ctx.policy.mode.is_live() {
        return LiveDecision::ShadowOnly;
    }
    let fleet_seat = ctx.session_class == FLEET_SEAT_CLASS;
    let released = fleet_seat && fleet_seat_released(ctx.question, settings.fleet_seats);
    if ctx.session_class != "operator" && !released {
        return LiveDecision::Exempt {
            reason: "session_class",
        };
    }
    if let Some(refusal) = ctx.note_refusal {
        return LiveDecision::Exempt {
            reason: refusal.code(),
        };
    }
    if let Some(higher) = ctx.higher_live.filter(|h| *h != ctx.question) {
        return LiveDecision::NotDelivered {
            superseded_by: higher,
        };
    }
    let anchor = ArmAnchor {
        steerer_id: ctx.steerer_id,
        question: ctx.question,
        anchor_turn_id: ctx.anchor_turn_id,
    };
    let pct = u8::try_from(ctx.policy.effective_holdback(settings)).unwrap_or(50);
    let wake = ctx.policy.mode == QuestionMode::Wake && !fleet_seat;
    match assign_arm(ctx.policy.holdback_salt, &anchor, pct) {
        Arm::Holdback { draw } => LiveDecision::Holdback {
            draw,
            anchor_turn_id: ctx.anchor_turn_id,
            due_at: wake.then(|| due(ctx, settings)),
        },
        Arm::Treated { draw } => {
            let planned = if wake {
                DeliveryPlan::Wake {
                    deliver_after: due(ctx, settings),
                }
            } else {
                DeliveryPlan::Append
            };
            LiveDecision::Deliver {
                draw,
                anchor_turn_id: ctx.anchor_turn_id,
                plan: repeat_plan(ctx, planned),
            }
        }
    }
}

pub fn highest_live(firing_live: &[Question]) -> Option<Question> {
    firing_live.iter().copied().min_by_key(|q| q.rank())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_an_append_plan_queues_its_note_at_once() {
        let wake = DeliveryPlan::Wake {
            deliver_after: Utc::now(),
        };
        assert!(DeliveryPlan::Append.enqueues());
        assert!(!wake.enqueues());
        assert!(!DeliveryPlan::CappedCorrection.enqueues());
    }

    fn policy(mode: QuestionMode, pct: Option<i64>) -> QuestionPolicy {
        QuestionPolicy {
            policy_id: 1,
            mode,
            holdback_pct: pct,
            holdback_salt: uuid::Uuid::from_u128(3),
        }
    }

    fn ctx<'a>(
        q: Question,
        p: &'a QuestionPolicy,
        prior: &'a AnchorHistory,
        now: DateTime<Utc>,
    ) -> LiveContext<'a> {
        LiveContext {
            question: q,
            steerer_id: 17,
            anchor_turn_id: 90,
            policy: p,
            session_class: "operator",
            note_refusal: None,
            higher_live: None,
            prior,
            turn_end_at: now,
            now,
        }
    }

    #[test]
    fn decide_live_fire_shadow_mode_is_shadow_only() {
        let p = policy(QuestionMode::Shadow, Some(0));
        let now = Utc::now();
        assert_eq!(
            decide_live_fire(
                &ctx(Question::Relapse, &p, &AnchorHistory::default(), now),
                &GuardSettings::default()
            ),
            LiveDecision::ShadowOnly
        );
    }

    #[test]
    fn decide_live_fire_wake_waits_the_grace() {
        let p = policy(QuestionMode::Wake, Some(0));
        let now = Utc::now();
        let prior = AnchorHistory::default();
        let relapse = decide_live_fire(
            &ctx(Question::Relapse, &p, &prior, now),
            &GuardSettings::default(),
        );
        let LiveDecision::Deliver {
            plan: DeliveryPlan::Wake { deliver_after },
            ..
        } = relapse
        else {
            panic!("expected a wake, got {relapse:?}");
        };
        assert_eq!(deliver_after, now + Duration::seconds(30));
        let wait = decide_live_fire(
            &ctx(Question::UnbackedWait, &p, &prior, now),
            &GuardSettings::default(),
        );
        let LiveDecision::Deliver {
            plan: DeliveryPlan::Wake { deliver_after },
            ..
        } = wait
        else {
            panic!("expected a wake, got {wait:?}");
        };
        assert_eq!(deliver_after, now + Duration::seconds(90));
    }

    #[test]
    fn decide_live_fire_one_wake_per_correction() {
        let p = policy(QuestionMode::Wake, Some(0));
        let now = Utc::now();
        let once = AnchorHistory {
            treated_pending_or_delivered: 1,
            ..AnchorHistory::default()
        };
        let twice = AnchorHistory {
            treated_pending_or_delivered: 2,
            ..AnchorHistory::default()
        };
        let second = decide_live_fire(
            &ctx(Question::Relapse, &p, &once, now),
            &GuardSettings::default(),
        );
        assert!(matches!(
            second,
            LiveDecision::Deliver {
                plan: DeliveryPlan::Append,
                ..
            }
        ));
        let third = decide_live_fire(
            &ctx(Question::Relapse, &p, &twice, now),
            &GuardSettings::default(),
        );
        assert!(matches!(
            third,
            LiveDecision::Deliver {
                plan: DeliveryPlan::CappedCorrection,
                ..
            }
        ));
    }

    #[test]
    fn decide_live_fire_one_note_per_turn_for_other_questions() {
        let p = policy(QuestionMode::Append, Some(0));
        let now = Utc::now();
        let none = AnchorHistory::default();
        let once = AnchorHistory {
            treated_pending_or_delivered: 1,
            ..AnchorHistory::default()
        };
        let first = decide_live_fire(
            &ctx(Question::UnobservedDone, &p, &none, now),
            &GuardSettings::default(),
        );
        assert!(matches!(
            first,
            LiveDecision::Deliver {
                plan: DeliveryPlan::Append,
                ..
            }
        ));
        let repeat = decide_live_fire(
            &ctx(Question::UnobservedDone, &p, &once, now),
            &GuardSettings::default(),
        );
        assert!(matches!(
            repeat,
            LiveDecision::Deliver {
                plan: DeliveryPlan::CappedCorrection,
                ..
            }
        ));
    }

    #[test]
    fn unobserved_done_speaks_once_per_change_even_on_a_new_turn() {
        let p = policy(QuestionMode::Append, Some(0));
        let now = Utc::now();
        let answered = AnchorHistory {
            treated_pending_or_delivered: 0,
            same_change_fires: 1,
        };
        let repeat = decide_live_fire(
            &ctx(Question::UnobservedDone, &p, &answered, now),
            &GuardSettings::default(),
        );
        assert!(matches!(
            repeat,
            LiveDecision::Deliver {
                plan: DeliveryPlan::CappedCorrection,
                ..
            }
        ));
        let other = decide_live_fire(
            &ctx(Question::UnbackedWait, &p, &answered, now),
            &GuardSettings::default(),
        );
        assert!(matches!(
            other,
            LiveDecision::Deliver {
                plan: DeliveryPlan::Append,
                ..
            }
        ));
    }

    #[test]
    fn a_fleet_seat_done_claim_is_appended_only_when_the_setting_is_on() {
        let p = policy(QuestionMode::Wake, Some(0));
        let now = Utc::now();
        let prior = AnchorHistory::default();
        let mut c = ctx(Question::UnobservedDone, &p, &prior, now);
        c.session_class = "fleet_seat";
        let on = GuardSettings {
            fleet_seats: true,
            ..GuardSettings::default()
        };
        assert!(matches!(
            decide_live_fire(&c, &on),
            LiveDecision::Deliver {
                plan: DeliveryPlan::Append,
                ..
            }
        ));
        assert!(matches!(
            decide_live_fire(&c, &GuardSettings::default()),
            LiveDecision::Exempt { .. }
        ));
        c.question = Question::Relapse;
        assert!(matches!(
            decide_live_fire(&c, &on),
            LiveDecision::Exempt { .. }
        ));
    }

    #[test]
    fn decide_live_fire_supersession_and_exemption() {
        let p = policy(QuestionMode::Append, Some(0));
        let now = Utc::now();
        let prior = AnchorHistory::default();
        let mut c = ctx(Question::UnobservedDone, &p, &prior, now);
        c.higher_live = Some(Question::Relapse);
        assert_eq!(
            decide_live_fire(&c, &GuardSettings::default()),
            LiveDecision::NotDelivered {
                superseded_by: Question::Relapse
            }
        );
        c.higher_live = None;
        c.session_class = "template";
        assert!(matches!(
            decide_live_fire(&c, &GuardSettings::default()),
            LiveDecision::Exempt { .. }
        ));
    }

    #[test]
    fn a_note_the_session_rule_refuses_is_judged_but_not_sent_and_joins_no_arm() {
        let p = policy(QuestionMode::Append, Some(50));
        let now = Utc::now();
        let prior = AnchorHistory::default();
        for refusal in [
            NoteRefusal::SessionNoteLimit,
            NoteRefusal::QuietAfterWrongNote,
            NoteRefusal::NeedBelowLevel,
        ] {
            let mut c = ctx(Question::AuthorisedHandback, &p, &prior, now);
            c.note_refusal = Some(refusal);
            assert_eq!(
                decide_live_fire(&c, &GuardSettings::default()),
                LiveDecision::Exempt {
                    reason: refusal.code()
                }
            );
        }
        let in_shadow = policy(QuestionMode::Shadow, Some(0));
        let mut shadow = ctx(Question::AuthorisedHandback, &in_shadow, &prior, now);
        shadow.note_refusal = Some(NoteRefusal::SessionNoteLimit);
        assert_eq!(
            decide_live_fire(&shadow, &GuardSettings::default()),
            LiveDecision::ShadowOnly
        );
    }

    #[test]
    fn decide_live_fire_full_holdback_withholds() {
        let p = policy(QuestionMode::Wake, Some(50));
        let now = Utc::now();
        let prior = AnchorHistory::default();
        let held = (0..200)
            .filter(|t| {
                let mut c = ctx(Question::UnbackedWait, &p, &prior, now);
                c.anchor_turn_id = *t;
                matches!(
                    decide_live_fire(&c, &GuardSettings::default()),
                    LiveDecision::Holdback {
                        due_at: Some(_),
                        ..
                    }
                )
            })
            .count();
        assert!(held > 60 && held < 140, "{held}");
    }
}
