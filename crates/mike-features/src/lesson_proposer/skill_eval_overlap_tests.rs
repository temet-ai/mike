use super::catalog::RealSkills;
use super::overlap::{existing_skills, own_folders};
use super::progress::Progress;
use crate::store::persistence::lessons::skill_draft_eval::{AgentScope, SiblingSkill};

fn skill(folder: &str) -> SiblingSkill {
    SiblingSkill {
        folder: folder.to_owned(),
        description: format!("about {folder}"),
    }
}

fn coordinator() -> AgentScope {
    AgentScope {
        name: "Fleet Coordinator".to_owned(),
        description: None,
        definition_path: None,
    }
}

fn names(progress: &Progress, own: &[String]) -> Vec<String> {
    existing_skills(progress, own)
        .into_iter()
        .map(|(name, _)| name)
        .collect()
}

#[test]
fn existing_skills_for_the_reviewer_are_the_whole_project_list_without_the_draft_or_repeats() {
    let mut progress = Progress::fresh();
    progress.siblings = Some(vec![skill("ui"), skill("database-migrations")]);
    progress.real_skills = Some(RealSkills {
        request_ids: vec![9],
        seen: 3,
        skills: vec![skill("lint")],
        listed: vec![skill("ui"), skill("lint"), skill("deploy")],
    });
    let own = own_folders("database-migrations", None);
    assert_eq!(names(&progress, &own), vec!["ui", "lint", "deploy"]);
    assert!(existing_skills(&Progress::fresh(), &own).is_empty());
}

#[test]
fn the_skill_in_the_drafts_own_folder_is_left_out_of_the_overlap_list() {
    let mut progress = Progress::fresh();
    progress.siblings = Some(vec![skill("fleet-seat-placement"), skill("ui")]);
    progress.real_skills = Some(RealSkills {
        request_ids: vec![9],
        seen: 3,
        skills: Vec::new(),
        listed: vec![
            skill("fleet-seat-placement"),
            skill("fleet-seat-placement-fleet-coordinator"),
            skill("deploy"),
        ],
    });
    let agent = coordinator();
    let own = own_folders("fleet-seat-placement-fleet-coordinator", Some(&agent));
    assert_eq!(
        own,
        vec![
            "fleet-seat-placement-fleet-coordinator".to_owned(),
            "fleet-seat-placement".to_owned()
        ]
    );
    assert_eq!(names(&progress, &own), vec!["ui", "deploy"]);
}

#[test]
fn a_draft_with_no_agent_suffix_has_only_its_own_folder() {
    let agent = coordinator();
    assert_eq!(own_folders("ui", Some(&agent)), vec!["ui".to_owned()]);
    assert_eq!(own_folders("ui", None), vec!["ui".to_owned()]);
    assert_eq!(
        own_folders("-fleet-coordinator", Some(&agent)),
        vec!["-fleet-coordinator".to_owned()]
    );
}
