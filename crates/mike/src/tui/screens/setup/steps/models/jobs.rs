#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Tier {
    Fast,
    Standard,
    Strong,
}

impl Tier {
    pub(super) const ALL: [Tier; 3] = [Tier::Fast, Tier::Standard, Tier::Strong];

    pub(super) fn name(self) -> &'static str {
        match self {
            Tier::Fast => "fast",
            Tier::Standard => "standard",
            Tier::Strong => "strong",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Needs {
    Switch(&'static str),
    LessonRelations,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Job {
    pub(super) name: &'static str,
    pub(super) key: &'static str,
    pub(super) tier: Tier,
    pub(super) needs: Needs,
}

pub(super) const JOBS: [Job; 6] = [
    Job {
        name: "Steerers",
        key: "steerer.model",
        tier: Tier::Strong,
        needs: Needs::Switch("steerer"),
    },
    Job {
        name: "Turn judge",
        key: "judge_worker.model",
        tier: Tier::Standard,
        needs: Needs::Switch("judge_worker"),
    },
    Job {
        name: "Session judge",
        key: "session_judge.model",
        tier: Tier::Fast,
        needs: Needs::Switch("session_judge"),
    },
    Job {
        name: "Lesson proposer",
        key: "judge_lessons.model",
        tier: Tier::Fast,
        needs: Needs::Switch("judge_lessons"),
    },
    Job {
        name: "Lesson relations (proposer)",
        key: "lesson_relations.proposer_model",
        tier: Tier::Strong,
        needs: Needs::LessonRelations,
    },
    Job {
        name: "Lesson relations (adjudicator)",
        key: "lesson_relations.adjudicator_model",
        tier: Tier::Fast,
        needs: Needs::LessonRelations,
    },
];

pub(super) fn running(switches: &[(String, bool)], lesson_relations: bool) -> Vec<Job> {
    let on = |key: &str| switches.iter().any(|(k, enabled)| k == key && *enabled);
    JOBS.into_iter()
        .filter(|job| match job.needs {
            Needs::Switch(key) => on(key),
            Needs::LessonRelations => lesson_relations,
        })
        .collect()
}

pub(super) fn used_by(jobs: &[Job], tier: Tier) -> String {
    let names: Vec<&str> = jobs
        .iter()
        .filter(|job| job.tier == tier)
        .map(|job| job.name)
        .collect();
    if names.is_empty() {
        "no job you switched on".to_string()
    } else {
        names.join(", ")
    }
}
