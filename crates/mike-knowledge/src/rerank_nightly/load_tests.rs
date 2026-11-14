use super::load::{load_per_cpu, one_minute, one_minute_from_sysctl, under_ceiling};

#[test]
fn one_minute_from_sysctl_reads_the_first_field_inside_the_braces() {
    assert_eq!(one_minute_from_sysctl("{ 1.83 2.01 2.10 }\n"), Some(1.83));
    assert_eq!(one_minute_from_sysctl("{0.5 0.4 0.3}"), Some(0.5));
}

#[test]
fn one_minute_from_sysctl_unparsable_output_is_none() {
    assert_eq!(one_minute_from_sysctl(""), None);
    assert_eq!(one_minute_from_sysctl("{ }"), None);
    assert_eq!(one_minute_from_sysctl("sysctl: unknown oid"), None);
}

#[test]
fn one_minute_reads_the_first_field_of_proc_loadavg() {
    assert_eq!(one_minute("12.32 7.24 4.48 3/2917 3786592\n"), Some(12.32));
    assert_eq!(one_minute(""), None);
    assert_eq!(one_minute("busy"), None);
}

#[test]
fn load_per_cpu_reads_a_non_negative_average_on_this_machine() {
    let load = load_per_cpu();

    assert!(load.is_some_and(|load| load >= 0.0), "{load:?}");
}

#[test]
fn under_ceiling_compares_the_load_with_the_ceiling() {
    assert!(under_ceiling(Some(0.4), 0.5));
    assert!(under_ceiling(Some(0.5), 0.5));
    assert!(!under_ceiling(Some(0.6), 0.5));
}

#[test]
fn under_ceiling_with_an_unreadable_load_treats_the_machine_as_busy() {
    assert!(!under_ceiling(None, f64::MAX));
}
