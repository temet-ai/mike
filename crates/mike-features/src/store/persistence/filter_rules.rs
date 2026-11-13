use chrono::{DateTime, Utc};

use super::PersistError;
use crate::filter::{FilterAction, FilterKind, FilterRule, RuleSource};

#[derive(sqlx::FromRow)]
struct FilterRuleRow {
    id: i64,
    uuid: uuid::Uuid,
    kind: String,
    value: String,
    action: String,
    text: Option<String>,
    trigger_token: Option<String>,
    inject_text: Option<String>,
    enabled: bool,
    source: String,
    updated_at: DateTime<Utc>,
    project_path: Option<String>,
    sort_order: i32,
    style_override: bool,
    title: Option<String>,
}

impl TryFrom<FilterRuleRow> for FilterRule {
    type Error = PersistError;
    fn try_from(row: FilterRuleRow) -> Result<Self, Self::Error> {
        let kind = str_to_kind(&row.kind).ok_or_else(|| {
            PersistError::DataInvariant(format!(
                "unknown filter_rules.kind {:?} for uuid {}",
                row.kind, row.uuid
            ))
        })?;
        let action = str_to_action(&row.action).ok_or_else(|| {
            PersistError::DataInvariant(format!(
                "unknown filter_rules.action {:?} for uuid {}",
                row.action, row.uuid
            ))
        })?;
        let source = str_to_source(&row.source).ok_or_else(|| {
            PersistError::DataInvariant(format!(
                "unknown filter_rules.source {:?} for uuid {}",
                row.source, row.uuid
            ))
        })?;
        let rule = FilterRule {
            id: row.id,
            uuid: row.uuid,
            kind,
            value: row.value,
            enabled: row.enabled,
            source,
            updated_at: row.updated_at,
            sort_order: row.sort_order,
            action,
            text: row.text,
            trigger_token: row.trigger_token,
            inject_text: row.inject_text,
            project_path: row.project_path,
            style_override: row.style_override,
            title: row.title,
        };
        rule.validate_structure()
            .map_err(PersistError::DataInvariant)?;
        Ok(rule)
    }
}

#[derive(sqlx::FromRow)]
pub struct SessionFilterRuleRow {
    pub rule_id: i64,
    pub kind: String,
    pub label: String,
    pub tag: Option<String>,
    pub enabled: bool,
    pub fired_turns: i64,
}

struct FiringStatsRow {
    id: i64,
    last_fired_at: Option<DateTime<Utc>>,
    fires_24h: i64,
    fires_7d: i64,
}

#[derive(Debug, Clone, Default)]
pub struct FilterRuleFiringStats {
    pub last_fired_at: Option<DateTime<Utc>>,
    pub fires_24h: i64,
    pub fires_7d: i64,
}

impl crate::store::persistence::proxy_repo::ProxyDb<'_> {
    pub async fn list_filter_rules(self) -> Result<Vec<FilterRule>, PersistError> {
        crate::metrics::time_db("list_filter_rules", async move {
            let rows = sqlx::query_as!(
                FilterRuleRow,
                r#"SELECT r.id AS "id!", r.uuid AS "uuid!", k.code AS "kind!", r.value AS "value!",
                          a.code AS "action!", r.text, r.trigger_token, r.inject_text, r.enabled AS "enabled!",
                          s.code AS "source!", r.updated_at AS "updated_at!",
                          r.project_path, r.sort_order AS "sort_order!",
                          r.style_override AS "style_override!", r.title
                   FROM rules.rules r
                   JOIN rules.kinds   k ON k.id = r.kind_id
                   JOIN rules.actions a ON a.id = r.action_id
                   JOIN rules.sources s ON s.id = r.source_id
                  WHERE r.deleted_at IS NULL
                  ORDER BY r.sort_order ASC, r.id ASC"#,
            )
            .fetch_all(&self.pool)
            .await?;

            rows.into_iter().map(FilterRule::try_from).collect()
        })
        .await
    }

    pub async fn insert_filter_rule(self, rule: &FilterRule) -> Result<(i64, i32), PersistError> {
        crate::metrics::time_db("insert_filter_rule", async move {
            let mut tx = self.pool.begin().await?;
            let row = sqlx::query!(
                r#"INSERT INTO rules.rules
                    (uuid, kind_id, value, action_id, text, trigger_token, inject_text, enabled, source_id, updated_at, project_path, sort_order, style_override, title)
                 SELECT $1, rk.id, $3, ra.id, $5, $6, $7, $8, rs.id, $10, $11,
                        (SELECT COALESCE(MAX(sort_order), 0) + 1 FROM rules.rules), $12, $13
                 FROM rules.kinds rk, rules.actions ra, rules.sources rs
                 WHERE rk.code = $2 AND ra.code = $4 AND rs.code = $9
                 RETURNING id, sort_order"#,
                rule.uuid,
                kind_to_str(rule.kind),
                &rule.value,
                action_to_str(rule.action),
                rule.text.as_deref(),
                rule.trigger_token.as_deref(),
                rule.inject_text.as_deref(),
                rule.enabled,
                source_to_str(rule.source),
                rule.updated_at,
                rule.project_path.as_deref(),
                rule.style_override,
                rule.title.as_deref(),
            )
            .fetch_one(&mut *tx)
            .await?;

            tx.commit().await?;
            Ok((row.id, row.sort_order))
        })
        .await
    }

    pub async fn update_filter_rule(self, rule: &FilterRule) -> Result<bool, PersistError> {
        crate::metrics::time_db("update_filter_rule", async move {
            let result = sqlx::query!(
                "UPDATE rules.rules r
                 SET kind_id       = rk.id,
                     value         = $3,
                     action_id     = ra.id,
                     text          = $5,
                     trigger_token = $6,
                     inject_text   = $7,
                     enabled       = $8,
                     source_id     = rs.id,
                     updated_at    = $10,
                     project_path  = $11,
                     sort_order    = $12,
                     style_override = $13,
                     title         = $14
                FROM rules.kinds   rk,
                     rules.actions ra,
                     rules.sources rs
                WHERE rk.code = $2
                  AND ra.code = $4
                  AND rs.code = $9
                  AND r.id    = $1",
                rule.id,
                kind_to_str(rule.kind),
                &rule.value,
                action_to_str(rule.action),
                rule.text.as_deref(),
                rule.trigger_token.as_deref(),
                rule.inject_text.as_deref(),
                rule.enabled,
                source_to_str(rule.source),
                rule.updated_at,
                rule.project_path.as_deref(),
                rule.sort_order,
                rule.style_override,
                rule.title.as_deref(),
            )
            .execute(&self.pool)
            .await?;
            Ok(result.rows_affected() > 0)
        })
        .await
    }

    pub async fn delete_filter_rule(self, id: i64) -> Result<bool, PersistError> {
        crate::metrics::time_db("delete_filter_rule", async move {
            let result = sqlx::query!(
                "UPDATE rules.rules
                 SET enabled = false,
                     deleted_at = NOW(),
                     updated_at = NOW()
                 WHERE id = $1
                   AND deleted_at IS NULL",
                id,
            )
            .execute(&self.pool)
            .await?;
            Ok(result.rows_affected() > 0)
        })
        .await
    }

    pub async fn fetch_session_filter_rules(
        self,
        session_uuid: uuid::Uuid,
    ) -> Result<Vec<SessionFilterRuleRow>, PersistError> {
        crate::metrics::time_db("fetch_session_filter_rules", async move {
            let rows = sqlx::query_as!(
                SessionFilterRuleRow,
                r#"SELECT r.id AS "rule_id!", k.code AS "kind!", r.value AS "label!",
                          r.trigger_token AS "tag", r.enabled AS "enabled!",
                          COUNT(DISTINCT sp.request_id) AS "fired_turns!"
                     FROM requests.system_prompt_firing_rules f
                     JOIN requests.system_prompts sp ON sp.id = f.request_system_prompt_id
                     JOIN requests.requests req ON req.id = sp.request_id
                     JOIN sessions.sessions s ON s.id = req.session_id
                     JOIN rules.rules r ON r.id = f.filter_rule_id
                     JOIN rules.kinds k ON k.id = r.kind_id
                    WHERE s.session_uuid = $1
                    GROUP BY r.id, k.code, r.value, r.trigger_token, r.enabled
                    ORDER BY r.id ASC"#,
                session_uuid,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn session_system_prompt_is_filtered(
        self,
        session_uuid: uuid::Uuid,
    ) -> Result<bool, PersistError> {
        crate::metrics::time_db("filter_session_system_prompt_is_filtered", async move {
            let filtered = sqlx::query_scalar!(
                r#"WITH latest AS (
                       SELECT sp.id
                         FROM requests.requests q
                         JOIN sessions.sessions s ON s.id = q.session_id
                         JOIN requests.turn_origins o ON o.id = q.turn_origin_id
                         JOIN requests.system_prompts sp ON sp.request_id = q.id
                        WHERE s.session_uuid = $1
                          AND o.code IN ('primary', 'primary_first')
                        ORDER BY q.created_at DESC, q.id DESC
                        LIMIT 1
                   )
                   SELECT count(f.id) > 0 AS "filtered!"
                     FROM latest
                LEFT JOIN requests.system_prompt_firing_rules f
                       ON f.request_system_prompt_id = latest.id"#,
                session_uuid,
            )
            .fetch_one(&self.pool)
            .await?;
            Ok(filtered)
        })
        .await
    }

    pub async fn fetch_filter_rule_firing_stats_all(
        self,
    ) -> Result<std::collections::HashMap<i64, FilterRuleFiringStats>, PersistError> {
        crate::metrics::time_db("fetch_filter_rule_firing_stats_all", async move {
            let rows = sqlx::query_as!(
                FiringStatsRow,
                r#"SELECT
                     fr.id AS "id!",
                     MAX(recent.created_at) AS last_fired_at,
                     COUNT(*) FILTER (WHERE recent.created_at >= NOW() - INTERVAL '1 day')::BIGINT AS "fires_24h!",
                     COUNT(recent.created_at)::BIGINT AS "fires_7d!"
                 FROM rules.rules fr
                 LEFT JOIN (
                     SELECT j.filter_rule_id, r.created_at
                     FROM requests.requests r
                     JOIN requests.system_prompts rsp ON rsp.request_id = r.id
                     JOIN requests.system_prompt_firing_rules j ON j.request_system_prompt_id = rsp.id
                     WHERE r.created_at >= NOW() - INTERVAL '7 days'
                 ) recent ON recent.filter_rule_id = fr.id
                 GROUP BY fr.id"#,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows
                .into_iter()
                .map(|row| {
                    (
                        row.id,
                        FilterRuleFiringStats {
                            last_fired_at: row.last_fired_at,
                            fires_24h: row.fires_24h,
                            fires_7d: row.fires_7d,
                        },
                    )
                })
                .collect())
        })
        .await
    }

    pub async fn fetch_filter_rule_firing_stats(
        self,
        rule_id: i64,
    ) -> Result<FilterRuleFiringStats, PersistError> {
        crate::metrics::time_db("fetch_filter_rule_firing_stats", async move {
            let stats = sqlx::query_as!(
                FilterRuleFiringStats,
                r#"SELECT
                         MAX(recent.created_at) AS last_fired_at,
                         COUNT(*) FILTER (WHERE recent.created_at >= NOW() - INTERVAL '1 day')::BIGINT AS "fires_24h!",
                         COUNT(recent.created_at)::BIGINT AS "fires_7d!"
                     FROM (
                         SELECT r.created_at
                         FROM requests.requests r
                         JOIN requests.system_prompts rsp ON rsp.request_id = r.id
                         JOIN requests.system_prompt_firing_rules j
                             ON j.request_system_prompt_id = rsp.id AND j.filter_rule_id = $1
                         WHERE r.created_at >= NOW() - INTERVAL '7 days'
                     ) recent"#,
                rule_id,
            )
            .fetch_one(&self.pool)
            .await?;
            Ok(stats)
        })
        .await
    }

    pub async fn fetch_latest_primary_first_request_id(
        self,
        project_path: Option<&str>,
    ) -> Result<Option<i64>, PersistError> {
        crate::metrics::time_db("fetch_latest_primary_first_request_id", async move {
            let id = match project_path {
                Some(path) => {
                    sqlx::query_scalar!(
                        r#"SELECT r.id
                           FROM requests.requests r
                           JOIN requests.turn_origins tor
                             ON tor.id = r.turn_origin_id AND tor.code = 'primary_first'
                           JOIN projects.projects p ON p.id = r.project_id
                          WHERE p.project_path = $1
                          ORDER BY r.created_at DESC
                          LIMIT 1"#,
                        path,
                    )
                    .fetch_optional(&self.pool)
                    .await?
                }
                None => {
                    sqlx::query_scalar!(
                        r#"SELECT r.id
                           FROM requests.requests r
                           JOIN requests.turn_origins tor
                             ON tor.id = r.turn_origin_id AND tor.code = 'primary_first'
                           JOIN projects.projects p ON p.id = r.project_id
                          ORDER BY r.created_at DESC
                          LIMIT 1"#,
                    )
                    .fetch_optional(&self.pool)
                    .await?
                }
            };
            Ok(id)
        })
        .await
    }
}

fn kind_to_str(kind: FilterKind) -> &'static str {
    match kind {
        FilterKind::TextSubstring => "text_substring",
        FilterKind::XmlTag => "xml_tag",
        FilterKind::LazySection => "lazy_section",
        FilterKind::InjectReminder => "inject_reminder",
    }
}

fn str_to_kind(raw: &str) -> Option<FilterKind> {
    match raw {
        "text_substring" => Some(FilterKind::TextSubstring),
        "xml_tag" => Some(FilterKind::XmlTag),
        "lazy_section" => Some(FilterKind::LazySection),
        "inject_reminder" => Some(FilterKind::InjectReminder),
        _ => None,
    }
}

fn action_to_str(action: FilterAction) -> &'static str {
    match action {
        FilterAction::Drop => "drop",
        FilterAction::Replace => "replace",
        FilterAction::Append => "append",
    }
}

fn str_to_action(raw: &str) -> Option<FilterAction> {
    match raw {
        "drop" => Some(FilterAction::Drop),
        "replace" => Some(FilterAction::Replace),
        "append" => Some(FilterAction::Append),
        _ => None,
    }
}

fn source_to_str(source: RuleSource) -> &'static str {
    match source {
        RuleSource::Toml => "toml",
        RuleSource::Ui => "ui",
    }
}

fn str_to_source(raw: &str) -> Option<RuleSource> {
    match raw {
        "toml" => Some(RuleSource::Toml),
        "ui" => Some(RuleSource::Ui),
        _ => None,
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "filter_rules/junction_tests.rs"]
pub mod junction_tests;
