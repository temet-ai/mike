use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub(super) fn edit_text(value: &mut String, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Char(ch) => value.push(ch),
        KeyCode::Backspace => {
            value.pop();
        }
        _ => return false,
    }
    true
}

pub(super) fn is_shortcuts_toggle(key: &KeyEvent, text_input: bool) -> bool {
    match key.code {
        KeyCode::Char('?') => !text_input,
        KeyCode::Char('/') => key.modifiers.contains(KeyModifiers::CONTROL),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_characters_are_literal_and_backspace_removes_one_character() {
        let text = "sq? $ ' \" ` \\ # 雪";
        let mut value = String::new();
        for ch in text.chars() {
            assert!(edit_text(
                &mut value,
                KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE)
            ));
        }
        assert_eq!(value, text);
        assert!(edit_text(
            &mut value,
            KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE)
        ));
        assert_eq!(value, text.trim_end_matches('雪'));
        for code in [KeyCode::F(2), KeyCode::Esc, KeyCode::Enter, KeyCode::Tab] {
            assert!(!edit_text(
                &mut value,
                KeyEvent::new(code, KeyModifiers::NONE)
            ));
        }
    }

    #[test]
    fn question_mark_is_literal_during_input_and_control_help_remains_available() {
        let question = KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE);
        let help = KeyEvent::new(KeyCode::Char('/'), KeyModifiers::CONTROL);
        assert!(!is_shortcuts_toggle(&question, true));
        assert!(is_shortcuts_toggle(&question, false));
        assert!(is_shortcuts_toggle(&help, true));
        assert!(is_shortcuts_toggle(&help, false));
    }
}
