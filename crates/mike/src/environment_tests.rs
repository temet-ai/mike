use super::*;

#[test]
fn map_environment_var_returns_the_value_it_was_given() {
    let env = MapEnvironment::new(&[("MIKE_A", "1")]);
    assert_eq!(env.var("MIKE_A").as_deref(), Some("1"));
}

#[test]
fn map_environment_var_for_an_unset_name_is_none() {
    let env = MapEnvironment::new(&[("MIKE_A", "1")]);
    assert_eq!(env.var("MIKE_B"), None);
}

#[test]
fn map_environment_with_replaces_an_earlier_value() {
    let env = MapEnvironment::new(&[("MIKE_A", "1")]).with("MIKE_A", "2");
    assert_eq!(env.var("MIKE_A").as_deref(), Some("2"));
}

#[test]
fn map_environment_vars_lists_every_entry_in_name_order() {
    let env = MapEnvironment::new(&[("MIKE_B", "2"), ("MIKE_A", "1")]);
    assert_eq!(
        env.vars(),
        vec![
            ("MIKE_A".to_owned(), "1".to_owned()),
            ("MIKE_B".to_owned(), "2".to_owned())
        ]
    );
}

#[test]
fn process_environment_var_for_a_name_nothing_sets_is_none() {
    assert_eq!(
        ProcessEnvironment.var("MIKE_ENVIRONMENT_TEST_NAME_NOTHING_SETS"),
        None
    );
}
