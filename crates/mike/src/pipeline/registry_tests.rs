use super::*;

#[test]
fn registry_follows_the_documented_stage_order() {
    let names: Vec<&str> = registry().iter().map(|s| s.name()).collect();
    let expected: Vec<&str> = STAGE_ORDER
        .iter()
        .copied()
        .filter(|n| *n != "steerer")
        .collect();
    assert_eq!(names, expected, "registry order drifted from STAGE_ORDER");
}

#[test]
fn registry_excludes_the_explicitly_invoked_stages() {
    let names: Vec<&str> = registry().iter().map(|s| s.name()).collect();
    assert!(!names.contains(&"steerer"), "steerer is invoked by name");
    assert!(!names.contains(&"composer"), "composer is invoked by name");
}
