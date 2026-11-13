const MIKE_OPENERS: [&str; 6] = [
    "Coordinator wake",
    "coordinator wake",
    "A session-scoped Stop hook",
    "Stop hook feedback:",
    "/goal",
    "Your brief is saved at ",
];

pub fn typed_by_person(prompt: &str, seats: &[String]) -> bool {
    let text = prompt.trim_start();
    !text.is_empty()
        && crate::wire::genuine::is_genuine_text(text)
        && !MIKE_OPENERS.iter().any(|opener| text.starts_with(opener))
        && !seats.iter().any(|seat| names_seat(text, seat))
}

fn names_seat(text: &str, seat: &str) -> bool {
    !seat.is_empty()
        && text
            .strip_prefix(seat)
            .and_then(|rest| rest.chars().next())
            .is_some_and(|next| matches!(next, ':' | '.' | '-'))
}

#[cfg(test)]
#[path = "seat_prompts_tests.rs"]
mod tests;
