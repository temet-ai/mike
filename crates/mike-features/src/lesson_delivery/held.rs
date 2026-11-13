use super::LESSON_TAG_OPEN;

pub fn tag_at(body: &[u8], start: usize) -> Option<(i64, usize)> {
    let tag = LESSON_TAG_OPEN.as_bytes();
    let digits_at = start + tag.len();
    if body.get(start..digits_at)? != tag {
        return None;
    }
    let digits = body
        .get(digits_at..)?
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    let close = digits_at + digits;
    if digits == 0 || body.get(close) != Some(&b']') {
        return None;
    }
    let id = std::str::from_utf8(body.get(digits_at..close)?)
        .ok()?
        .parse::<i64>()
        .ok()?;
    Some((id, close + 1))
}

pub fn held_lesson_ids(body: &[u8]) -> Vec<i64> {
    let tag = LESSON_TAG_OPEN.as_bytes();
    let mut ids = Vec::new();
    let mut from = 0;
    while let Some(offset) = find(body.get(from..).unwrap_or_default(), tag) {
        let start = from + offset;
        if let Some((id, _)) = tag_at(body, start)
            && !ids.contains(&id)
        {
            ids.push(id);
        }
        from = start + tag.len();
    }
    ids
}

pub fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}
