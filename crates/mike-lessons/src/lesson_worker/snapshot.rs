use crate::store::persistence::lessons_repo::LessonsPersist as _;
use std::collections::{HashMap, HashSet};

use crate::store::persistence::agents::AgentAlias;
use crate::store::persistence::lessons::{ChangeMark, InjectableLessonRow};
use crate::store::persistence::{PersistError, PersistHandle};

pub use crate::lesson_cache::snapshot::{InjectableLesson, LessonCache, LessonSnapshot};

pub struct LessonSet {
    pub fingerprint: ChangeMark,
    pub by_project: HashMap<String, Vec<InjectableLesson>>,
    pub by_agent: HashMap<i64, Vec<InjectableLesson>>,
}

pub async fn load_changed_lessons(
    persist: &PersistHandle,
    previous: Option<ChangeMark>,
) -> Result<Option<LessonSet>, PersistError> {
    let fingerprint = persist.lessons().injectable_lessons_fingerprint().await?;
    if previous == Some(fingerprint) {
        return Ok(None);
    }
    let rows = persist.lessons().list_project_injectable_lessons().await?;
    let (by_project, by_agent) = group_injectable(rows);
    Ok(Some(LessonSet {
        fingerprint,
        by_project,
        by_agent,
    }))
}

fn group_injectable(
    rows: Vec<InjectableLessonRow>,
) -> (
    HashMap<String, Vec<InjectableLesson>>,
    HashMap<i64, Vec<InjectableLesson>>,
) {
    let mut by_project: HashMap<String, Vec<InjectableLesson>> = HashMap::new();
    let mut by_agent: HashMap<i64, Vec<InjectableLesson>> = HashMap::new();
    for row in rows {
        let lesson = InjectableLesson {
            id: row.id,
            kind: row.kind,
            text: row.text,
            topic: row.topic,
            agent_id: row.agent_id,
        };
        if let Some(agent_id) = lesson.agent_id {
            by_agent.entry(agent_id).or_default().push(lesson.clone());
        }
        by_project.entry(row.project_path).or_default().push(lesson);
    }
    merge_legacy_aliases(&mut by_project);
    (by_project, by_agent)
}

fn merge_legacy_aliases(by_project: &mut HashMap<String, Vec<InjectableLesson>>) {
    let legacy_merges: Vec<(String, Vec<InjectableLesson>)> = by_project
        .keys()
        .filter_map(|root| {
            let alias = crate::extract::request::legacy_trimmed_alias(root)?;
            let lessons = by_project.get(&alias)?;
            Some((root.clone(), lessons.clone()))
        })
        .collect();
    for (root, lessons) in legacy_merges {
        by_project.entry(root).or_default().extend(lessons);
    }
}

pub async fn load_persona_alias_map(
    persist: &PersistHandle,
) -> Result<HashMap<String, i64>, PersistError> {
    let aliases = persist.fetch_resolution_map().await?;
    Ok(persona_alias_map_from_rows(aliases))
}

fn persona_alias_map_from_rows(aliases: Vec<AgentAlias>) -> HashMap<String, i64> {
    let (names, others): (Vec<AgentAlias>, Vec<AgentAlias>) =
        aliases.into_iter().partition(|a| a.kind == "name");
    others
        .into_iter()
        .chain(names)
        .map(|a| (a.alias.to_lowercase(), a.agent_id))
        .collect()
}

pub async fn load_topic_withheld(persist: &PersistHandle) -> Result<HashSet<i64>, PersistError> {
    let ids = persist
        .lessons()
        .list_topic_lessons_withheld_from_content()
        .await?;
    Ok(ids.into_iter().collect())
}
