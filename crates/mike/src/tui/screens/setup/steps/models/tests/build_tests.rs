use super::*;

#[test]
fn only_the_jobs_the_chosen_features_run_are_listed() {
    let none = running(&[], false);
    assert!(none.is_empty());
    let some = running(&on(&["judge_worker", "session_judge"]), false);
    assert_eq!(
        some.iter().map(|j| j.name).collect::<Vec<_>>(),
        vec!["Turn judge", "Session judge"]
    );
    let off = vec![("steerer".to_string(), false)];
    assert!(running(&off, false).is_empty());
    let relations = running(&[], true);
    assert_eq!(relations.len(), 2);
    assert_eq!(JOBS.len(), 6);
}

#[test]
fn a_tier_names_the_jobs_that_use_it() {
    let jobs = running(
        &on(&["steerer", "judge_worker", "session_judge", "judge_lessons"]),
        false,
    );
    assert_eq!(used_by(&jobs, Tier::Fast), "Session judge, Lesson proposer");
    assert_eq!(used_by(&jobs, Tier::Standard), "Turn judge");
    assert_eq!(used_by(&jobs, Tier::Strong), "Steerers");
    assert_eq!(used_by(&[], Tier::Strong), "no job you switched on");
}

#[test]
fn models_are_normalised_into_their_providers_form() {
    assert_eq!(normalize("claude", "haiku"), "claude/haiku");
    assert_eq!(normalize("claude", "claude/opus"), "claude/opus");
    assert_eq!(normalize("codex", " gpt-6-sol "), "codex/gpt-6-sol");
    assert_eq!(
        normalize("openrouter", "google/gemini-3.8-flash"),
        "openrouter/google/gemini-3.8-flash"
    );
    assert_eq!(normalize("openrouter", "openrouter/x/y"), "openrouter/x/y");
    assert_eq!(provider_of("claude/haiku"), "claude");
    assert_eq!(provider_of("codex/x"), "codex");
    assert_eq!(provider_of("google/gemini-3.8-flash"), "openrouter");
}

#[test]
fn the_intro_recommends_the_first_available_provider_and_names_what_is_not_available() {
    let source = source(false, &["judge_worker"], false);
    let panel = plan_panel(&source, "");
    let labels: Vec<&str> = panel.choices.iter().map(|c| c.label.as_str()).collect();
    assert_eq!(
        labels,
        vec![
            "Use Claude for every job",
            "Choose a provider and a model for each job",
            "Set up an OpenRouter key",
            "Leave the models as they are"
        ]
    );
    assert_eq!(
        panel.selected().map(|c| c.action.clone()),
        Some(IntroAction::Provider("claude".to_string()))
    );
    let found = panel.found.join("\n");
    assert!(found.contains("Claude: claude is ready"), "{found}");
    assert!(found.contains("Codex: not available. fix codex"), "{found}");
    assert!(
        found.contains("OpenRouter: not available. fix openrouter"),
        "{found}"
    );
    assert!(
        found.contains("Jobs that need a model: Turn judge"),
        "{found}"
    );
    assert!(
        !labels.iter().any(|l| l.contains("Codex")),
        "an unavailable provider is not offered"
    );
}

#[test]
fn with_openrouter_available_no_key_setup_is_offered_and_it_is_listed_last_among_providers() {
    let source = source(true, &["judge_worker"], false);
    let labels: Vec<String> = plan_panel(&source, "")
        .choices
        .iter()
        .map(|c| c.label.clone())
        .collect();
    assert_eq!(labels[0], "Use Claude for every job");
    assert_eq!(labels[1], "Use OpenRouter for every job");
    assert!(!labels.iter().any(|l| l.contains("Set up")));
}

#[test]
fn the_key_setup_warns_that_a_background_service_does_not_see_shell_variables() {
    let mut with_service = source(false, &["judge_worker"], false);
    with_service.service_installed = true;
    let panel = plan_panel(&with_service, "OPENROUTER_API_KEY");
    let setup = panel
        .choices
        .iter()
        .find(|c| c.action == IntroAction::SetUpOpenRouter);
    let why = setup.map(|c| c.why.clone()).unwrap_or_default();
    assert!(
        why.contains("starts without your shell's variables"),
        "{why}"
    );
    assert_eq!(
        setup
            .and_then(|c| c.input.as_ref())
            .map(|i| i.value.clone()),
        Some("OPENROUTER_API_KEY".to_string())
    );
}

#[test]
fn without_jobs_the_step_says_there_is_nothing_to_set() {
    let panel = idle_panel("nothing here");
    assert!(panel.choices.is_empty());
    assert_eq!(panel.note.as_deref(), Some("nothing here"));
}

#[test]
fn one_provider_form_has_a_provider_three_tiers_three_limits_and_a_save_row() {
    let source = source(true, &["steerer", "judge_worker", "session_judge"], false);
    let form = build_form(&Mode::One("claude".to_string()), &source);
    let roles: Vec<Role> = form.rows.iter().map(|r| r.role).collect();
    assert_eq!(
        roles,
        vec![
            Role::Provider,
            Role::Tier(Tier::Fast),
            Role::Tier(Tier::Standard),
            Role::Tier(Tier::Strong),
            Role::DailySpend,
            Role::MaxCalls,
            Role::RatePerMinute,
            Role::Submit
        ]
    );
    assert_eq!(form.rows[1].cells[0].value(), "claude/haiku");
    assert_eq!(form.rows[3].cells[0].value(), "claude/opus");
    assert_eq!(form.rows[1].detail, "Used by: Session judge");
    assert!(
        form.note
            .clone()
            .unwrap_or_default()
            .contains("your own Claude or ChatGPT plan")
    );
}

#[test]
fn collecting_one_provider_gives_the_provider_the_three_tier_models_and_the_limits()
-> anyhow::Result<()> {
    let source = source(true, &["judge_worker"], false);
    let mode = Mode::One("claude".to_string());
    let form = build_form(&mode, &source);
    let settings = collect(&form, &source, &mode).map_err(|e| anyhow::anyhow!(e))?;
    let keys: Vec<&str> = settings.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(
        keys,
        vec![
            "llm.provider",
            "llm.provider_models.claude.fast",
            "llm.provider_models.claude.standard",
            "llm.provider_models.claude.strong",
            "llm.daily_spend_ceiling_usd",
            "llm.ceiling_max_calls",
            "llm.rate_limit_per_minute"
        ]
    );
    assert_eq!(settings[0].1, json!("claude"));
    assert_eq!(settings[2].1, json!("claude/sonnet"));
    assert_eq!(settings[4].1, json!(0.0));
    assert_eq!(settings[5].1, json!(100));
    Ok(())
}

#[test]
fn changing_the_provider_in_the_form_reloads_the_tier_models() {
    let source = source(true, &["judge_worker"], false);
    let mode = Mode::One("claude".to_string());
    let mut form = build_form(&mode, &source);
    assert!(matches!(
        form.handle(key(KeyCode::Right)),
        FormEvent::Changed { row: 0, cell: 0 }
    ));
    assert_eq!(form.rows[0].cells[0].value(), "openrouter");
    refresh_after_change(&mut form, &source, 0, 0);
    assert!(
        form.rows[2].cells[0].value().starts_with("openrouter/"),
        "{}",
        form.rows[2].cells[0].value()
    );
    assert!(
        form.rows[2].cells[0]
            .options
            .iter()
            .all(|o| o.starts_with("openrouter/"))
    );
    assert_eq!(
        form.rows[2].cells[0].options[0], "openrouter/anthropic/claude-sonnet-5-5",
        "known-good models come first"
    );
}

#[test]
fn the_per_job_form_has_a_provider_and_a_model_for_every_running_job() -> anyhow::Result<()> {
    let source = source(true, &["judge_worker", "session_judge"], false);
    let form = build_form(&Mode::PerJob, &source);
    assert_eq!(form.rows[0].label, "Turn judge");
    assert_eq!(
        form.rows[0].cells[0].value(),
        "openrouter",
        "its stored model is an OpenRouter model"
    );
    assert_eq!(
        form.rows[0].cells[1].value(),
        "openrouter/google/gemini-3.8-flash"
    );
    assert_eq!(form.rows[1].cells[0].value(), "claude");
    assert_eq!(form.rows[1].cells[1].value(), "claude/haiku");
    let settings = collect(&form, &source, &Mode::PerJob).map_err(|e| anyhow::anyhow!(e))?;
    assert_eq!(settings[0], ("llm.provider".to_string(), json!("per_role")));
    assert_eq!(
        settings[1],
        (
            "judge_worker.model".to_string(),
            json!("openrouter/google/gemini-3.8-flash")
        )
    );
    assert_eq!(
        settings[2],
        ("session_judge.model".to_string(), json!("claude/haiku"))
    );
    Ok(())
}

#[test]
fn a_job_whose_stored_provider_is_unavailable_starts_on_an_available_one() {
    let source = source(false, &["judge_worker"], false);
    let form = build_form(&Mode::PerJob, &source);
    assert_eq!(form.rows[0].cells[0].value(), "claude");
    assert_eq!(form.rows[0].cells[0].options, vec!["claude"]);
    assert_eq!(
        form.rows[0].cells[1].value(),
        "claude/sonnet",
        "the tier default of the provider"
    );
}

#[test]
fn a_typed_model_is_prefixed_for_its_provider_and_a_clash_is_refused() {
    let source = source(true, &["session_judge"], false);
    let mut form = build_form(&Mode::PerJob, &source);
    form.rows[0].cells[1] = Cell::text("sonnet");
    let settings = collect(&form, &source, &Mode::PerJob)
        .ok()
        .unwrap_or_default();
    assert_eq!(settings[1].1, json!("claude/sonnet"));
    form.rows[0].cells[1] = Cell::text("codex/gpt-6-sol");
    let error = collect(&form, &source, &Mode::PerJob)
        .err()
        .unwrap_or_default();
    assert!(error.contains("is not a claude model"), "{error}");
    form.rows[0].cells[1] = Cell::text("  ");
    assert!(collect(&form, &source, &Mode::PerJob).is_err());
}

#[test]
fn the_lesson_relations_proposer_and_adjudicator_must_differ() {
    let source = source(true, &[], true);
    let mut form = build_form(&Mode::PerJob, &source);
    let first = form.rows[0].cells[1].value();
    form.rows[1].cells[0] = form.rows[0].cells[0].clone();
    form.rows[1].cells[1] = Cell::text(&first);
    let error = collect(&form, &source, &Mode::PerJob)
        .err()
        .unwrap_or_default();
    assert!(error.contains("must use different models"), "{error}");
}

#[test]
fn the_limits_are_validated_with_a_sentence_that_names_the_field() {
    let source = source(true, &["judge_worker"], false);
    let mode = Mode::One("claude".to_string());
    for (role, bad, expected) in [
        (Role::DailySpend, "lots", "daily spend limit"),
        (Role::DailySpend, "5000", "daily spend limit"),
        (Role::MaxCalls, "0", "call limit"),
        (Role::RatePerMinute, "1.5", "calls-per-minute limit"),
    ] {
        let mut form = build_form(&mode, &source);
        if let Some(row) = form.rows.iter_mut().find(|r| r.role == role) {
            row.cells[0] = Cell::text(bad);
        }
        let error = collect(&form, &source, &mode).err().unwrap_or_default();
        assert!(error.contains(expected), "{bad}: {error}");
    }
}

#[test]
fn the_form_edits_a_limit_by_typing_and_never_treats_letters_as_shortcuts() {
    let source = source(true, &["judge_worker"], false);
    let mut form = build_form(&Mode::One("claude".to_string()), &source);
    form.cursor = form
        .rows
        .iter()
        .position(|r| r.role == Role::DailySpend)
        .unwrap_or(0);
    form.handle(key(KeyCode::Char('e')));
    assert!(form.editing);
    for _ in 0..3 {
        form.handle(key(KeyCode::Backspace));
    }
    for event in typed("2.5bsf") {
        assert_eq!(form.handle(event), FormEvent::Idle);
    }
    form.handle(key(KeyCode::Enter));
    assert!(!form.editing);
    assert_eq!(form.rows[form.cursor].cells[0].value(), "2.5bsf");
    assert_eq!(form.handle(key(KeyCode::Char('s'))), FormEvent::Skip);
    assert_eq!(form.handle(key(KeyCode::Char('b'))), FormEvent::Back);
}

#[test]
fn the_save_row_submits_and_a_pick_cell_wraps() {
    let source = source(true, &["judge_worker"], false);
    let mut form = build_form(&Mode::One("claude".to_string()), &source);
    form.cursor = form.rows.len() - 1;
    assert_eq!(form.handle(key(KeyCode::Enter)), FormEvent::Submit);
    form.cursor = 1;
    let count = form.rows[1].cells[0].options.len();
    for _ in 0..count {
        form.handle(key(KeyCode::Right));
    }
    assert_eq!(form.rows[1].cells[0].value(), "claude/haiku");
}

#[test]
fn providers_in_lists_each_provider_a_chosen_model_runs_on() {
    let settings = vec![
        ("llm.provider".to_string(), json!("per_role")),
        ("judge_worker.model".to_string(), json!("claude/sonnet")),
        (
            "session_judge.model".to_string(),
            json!("openrouter/google/gemini-3.8-flash"),
        ),
        ("steerer.model".to_string(), json!("claude/opus")),
        ("llm.daily_spend_ceiling_usd".to_string(), json!(1.0)),
    ];
    assert_eq!(providers_in(&settings), vec!["claude", "openrouter"]);
}
