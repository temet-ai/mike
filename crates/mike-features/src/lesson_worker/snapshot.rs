use crate::store::persistence::lessons_repo::LessonsPersist as _;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, LazyLock};

use dashmap::{DashMap, DashSet};

use crate::store::persistence::agents::AgentAlias;
use crate::store::persistence::lessons::{ChangeMark, InjectableLessonRow};
use crate::store::persistence::{PersistError, PersistHandle};

static CLIENT_SERVED_AGENT_LESSONS: LazyLock<DashSet<(String, String)>> =
    LazyLock::new(DashSet::new);

#[derive(Debug, Default, Clone)]
pub struct LessonSnapshot {
    pub shared: Vec<InjectableLesson>,
    pub relevance: Arc<super::relevance::LessonRelevance>,
    pub session_project_paths: HashMap<String, String>,
    pub by_project: HashMap<String, Vec<InjectableLesson>>,
    pub by_agent: HashMap<i64, Vec<InjectableLesson>>,
    pub persona_alias_map: HashMap<String, i64>,
    pub topic_withheld: Arc<HashSet<i64>>,
    pub agent_lesson_injected: Arc<DashMap<(String, String), ()>>,
    pub lessons_fingerprint: Option<ChangeMark>,
    pub sessions_fingerprint: Option<ChangeMark>,
}

#[derive(Debug, Clone)]
pub struct InjectableLesson {
    pub id: i64,
    pub kind: String,
    pub text: String,
    pub topic: Option<String>,
    pub agent_id: Option<i64>,
}

impl LessonSnapshot {
    pub fn project_for_session<'a>(&'a self, session: &str, path: &'a str) -> &'a str {
        self.session_project_paths
            .get(session)
            .map(String::as_str)
            .unwrap_or(path)
    }

    pub fn with_session_project(&self, session: String, project: String) -> Self {
        let mut snapshot = self.clone();
        snapshot.session_project_paths.insert(session, project);
        snapshot
    }

    pub fn withholds_content(&self, lesson: &InjectableLesson) -> bool {
        lesson.topic.is_some() && self.topic_withheld.contains(&lesson.id)
    }

    pub fn for_project(&self, project_path: &str) -> &[InjectableLesson] {
        if let Some(lessons) = self.by_project.get(project_path) {
            return lessons.as_slice();
        }
        crate::extract::request::legacy_trimmed_alias(project_path)
            .and_then(|alias| self.by_project.get(&alias))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn agent_lessons_for(&self, agent_id: i64) -> &[InjectableLesson] {
        self.by_agent
            .get(&agent_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn resolve_persona(&self, persona: &str) -> Option<i64> {
        self.persona_alias_map.get(&persona.to_lowercase()).copied()
    }

    pub fn mark_agent_lesson_injected(&self, session_id: &str, persona: &str) -> bool {
        let key = (session_id.to_string(), persona.to_lowercase());
        self.agent_lesson_injected.insert(key, ()).is_none()
    }

    pub fn mark_agent_lesson_served_by_client(&self, session_id: &str, agent_type: &str) -> bool {
        CLIENT_SERVED_AGENT_LESSONS.insert((session_id.to_string(), agent_type.to_lowercase()));
        self.mark_agent_lesson_injected(session_id, agent_type)
    }

    pub fn agent_lesson_served_by_client(&self, session_id: &str, persona: &str) -> bool {
        let persona = persona.to_lowercase();
        let agent = self.resolve_persona(&persona);
        CLIENT_SERVED_AGENT_LESSONS.iter().any(|entry| {
            let (session, served) = entry.key();
            session == session_id
                && (*served == persona || agent.is_some() && self.resolve_persona(served) == agent)
        })
    }

    pub fn session_agent_lessons_served_by_client(&self, session_id: &str) -> bool {
        CLIENT_SERVED_AGENT_LESSONS
            .iter()
            .any(|entry| entry.key().0 == session_id)
    }

    pub fn project_count(&self) -> usize {
        self.by_project.len()
    }

    pub fn from_pairs<I>(entries: I) -> Self
    where
        I: IntoIterator<Item = (String, Vec<(String, String)>)>,
    {
        let mut next_id = 0i64;
        let by_project = entries
            .into_iter()
            .map(|(project, lessons)| {
                let lessons = lessons
                    .into_iter()
                    .map(|(kind, text)| {
                        next_id += 1;
                        InjectableLesson {
                            id: next_id,
                            kind,
                            text,
                            topic: None,
                            agent_id: None,
                        }
                    })
                    .collect();
                (project, lessons)
            })
            .collect();
        Self {
            by_project,
            ..Default::default()
        }
    }

    #[cfg(any(test, feature = "test-seams"))]
    pub fn from_topic_fixtures(
        project: &str,
        lessons: Vec<(&str, Option<&str>, bool)>,
    ) -> Self {
        let mut withheld = HashSet::new();
        let injectable = lessons
            .into_iter()
            .zip(1i64..)
            .map(|((text, topic, is_withheld), id)| {
                if is_withheld {
                    withheld.insert(id);
                }
                InjectableLesson {
                    id,
                    kind: "convention".to_owned(),
                    text: text.to_owned(),
                    topic: topic.map(str::to_owned),
                    agent_id: None,
                }
            })
            .collect();
        Self {
            by_project: HashMap::from([(project.to_owned(), injectable)]),
            topic_withheld: Arc::new(withheld),
            ..Default::default()
        }
    }

    pub fn from_agent_fixtures<A, L>(aliases: A, agent_lessons: L) -> Self
    where
        A: IntoIterator<Item = (String, i64)>,
        L: IntoIterator<Item = (i64, Vec<(String, String)>)>,
    {
        let persona_alias_map = aliases
            .into_iter()
            .map(|(persona, agent_id)| (persona.to_lowercase(), agent_id))
            .collect();
        let mut next_id = 0i64;
        let by_agent = agent_lessons
            .into_iter()
            .map(|(agent_id, lessons)| {
                let lessons = lessons
                    .into_iter()
                    .map(|(kind, text)| {
                        next_id += 1;
                        InjectableLesson {
                            id: next_id,
                            kind,
                            text,
                            topic: None,
                            agent_id: Some(agent_id),
                        }
                    })
                    .collect();
                (agent_id, lessons)
            })
            .collect();
        Self {
            by_agent,
            persona_alias_map,
            ..Default::default()
        }
    }
}

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

pub async fn load_topic_withheld(
    persist: &PersistHandle,
) -> Result<HashSet<i64>, PersistError> {
    let ids = persist
        .lessons()
        .list_topic_lessons_withheld_from_content()
        .await?;
    Ok(ids.into_iter().collect())
}
