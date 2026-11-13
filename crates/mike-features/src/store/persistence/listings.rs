use uuid::Uuid;

use super::PersistError;
use super::rehydrate;

pub enum ProjectSelector<'a> {
    Uuid(Uuid),
    Path(&'a str),
}

pub struct ProjectDetailRow {
    pub project: rehydrate::ProjectDetailHead,
    pub locations: Vec<rehydrate::ProjectLocationRow>,
}

pub struct ListingWindow {
    pub order: crate::types::SortOrder,
    pub date_from: Option<chrono::DateTime<chrono::Utc>>,
    pub date_to: Option<chrono::DateTime<chrono::Utc>>,
}

pub struct SessionListFilters<'a> {
    pub search: Option<&'a str>,
    pub min_request_count: Option<i64>,
    pub project: Option<&'a str>,
}

impl crate::store::persistence::launch_repo::LaunchDb<'_> {
    pub async fn list_requests_paginated(
        self,
        offset: usize,
        limit: usize,
        prompt_sha256: Option<&str>,
        window: &ListingWindow,
        cursor: Option<super::keyset::KeysetCursor>,
    ) -> Result<
        (
            Vec<crate::store::request_summary::RequestSummaryView>,
            Option<super::keyset::KeysetCursor>,
        ),
        PersistError,
    > {
        crate::metrics::time_db(
            "list_requests_paginated",
            rehydrate::list_requests_paginated(
                &self.pool,
                rehydrate::PageWindow {
                    offset,
                    limit,
                    order: window.order,
                    date_from: window.date_from,
                    date_to: window.date_to,
                    cursor,
                },
                prompt_sha256,
            ),
        )
        .await
    }

    pub async fn list_sessions_paginated_with_search(
        self,
        offset: usize,
        limit: usize,
        search: Option<&str>,
        window: &ListingWindow,
    ) -> Result<Vec<rehydrate::SessionSummaryRow>, PersistError> {
        crate::metrics::time_db(
            "list_sessions_paginated_with_search",
            rehydrate::list_sessions_paginated_with_search(
                &self.pool,
                rehydrate::PageWindow {
                    offset,
                    limit,
                    order: window.order,
                    date_from: window.date_from,
                    date_to: window.date_to,
                    cursor: None,
                },
                search,
            ),
        )
        .await
    }

    pub async fn list_sessions_by_ids(
        self,
        ids: &[uuid::Uuid],
    ) -> Result<Vec<rehydrate::SessionSummaryRow>, PersistError> {
        crate::metrics::time_db(
            "list_sessions_by_ids",
            rehydrate::list_sessions_by_ids(&self.pool, ids),
        )
        .await
    }

    pub async fn list_sessions_paginated_filtered(
        self,
        offset: usize,
        limit: usize,
        filters: &SessionListFilters<'_>,
        window: &ListingWindow,
    ) -> Result<Vec<rehydrate::SessionSummaryRow>, PersistError> {
        crate::metrics::time_db(
            "list_sessions_paginated_filtered",
            rehydrate::list_sessions_paginated_filtered(
                &self.pool,
                rehydrate::SessionListFilterParams {
                    window: rehydrate::PageWindow {
                        offset,
                        limit,
                        order: window.order,
                        date_from: window.date_from,
                        date_to: window.date_to,
                        cursor: None,
                    },
                    search: filters.search,
                    min_request_count: filters.min_request_count,
                    project: filters.project,
                },
            ),
        )
        .await
    }

    pub async fn list_session_request_ids_asc(
        self,
        session_db_id: i64,
    ) -> Result<Vec<rehydrate::SessionRequestIdRow>, PersistError> {
        crate::metrics::time_db(
            "list_session_request_ids_asc",
            rehydrate::list_session_request_ids_asc(&self.pool, session_db_id),
        )
        .await
    }

    pub async fn list_projects(
        self,
    ) -> Result<Vec<rehydrate::ProjectListRow>, PersistError> {
        crate::metrics::time_db("list_projects", rehydrate::list_projects(&self.pool)).await
    }

    pub async fn list_project_counts(
        self,
    ) -> Result<Vec<rehydrate::ProjectCountsRow>, PersistError> {
        crate::metrics::time_db(
            "list_project_counts",
            rehydrate::list_project_counts(&self.pool),
        )
        .await
    }

    pub async fn fetch_project_detail(
        self,
        selector: ProjectSelector<'_>,
    ) -> Result<Option<ProjectDetailRow>, PersistError> {
        let project_id = match selector {
            ProjectSelector::Uuid(uuid) => {
                rehydrate::find_project_id_by_uuid(&self.pool, uuid).await?
            }
            ProjectSelector::Path(path) => self.find_project_id_by_path(path).await?,
        };
        let Some(project_id) = project_id else {
            return Ok(None);
        };
        crate::metrics::time_db("fetch_project_detail", async {
            let Some(project) = rehydrate::fetch_project_row(&self.pool, project_id).await? else {
                return Ok(None);
            };
            let locations = rehydrate::list_project_location_rows(&self.pool, project_id).await?;
            Ok(Some(ProjectDetailRow { project, locations }))
        })
        .await
    }

    pub async fn count_projects(self) -> Result<i64, PersistError> {
        crate::metrics::time_db("count_projects", rehydrate::count_projects(&self.pool)).await
    }

    pub async fn fetch_session_head(
        self,
        session_db_id: i64,
    ) -> Result<Option<rehydrate::SessionHeadRow>, PersistError> {
        crate::metrics::time_db(
            "fetch_session_head",
            rehydrate::fetch_session_head(&self.pool, session_db_id),
        )
        .await
    }

    pub async fn fetch_session_prompt_kind_counts(
        self,
        session_db_id: i64,
    ) -> Result<Vec<rehydrate::PromptKindCountRow>, PersistError> {
        crate::metrics::time_db(
            "fetch_session_prompt_kind_counts",
            rehydrate::fetch_session_prompt_kind_counts(&self.pool, session_db_id),
        )
        .await
    }
}
