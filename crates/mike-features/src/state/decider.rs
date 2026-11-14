use super::AppState;
use crate::llm::decisions::point::Decider;

impl AppState {
    pub fn decider(&self, point: &'static str) -> Option<Decider> {
        let persist = self.persist.as_ref()?;
        let base = self.judge_llm.as_ref()?;
        let keyed = crate::llm::key::keyed_client(base, &self.steerer_openrouter_key)?;
        let overlay = self.settings_overlay.load();
        Decider::for_point(
            &keyed,
            persist,
            self.feature_toggle_overlay.load().decisions,
            &overlay,
            point,
        )
    }
}
