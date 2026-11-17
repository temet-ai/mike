use std::str::FromStr;

use super::LESSON_TAG_OPEN;
use crate::conversation::EarlierTurns;
use crate::lesson_worker::InjectableLesson;

pub const REVISION_MARK: &str = " rev:";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeldTag {
    pub id: i64,
    pub revision: Option<i32>,
}

impl HeldTag {
    pub fn holds(&self, lesson: &InjectableLesson) -> bool {
        self.id == lesson.id && self.revision.is_none_or(|held| held == lesson.revision)
    }
}

pub fn is_held(tags: &[HeldTag], lesson: &InjectableLesson) -> bool {
    tags.iter().any(|tag| tag.holds(lesson))
}

pub struct TagMatch {
    pub tag: HeldTag,
    pub end: usize,
}

fn number_at<T: FromStr>(body: &[u8], at: usize) -> Option<(T, usize)> {
    let digits = body
        .get(at..)?
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    if digits == 0 {
        return None;
    }
    let end = at + digits;
    let value = std::str::from_utf8(body.get(at..end)?).ok()?.parse().ok()?;
    Some((value, end))
}

pub fn parse_tag(body: &[u8], start: usize) -> Option<TagMatch> {
    let open = LESSON_TAG_OPEN.as_bytes();
    let id_at = start + open.len();
    if body.get(start..id_at)? != open {
        return None;
    }
    let (id, after_id) = number_at::<i64>(body, id_at)?;
    let mark = REVISION_MARK.as_bytes();
    let (revision, close) = if body.get(after_id..after_id + mark.len()) == Some(mark) {
        let (revision, close) = number_at::<i32>(body, after_id + mark.len())?;
        (Some(revision), close)
    } else {
        (None, after_id)
    };
    (body.get(close) == Some(&b']')).then_some(TagMatch {
        tag: HeldTag { id, revision },
        end: close + 1,
    })
}

pub fn tag_at(body: &[u8], start: usize) -> Option<(i64, usize)> {
    parse_tag(body, start).map(|found| (found.tag.id, found.end))
}

pub fn held_lesson_tags(body: &[u8], earlier: &EarlierTurns) -> Vec<HeldTag> {
    let mut tags = Vec::new();
    for message in earlier.raw() {
        collect_tags(message.get().as_bytes(), &mut tags);
    }
    collect_tags(body, &mut tags);
    tags
}

fn collect_tags(body: &[u8], tags: &mut Vec<HeldTag>) {
    let open = LESSON_TAG_OPEN.as_bytes();
    let mut from = 0;
    while let Some(offset) = find(body.get(from..).unwrap_or_default(), open) {
        let start = from + offset;
        if let Some(found) = parse_tag(body, start)
            && !tags.contains(&found.tag)
        {
            tags.push(found.tag);
        }
        from = start + open.len();
    }
}

pub fn held_lesson_ids(body: &[u8]) -> Vec<i64> {
    let mut ids = Vec::new();
    for tag in held_lesson_tags(body, &crate::conversation::WHOLE_BODY) {
        if !ids.contains(&tag.id) {
            ids.push(tag.id);
        }
    }
    ids
}

pub fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}
