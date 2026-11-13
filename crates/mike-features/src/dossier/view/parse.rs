use chrono::{DateTime, Utc};

use super::types::{AsOf, Audience, Budget, SessionSlices, SteererSection};

pub fn parse_as_of(at: DateTime<Utc>, mode: Option<&str>) -> Result<AsOf, String> {
    match mode.unwrap_or("knowledge") {
        "knowledge" => Ok(AsOf::knowledge(at)),
        "reconstructed" => Ok(AsOf::reconstructed_for_eval(at)),
        other => Err(format!(
            "mode must be knowledge or reconstructed, got {other:?}"
        )),
    }
}

fn split(list: Option<&str>) -> Vec<&str> {
    list.map(|l| {
        l.split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect()
    })
    .unwrap_or_default()
}

pub fn parse_slices(list: Option<&str>) -> Result<SessionSlices, String> {
    let mut slices = SessionSlices::default();
    for name in split(list) {
        match name {
            "reactions" => slices.reactions = true,
            "promises" => slices.promises = true,
            "directives" => slices.directives = true,
            other => {
                return Err(format!(
                    "unknown slice {other:?}; use reactions, promises, directives"
                ));
            }
        }
    }
    Ok(slices)
}

pub fn parse_sections(list: Option<&str>) -> Result<Vec<SteererSection>, String> {
    let names = split(list);
    if names.is_empty() {
        return Ok(vec![
            SteererSection::Ending,
            SteererSection::RecentReactions,
            SteererSection::Directives { classes: None },
            SteererSection::Promises,
            SteererSection::SteeringFacts,
            SteererSection::Asks,
            SteererSection::Corrections,
        ]);
    }
    names
        .into_iter()
        .map(|name| match name {
            "ending" => Ok(SteererSection::Ending),
            "reactions" => Ok(SteererSection::RecentReactions),
            "directives" => Ok(SteererSection::Directives { classes: None }),
            "promises" => Ok(SteererSection::Promises),
            "steering_facts" | "steering-facts" => Ok(SteererSection::SteeringFacts),
            "asks" => Ok(SteererSection::Asks),
            "corrections" => Ok(SteererSection::Corrections),
            "similar_corrections" => Ok(SteererSection::SimilarCorrections),
            "similar_directives" => Ok(SteererSection::SimilarDirectives),
            "similar_lessons" => Ok(SteererSection::SimilarLessons),
            "notes" => Ok(SteererSection::EarlierNotes),
            "opening" => Ok(SteererSection::TurnOpening),
            "turns" => Ok(SteererSection::EarlierTurns),
            other => Err(format!("unknown steerer section {other:?}")),
        })
        .collect()
}

pub struct AudienceArgs<'a> {
    pub audience: &'a str,
    pub turn_id: Option<i64>,
    pub slices: Option<&'a str>,
    pub sections: Option<&'a str>,
    pub budget_chars: usize,
}

pub fn parse_audience_parts(args: &AudienceArgs<'_>) -> Result<Option<(Audience, Budget)>, String> {
    let budget = Budget::Chars(args.budget_chars);
    match args.audience {
        "none" => Ok(None),
        "judge" => {
            let turn_id = args
                .turn_id
                .ok_or_else(|| "audience=judge needs turn_id".to_owned())?;
            Ok(Some((Audience::Judge { turn_id }, Budget::JudgeSlices)))
        }
        "session_judge" | "session-judge" => Ok(Some((
            Audience::SessionJudge {
                slices: parse_slices(args.slices)?,
            },
            budget,
        ))),
        "steerer" => Ok(Some((
            Audience::Steerer {
                turn_id: args.turn_id,
                sections: parse_sections(args.sections)?,
            },
            budget,
        ))),
        other => Err(format!(
            "audience must be none, judge, session_judge or steerer, got {other:?}"
        )),
    }
}
