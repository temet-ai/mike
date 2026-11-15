use std::collections::BTreeSet;

use super::fetch::{is_missing_object_code, recall_markers};
use super::queries::{Binds, SPECS};

fn highest_placeholder(sql: &str) -> usize {
    sql.split('$')
        .skip(1)
        .filter_map(|rest| {
            let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
            digits.parse::<usize>().ok()
        })
        .max()
        .unwrap_or(0)
}

#[test]
fn specs_reference_exactly_the_placeholders_their_binds_supply() {
    for spec in SPECS {
        let supplied = match spec.binds {
            Binds::None => 0,
            Binds::Start => 1,
            Binds::Window => 2,
            Binds::Recall => 6,
        };
        assert_eq!(
            highest_placeholder(spec.sql),
            supplied,
            "query {} binds {:?}",
            spec.name,
            spec.binds
        );
    }
}

#[test]
fn specs_have_unique_names_and_wrap_one_row_per_object() {
    let names: BTreeSet<&str> = SPECS.iter().map(|spec| spec.name).collect();
    assert_eq!(names.len(), SPECS.len());
    for spec in SPECS {
        assert!(
            spec.sql.starts_with("SELECT row_to_json(a) FROM ("),
            "query {} is not wrapped",
            spec.name
        );
    }
}

#[test]
fn specs_that_read_a_later_migration_are_optional() {
    let optional: BTreeSet<&str> = SPECS
        .iter()
        .filter(|spec| spec.optional)
        .map(|spec| spec.name)
        .collect();
    let expected = BTreeSet::from([
        "lesson_shape",
        "lesson_shape_injected",
        "lesson_shape_pending",
        "lesson_shape_resolved",
        "lesson_relations_write_time",
        "lesson_replacement_effect",
        "lesson_placement",
        "lesson_placement_stuck",
        "lesson_placement_proposals",
        "skill_live",
    ]);
    assert_eq!(optional, expected);
}

#[test]
fn recall_query_binds_the_stage_markers_the_injector_records() {
    let recall = SPECS
        .iter()
        .find(|spec| spec.binds == Binds::Recall)
        .map(|spec| spec.name);
    assert_eq!(recall, Some("memory_recall_tags"));
    let markers = recall_markers().map(|marker| marker.to_string());
    assert!(markers[1].contains("recall_no_tags"));
    assert!(markers[2].contains("recall_lessons_injected"));
    assert!(markers[3].contains("recall_delivered_out_of_band"));
    assert!(
        markers
            .iter()
            .all(|marker| marker.contains("recall_injector"))
    );
}

#[test]
fn missing_object_codes_cover_tables_columns_and_schemas_only() {
    for code in ["42P01", "42703", "3F000"] {
        assert!(is_missing_object_code(code), "{code}");
    }
    for code in ["42601", "57014", "25006", "08006"] {
        assert!(!is_missing_object_code(code), "{code}");
    }
}
