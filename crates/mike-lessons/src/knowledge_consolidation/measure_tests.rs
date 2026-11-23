use super::measure::{CEILING, FLOOR, next_similarity, share, steer};
use super::tally::Roles;

#[test]
fn too_many_elsewhere_raises_and_too_many_found_outside_lowers() {
    assert!((steer(0.70, 0.20, 0.0) - 0.71).abs() < 1e-6);
    assert!((steer(0.70, 0.0, 0.40) - 0.69).abs() < 1e-6);
    assert!((steer(0.70, 0.10, 0.20) - 0.70).abs() < 1e-6);
}

#[test]
fn the_step_is_clamped_to_the_allowed_range() {
    assert!((steer(CEILING, 0.5, 0.0) - CEILING).abs() < 1e-6);
    assert!((steer(FLOOR, 0.0, 0.9) - FLOOR).abs() < 1e-6);
    assert!((steer(0.50, 0.0, 0.0) - FLOOR).abs() < 1e-6);
}

#[test]
fn when_both_errors_are_over_the_larger_relative_excess_wins() {
    assert!((steer(0.70, 0.50, 0.40) - 0.71).abs() < 1e-6);
    assert!((steer(0.70, 0.16, 0.90) - 0.69).abs() < 1e-6);
}

#[test]
fn next_similarity_reads_the_shares_from_the_role_counts() {
    let roles = Roles {
        members: 10,
        elsewhere: 3,
        covered: 4,
        covered_outside: 1,
    };
    let (next, too_tight) = next_similarity(0.70, &roles);
    assert!((next - 0.71).abs() < 1e-6);
    assert!((too_tight - 0.25).abs() < 1e-6);
    assert!(share(0, 0).abs() < f32::EPSILON);
}
