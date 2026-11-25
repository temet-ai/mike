use uuid::Uuid;

use crate::llm::decisions::text::tail_chars;
use crate::memory_keeper::resolve_text::typed_words;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::lessons_repo::LessonsPersist as _;

const EARLIER_CHARS: usize = 600;

#[derive(Debug, Default, Clone)]
pub struct PromptContext {
    pub earlier_ask: String,
    pub last_action: String,
}

impl PromptContext {
    pub async fn load(persist: Option<&PersistHandle>, session: Uuid) -> Self {
        let Some(persist) = persist else {
            return Self::default();
        };
        match persist.lessons().latest_turn_texts(session).await {
            Ok(Some((ask, answer))) => Self::from_turn(&ask, &answer),
            Ok(None) => Self::default(),
            Err(error) => {
                tracing::warn!(%error, "earlier turn unavailable; matching lessons by the prompt alone");
                Self::default()
            }
        }
    }

    pub fn from_turn(ask: &str, answer: &str) -> Self {
        Self {
            earlier_ask: tail_chars(&typed_words(ask), EARLIER_CHARS).to_owned(),
            last_action: tail_chars(answer.trim(), EARLIER_CHARS).to_owned(),
        }
    }

    pub fn query(&self, prompt: &str) -> String {
        let typed = typed_words(prompt);
        [
            self.earlier_ask.as_str(),
            self.last_action.as_str(),
            typed.as_str(),
        ]
        .into_iter()
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::PromptContext;

    #[test]
    fn query_puts_the_prompt_after_the_earlier_turn() {
        let context = PromptContext::from_turn("make the images", " Made three. ");
        assert_eq!(
            context.query("redo it"),
            "make the images\nMade three.\nredo it"
        );
    }

    #[test]
    fn query_without_an_earlier_turn_is_the_prompt() {
        assert_eq!(PromptContext::default().query("redo it"), "redo it");
    }
}
