use super::answer::{
    JudgeAnswer, Member, MemberRole, PullKind, PullRole, Pull, Relation, Universe, validate,
};

fn member(id: i64, role: MemberRole, by: Option<i64>) -> Member {
    Member {
        id,
        role,
        by,
        note: String::new(),
        evidence_for: None,
    }
}

fn pull(id: i64, role: PullRole, by: i64) -> Pull {
    Pull {
        id,
        role,
        by: Some(by),
        note: String::new(),
        evidence_for: None,
    }
}

fn answer(members: Vec<Member>, pull_in: Vec<Pull>) -> JudgeAnswer {
    JudgeAnswer {
        subject: " Topic ".to_owned(),
        members,
        pull_in,
        text: " Text. ".to_owned(),
        evidence_used: Vec::new(),
        merges: Vec::new(),
        outdated: Vec::new(),
        tighten: Vec::new(),
    }
}

fn universe<'a>(members: &'a [i64], nearby: &'a [i64], evidence: &'a [i64]) -> Universe<'a> {
    Universe {
        members,
        nearby,
        evidence,
    }
}

fn relation(validated: &super::answer::Validated, id: i64) -> Option<Relation> {
    validated.members.iter().find(|m| m.id == id).map(|m| m.relation)
}

#[test]
fn unknown_ids_are_dropped_and_missing_members_default_to_keep() {
    let a = answer(
        vec![member(1, MemberRole::Keep, None), member(99, MemberRole::Keep, None)],
        vec![pull(77, PullRole::Adds, 1)],
    );
    let v = validate(&a, &universe(&[1, 2], &[10], &[]));
    assert_eq!(v.members.len(), 2);
    assert_eq!(relation(&v, 2), Some(Relation::Keep));
    assert!(v.pulled.is_empty());
    assert_eq!(v.subject, "Topic");
    assert_eq!(v.text, "Text.");
}

#[test]
fn by_must_name_a_member_or_a_pulled_in_statement() {
    let a = answer(
        vec![
            member(1, MemberRole::Covered, Some(2)),
            member(2, MemberRole::Keep, None),
            member(3, MemberRole::Covered, Some(10)),
            member(4, MemberRole::Covered, Some(55)),
        ],
        vec![pull(10, PullRole::Adds, 2)],
    );
    let v = validate(&a, &universe(&[1, 2, 3, 4], &[10, 11], &[]));
    assert_eq!(relation(&v, 1), Some(Relation::CoveredBy(2)));
    assert_eq!(relation(&v, 3), Some(Relation::CoveredBy(10)));
    assert_eq!(relation(&v, 4), Some(Relation::Keep));
}

#[test]
fn a_nearby_statement_that_was_not_pulled_in_cannot_be_named() {
    let a = answer(vec![member(1, MemberRole::Covered, Some(11))], vec![]);
    let v = validate(&a, &universe(&[1], &[11], &[]));
    assert_eq!(relation(&v, 1), Some(Relation::Keep));
}

#[test]
fn a_statement_cannot_be_covered_by_itself_or_by_one_it_covers() {
    let a = answer(
        vec![
            member(1, MemberRole::Covered, Some(1)),
            member(2, MemberRole::Covered, Some(3)),
            member(3, MemberRole::Covered, Some(2)),
            member(4, MemberRole::Covered, Some(5)),
            member(5, MemberRole::Covered, Some(6)),
            member(6, MemberRole::Covered, Some(4)),
        ],
        vec![],
    );
    let v = validate(&a, &universe(&[1, 2, 3, 4, 5, 6], &[], &[]));
    assert_eq!(relation(&v, 1), Some(Relation::Keep));
    assert_eq!(relation(&v, 2), Some(Relation::CoveredBy(3)));
    assert_eq!(relation(&v, 3), Some(Relation::Keep));
    assert_eq!(relation(&v, 4), Some(Relation::CoveredBy(5)));
    assert_eq!(relation(&v, 5), Some(Relation::CoveredBy(6)));
    assert_eq!(relation(&v, 6), Some(Relation::Keep));
}

#[test]
fn a_pulled_statement_that_covers_a_member_covers_it_and_cycles_are_refused() {
    let a = answer(
        vec![member(1, MemberRole::Keep, None), member(2, MemberRole::Covered, Some(10))],
        vec![pull(10, PullRole::Covers, 1), pull(11, PullRole::Covered, 1)],
    );
    let v = validate(&a, &universe(&[1, 2], &[10, 11], &[]));
    assert_eq!(relation(&v, 1), Some(Relation::CoveredBy(10)));
    assert_eq!(relation(&v, 2), Some(Relation::CoveredBy(10)));
    let kinds: Vec<(i64, PullKind)> = v.pulled.iter().map(|p| (p.id, p.kind)).collect();
    assert_eq!(kinds, vec![(10, PullKind::CoversMember), (11, PullKind::CoveredByMember)]);
}

#[test]
fn a_pulled_statement_covered_by_a_member_who_it_also_covers_is_refused() {
    let a = answer(
        vec![member(1, MemberRole::Covered, Some(10))],
        vec![pull(10, PullRole::Covered, 1)],
    );
    let v = validate(&a, &universe(&[1], &[10], &[]));
    assert_eq!(relation(&v, 1), Some(Relation::Keep));
    assert_eq!(v.pulled.len(), 1);
}

#[test]
fn evidence_for_must_name_one_of_the_two_conflicting_statements() {
    let mut named = member(1, MemberRole::Conflicts, Some(2));
    named.evidence_for = Some(2);
    let mut stray = member(3, MemberRole::Conflicts, Some(4));
    stray.evidence_for = Some(9);
    let a = answer(vec![named, stray], vec![]);
    let v = validate(&a, &universe(&[1, 2, 3, 4], &[], &[]));
    assert_eq!(v.members[0].evidence_for, Some(2));
    assert_eq!(v.members[1].evidence_for, None);
}

#[test]
fn merges_keep_only_known_ids_and_evidence_only_known_items() {
    let mut a = answer(vec![], vec![]);
    a.merges = vec![super::answer::MergeGroup {
        ids: vec![1, 2, 404],
        text: " merged ".to_owned(),
    }];
    a.evidence_used = vec![
        super::answer::EvidenceUse { id: 1_000_000, shows: "x".to_owned() },
        super::answer::EvidenceUse { id: 5, shows: "y".to_owned() },
    ];
    let v = validate(&a, &universe(&[1, 2], &[], &[1_000_000]));
    assert_eq!(v.merges[0].ids, vec![1, 2]);
    assert_eq!(v.merges[0].text, "merged");
    assert_eq!(v.evidence_used.len(), 1);
}

#[test]
fn the_answer_parses_from_the_json_the_schema_describes() -> anyhow::Result<()> {
    let json = serde_json::json!({
        "subject": "Commits", "text": "Use jj.",
        "members": [{"id": 1, "role": "covered", "by": 2, "note": "same", "evidence_for": null}],
        "pull_in": [{"id": 9, "role": "covers", "by": 1, "note": "", "evidence_for": null}],
        "evidence_used": [{"id": 1000000, "shows": "later"}],
        "merges": [{"ids": [1, 2], "text": "Use jj."}]
    });
    let parsed: JudgeAnswer = serde_json::from_value(json)?;
    assert_eq!(parsed.members[0].role, MemberRole::Covered);
    assert_eq!(parsed.pull_in[0].role, PullRole::Covers);
    Ok(())
}

#[test]
fn outdated_and_tighten_keep_only_known_ids_known_evidence_and_one_per_statement() {
    use super::answer::{Outdated, Tighten};
    let mut a = answer(vec![], vec![]);
    a.outdated = vec![
        Outdated { id: 1, evidence_id: 1_000_000, text: " now jj ".to_owned() },
        Outdated { id: 1, evidence_id: 1_000_000, text: "dup".to_owned() },
        Outdated { id: 9, evidence_id: 1_000_000, text: "unknown id".to_owned() },
        Outdated { id: 2, evidence_id: 5, text: "unknown evidence".to_owned() },
        Outdated { id: 2, evidence_id: 1_000_000, text: "  ".to_owned() },
    ];
    a.tighten = vec![
        Tighten { id: 1, text: "already outdated".to_owned() },
        Tighten { id: 2, text: " short ".to_owned() },
    ];
    let v = validate(&a, &universe(&[1, 2], &[], &[1_000_000]));
    assert_eq!(v.outdated.len(), 1);
    assert_eq!(v.outdated[0].text, "now jj");
    assert_eq!(v.tighten.len(), 1);
    assert_eq!(v.tighten[0].id, 2);
    assert_eq!(v.evidence_used.len(), 1);
}
