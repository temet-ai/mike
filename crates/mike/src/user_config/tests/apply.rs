use super::*;

fn env_of<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
    move |name| {
        pairs
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, v)| (*v).to_string())
    }
}

fn environment_after_file(process: &MapEnvironment, entries: &[Entry]) -> MapEnvironment {
    plan(entries, &|name| process.var(name))
        .into_iter()
        .fold(process.clone(), |environment, (name, value)| {
            environment.with(&name, &value)
        })
}

#[test]
fn the_file_fills_only_variables_the_process_does_not_have() {
    let entries = [
        literal("MIKE_LISTEN_ADDR", "from-file"),
        literal("MIKE_INSPECT_ADDR", "from-file"),
    ];
    let planned = plan(&entries, &env_of(&[("MIKE_LISTEN_ADDR", "from-env")]));
    assert_eq!(
        planned,
        [("MIKE_INSPECT_ADDR".to_string(), "from-file".to_string())]
    );
}

#[test]
fn an_empty_value_in_the_file_means_not_set() {
    let planned = plan(&[literal("MIKE_DATABASE_URL", "  ")], &env_of(&[]));
    assert!(planned.is_empty());
}

#[test]
fn a_named_variable_supplies_the_value_when_it_is_set_and_nothing_when_it_is_not() {
    let entries = [from_env("MIKE_STEERER_OPENROUTER_API_KEY", "MY_KEY")];
    assert_eq!(
        plan(&entries, &env_of(&[("MY_KEY", "sk-1")])),
        [(
            "MIKE_STEERER_OPENROUTER_API_KEY".to_string(),
            "sk-1".to_string()
        )]
    );
    assert!(plan(&entries, &env_of(&[])).is_empty());
    assert!(plan(&entries, &env_of(&[("MY_KEY", " ")])).is_empty());
}

#[test]
fn a_literal_beats_a_named_variable_for_the_same_setting() {
    let entries = [
        from_env("MIKE_STEERER_OPENROUTER_API_KEY", "MY_KEY"),
        literal("MIKE_STEERER_OPENROUTER_API_KEY", "literal"),
    ];
    assert_eq!(
        plan(&entries, &env_of(&[("MY_KEY", "named")])),
        [(
            "MIKE_STEERER_OPENROUTER_API_KEY".to_string(),
            "literal".to_string()
        )]
    );
}

#[test]
fn precedence_runs_defaults_then_file_then_environment_then_cli_flags() -> anyhow::Result<()> {
    let (_dir, path) = temp_file(
        "listen_addr = \"127.0.0.1:7001\"\ninspect_addr = \"127.0.0.1:7002\"\nupstream_url = \"https://file.example\"\n",
    )?;
    let process = MapEnvironment::isolated().with("MIKE_INSPECT_ADDR", "127.0.0.1:7777");

    let defaults =
        crate::config::load::load_with(CliOverrides::default(), &MapEnvironment::isolated())?;
    assert_eq!(defaults.proxy_listen_addr, "127.0.0.1:9988");

    let entries = ConfigFile::open(&path)?.entries()?;
    assert_eq!(entries.len(), 3);
    let after_file = environment_after_file(&process, &entries);

    let with_file = crate::config::load::load_with(CliOverrides::default(), &after_file)?;
    assert_eq!(with_file.proxy_listen_addr, "127.0.0.1:7001");
    assert_eq!(with_file.upstream_url, "https://file.example");
    assert_eq!(with_file.inspect_listen_addr, "127.0.0.1:7777");

    let cli = CliOverrides {
        proxy_listen_addr: Some("127.0.0.1:7003".to_string()),
        ..CliOverrides::default()
    };
    assert_eq!(
        crate::config::load::load_with(cli, &after_file)?.proxy_listen_addr,
        "127.0.0.1:7003"
    );
    Ok(())
}

#[test]
fn a_missing_or_broken_file_is_reported_in_its_state_and_assigns_nothing() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let missing = apply_config_file(Ok(&dir.path().join("nope.toml")));
    assert_eq!(missing.state, FileState::Missing);
    assert!(missing.assigned.is_empty());

    let (_broken_dir, broken) = temp_file("database_url = \n")?;
    let applied = apply_config_file(Ok(&broken));
    assert!(matches!(applied.state, FileState::Unusable(_)));
    assert!(applied.assigned.is_empty());

    let nowhere = apply_config_file(Err("HOME is not set".to_string()));
    assert_eq!(
        nowhere.state,
        FileState::NoLocation("HOME is not set".to_string())
    );
    Ok(())
}

#[test]
fn the_report_names_the_file_and_the_source_of_each_variable_without_values() {
    let provenance = Provenance {
        file: Some(PathBuf::from("/home/ada/.config/mike/config.toml")),
        state: FileState::Loaded { keys: 2 },
        origins: BTreeMap::from([
            ("MIKE_DATABASE_URL".to_string(), Origin::ConfigFile),
            ("MIKE_LISTEN_ADDR".to_string(), Origin::Environment),
            ("MIKE_LAUNCH_BACKEND".to_string(), Origin::DotEnv),
        ]),
    };
    let report = render_provenance(&provenance);
    assert!(
        report.contains("/home/ada/.config/mike/config.toml (2 keys)"),
        "{report}"
    );
    assert!(
        report.contains("MIKE_DATABASE_URL: config file"),
        "{report}"
    );
    assert!(report.contains("MIKE_LISTEN_ADDR: environment"), "{report}");
    assert!(
        report.contains("MIKE_LAUNCH_BACKEND: .env in the working directory"),
        "{report}"
    );
    assert!(report.lines().all(|l| l.starts_with('#')), "{report}");
}

#[test]
fn a_multi_line_parse_error_stays_on_one_comment_line() {
    let provenance = Provenance {
        file: Some(PathBuf::from("/c.toml")),
        state: FileState::Unusable("line one\nline two".to_string()),
        origins: BTreeMap::new(),
    };
    let report = render_provenance(&provenance);
    assert!(report.lines().all(|l| l.starts_with('#')), "{report}");
}

#[test]
fn the_planned_variables_come_from_the_file_and_never_replace_a_process_variable()
-> anyhow::Result<()> {
    let (_dir, path) = temp_file(
        "listen_addr = \"127.0.0.1:7200\"\ndatabase_url = \"postgres://file@h/db\"\nlaunch_backend = \"tmux\"\n",
    )?;
    let process = MapEnvironment::isolated().with("MIKE_DATABASE_URL", "postgres://process@h/db");
    let entries = ConfigFile::open(&path)?.entries()?;

    let after_file = environment_after_file(&process, &entries);

    assert_eq!(
        after_file.var("MIKE_LISTEN_ADDR").as_deref(),
        Some("127.0.0.1:7200")
    );
    assert_eq!(
        after_file.var("MIKE_DATABASE_URL").as_deref(),
        Some("postgres://process@h/db")
    );
    assert_eq!(
        after_file.var("MIKE_LAUNCH_BACKEND").as_deref(),
        Some("tmux")
    );
    let planned = plan(&entries, &|name| process.var(name));
    let names: Vec<&str> = planned.iter().map(|(name, _)| name.as_str()).collect();
    assert!(names.contains(&"MIKE_LISTEN_ADDR"), "{names:?}");
    assert!(names.contains(&"MIKE_LAUNCH_BACKEND"), "{names:?}");
    assert!(!names.contains(&"MIKE_DATABASE_URL"), "{names:?}");
    Ok(())
}
