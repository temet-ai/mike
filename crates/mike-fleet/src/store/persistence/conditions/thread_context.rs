use super::types::ConditionError;
use crate::store::persistence::fleet_repo::FleetDb;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use crate::store::persistence::work_threads::{HistoryPage, ThreadDetail, WorkThreadError};

const HISTORY_READ: i64 = 20;
const HANDOFF_KINDS: [&str; 2] = ["handoff", "brief"];

fn labelled(out: &mut String, label: &str, value: &str) {
    let value = value.trim();
    if !value.is_empty() {
        out.push_str(&format!("{label}: {value}\n"));
    }
}

pub fn render_thread_context(detail: &ThreadDetail) -> String {
    let mut out = String::new();
    let title = detail.thread.title.as_deref().unwrap_or("");
    out.push_str(&format!("Work thread `{}` {title}\n", detail.thread.slug));
    if let Some(state) = &detail.state {
        labelled(&mut out, "tldr", &state.tldr);
        labelled(&mut out, "where we are", &state.where_we_are);
        labelled(&mut out, "next up", &state.next_up);
    }
    let newest = detail
        .history
        .iter()
        .find(|entry| HANDOFF_KINDS.contains(&entry.kind.as_str()));
    if let Some(entry) = newest {
        out.push_str(&format!(
            "newest {} entry ({}): {}\n{}\n",
            entry.kind,
            entry.entry_date,
            entry.summary.trim(),
            entry.body.trim()
        ));
    }
    out.trim_end().to_owned()
}

impl FleetDb<'_> {
    pub async fn condition_thread_context(
        self,
        project_id: i64,
        slug: &str,
    ) -> Result<Option<String>, ConditionError> {
        let page = HistoryPage {
            limit: HISTORY_READ,
            before_ord: None,
        };
        match self.launch().get_work_thread(project_id, slug, page).await {
            Ok(detail) => Ok(Some(render_thread_context(&detail))),
            Err(WorkThreadError::NotFound(_)) => Ok(None),
            Err(WorkThreadError::Invalid(message)) => Err(ConditionError::Invalid(message)),
            Err(WorkThreadError::Persist(e)) => Err(ConditionError::Persist(e)),
        }
    }
}
