use super::typed_by_person;

fn seats() -> Vec<String> {
    ["coordinator", "acceptance", "acceptance-2", "ts-T4"]
        .map(String::from)
        .to_vec()
}

#[test]
fn prompts_mike_types_into_a_seat_are_not_a_person() {
    for prompt in [
        "ts-T4-cba0da45. Your brief is saved at /home/u/.local/state/mike/briefs/x.md - read it",
        "coordinator-970853a1. Your brief is saved at /tmp/b.md",
        "Coordinator wake for fleet-redesign-1 (6716aafe): run the wake loop",
        "A session-scoped Stop hook is now active with condition: \"Task T4\"",
        "/goal clear",
        "acceptance-2: (1) receipt 1528 result; (2) run the gate",
        "acceptance: fleet-redesign-1 — judge reports as they land",
        "Stop hook feedback: not met",
        "",
    ] {
        assert!(!typed_by_person(prompt, &seats()), "{prompt}");
    }
}

#[test]
fn prompts_a_person_types_count_even_when_they_name_a_seat_in_passing() {
    for prompt in [
        "why did the coordinator stop?",
        "coordinator, hold the merge until I look",
        "ship T4 now",
    ] {
        assert!(typed_by_person(prompt, &seats()), "{prompt}");
    }
}
