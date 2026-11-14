use super::{allows, parse_meminfo, parse_vm_stat};

#[test]
fn meminfo_reports_available_memory_in_gib() {
    let text =
        "MemTotal:       49152000 kB\nMemFree:         1000000 kB\nMemAvailable:   10485760 kB\n";
    assert_eq!(parse_meminfo(text), Some(10.0));
    assert_eq!(parse_meminfo("MemTotal: 1 kB\n"), None);
}

#[test]
fn vm_stat_sums_free_inactive_and_speculative_pages() {
    let text = "Mach Virtual Memory Statistics: (page size of 16384 bytes)\nPages free:                               65536.\nPages active:                            100000.\nPages inactive:                           32768.\nPages speculative:                        32768.\n";
    assert_eq!(parse_vm_stat(text), Some(2.0));
    assert_eq!(parse_vm_stat("not vm_stat output"), None);
}

#[test]
fn a_memory_floor_holds_staffing_only_when_free_memory_is_known_and_not_above_it() {
    assert!(allows(None, Some(0.5)));
    assert!(allows(Some(4.0), None));
    assert!(allows(Some(4.0), Some(4.5)));
    assert!(!allows(Some(4.0), Some(4.0)));
    assert!(!allows(Some(4.0), Some(1.0)));
}
