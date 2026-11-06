use crate::store::persistence::keyset::KeysetCursor;
use crate::types::SortOrder;

pub(super) fn limit_to_i64(v: usize) -> i64 {
    i64::try_from(v).unwrap_or(i64::MAX)
}

#[derive(Debug, Clone, Copy)]
pub struct PageWindow {
    pub offset: usize,
    pub limit: usize,
    pub order: SortOrder,
    pub date_from: Option<chrono::DateTime<chrono::Utc>>,
    pub date_to: Option<chrono::DateTime<chrono::Utc>>,
    pub cursor: Option<KeysetCursor>,
}

impl PageWindow {
    pub(super) fn effective_offset(&self) -> i64 {
        if self.cursor.is_some() {
            0
        } else {
            limit_to_i64(self.offset)
        }
    }

    pub(super) fn keyset_comparison(&self) -> &'static str {
        match self.order {
            SortOrder::Desc => "<",
            SortOrder::Asc => ">",
        }
    }
}
