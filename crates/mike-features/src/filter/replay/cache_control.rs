use std::ops::Range;

use serde::Deserialize;
use serde_json::Value;
use serde_json::value::RawValue;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ControlMap {
    source_blocks: Vec<Value>,
    replacement_controls: Vec<Option<Range<usize>>>,
}

#[derive(Deserialize)]
struct ControlField<'a> {
    #[serde(borrow, default)]
    cache_control: Option<&'a RawValue>,
}

fn without_control(block: &RawValue) -> Option<Value> {
    let mut value: Value = serde_json::from_str(block.get()).ok()?;
    value.as_object_mut()?.remove("cache_control");
    Some(value)
}

fn shapes(blocks: &[&RawValue]) -> Option<Vec<Value>> {
    blocks.iter().map(|b| without_control(b)).collect()
}

fn control_span(base: &[u8], block: &RawValue) -> Option<Option<Range<usize>>> {
    let field: ControlField<'_> = serde_json::from_str(block.get()).ok()?;
    let Some(control) = field.cache_control else {
        return Some(None);
    };
    let text = control.get();
    let start = (text.as_ptr() as usize).checked_sub(base.as_ptr() as usize)?;
    let end = start.checked_add(text.len())?;
    (end <= base.len()).then_some(Some(start..end))
}

fn relative(span: Option<Range<usize>>, origin: usize) -> Option<Option<Range<usize>>> {
    match span {
        Some(r) => Some(Some(
            r.start.checked_sub(origin)?..r.end.checked_sub(origin)?,
        )),
        None => Some(None),
    }
}

impl ControlMap {
    pub(super) fn capture(
        source: &[&RawValue],
        filtered: &[u8],
        replacement_start: usize,
        replacement: &[&RawValue],
    ) -> Option<Self> {
        let replacement_controls = replacement
            .iter()
            .map(|b| relative(control_span(filtered, b)?, replacement_start))
            .collect::<Option<Vec<_>>>()?;
        Some(Self {
            source_blocks: shapes(source)?,
            replacement_controls,
        })
    }

    pub(super) fn retarget(
        &self,
        replacement: &[u8],
        raw: &[u8],
        incoming: &[&RawValue],
    ) -> Option<Vec<u8>> {
        if shapes(incoming)? != self.source_blocks {
            return None;
        }
        if self.replacement_controls.iter().all(Option::is_none) {
            return Some(replacement.to_vec());
        }
        if self.replacement_controls.len() != incoming.len() {
            return None;
        }
        let mut out = Vec::with_capacity(replacement.len());
        let mut cursor = 0;
        for (control, block) in self.replacement_controls.iter().zip(incoming) {
            match (control, control_span(raw, block)?) {
                (Some(at), Some(from)) => {
                    out.extend_from_slice(replacement.get(cursor..at.start)?);
                    out.extend_from_slice(raw.get(from)?);
                    cursor = at.end;
                }
                (None, None) => {}
                _ => return None,
            }
        }
        out.extend_from_slice(replacement.get(cursor..)?);
        Some(out)
    }
}
