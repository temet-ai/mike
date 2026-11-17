use std::borrow::Cow;

use serde_json::Value;

use crate::conversation::EarlierTurns;

pub fn conversation<'a>(messages: &'a [Value], earlier: &EarlierTurns) -> Cow<'a, [Value]> {
    if earlier.is_empty() {
        Cow::Borrowed(messages)
    } else {
        Cow::Owned(earlier.conversation_with(messages))
    }
}

#[cfg(test)]
pub(crate) fn earlier_from_messages(messages: &[Value]) -> anyhow::Result<EarlierTurns> {
    let raw = serde_json::to_vec(&serde_json::json!({ "messages": messages }))?;
    let parsed = crate::conversation::body_messages(&raw)
        .ok_or_else(|| anyhow::anyhow!("earlier messages did not parse"))?;
    Ok(EarlierTurns::from_chain(std::sync::Arc::from(parsed)))
}
