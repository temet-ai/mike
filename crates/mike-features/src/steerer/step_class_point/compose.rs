use super::questions::CLASS;
use crate::llm::decisions::point::Asked;
use crate::steerer::guard::step_class::StepClass;

pub const CHOOSE_AT: f64 = 0.5;

pub fn compose(asked: &Asked) -> Option<(StepClass, f64)> {
    if asked.failed() {
        return None;
    }
    let (key, probability) = asked.choice(CLASS)?;
    let class = StepClass::from_code(key)?;
    (probability >= CHOOSE_AT).then_some((class, probability))
}
