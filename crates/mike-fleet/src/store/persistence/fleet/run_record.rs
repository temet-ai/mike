use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

use crate::store::persistence::PersistError;

const RELAUNCH_EVENTS: &str = "
SELECT count(*)
  FROM launch.fleet_events
 WHERE run_id = $1
   AND kind = 'seat_closed'
   AND payload->>'reason' = $2
";

#[cfg(test)]
#[path = "run_record_tests.rs"]
mod tests;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SeatMeasure {
    pub seat: String,
    pub kind: String,
    pub requests: i64,
    pub working_minutes: f64,
    pub waiting_minutes: f64,
    pub tool_calls: i64,
    pub git_reads: i64,
    pub operation_commands: i64,
    pub minutes_to_first_commit: Option<f64>,
    pub calls_to_first_commit: Option<i64>,
    pub offered: Vec<String>,
    pub used: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ReportMeasure {
    pub reports: i64,
    pub answered: i64,
    pub median_minutes: Option<f64>,
    pub longest_minutes: Option<f64>,
    pub late_minutes: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TimingMeasure {
    pub time_to_done_minutes: f64,
    pub stop_minutes: f64,
    pub relaunch_gap_minutes: f64,
    pub no_seat_minutes: f64,
    pub seat_launches: i64,
    pub hand_starts: i64,
    pub task_count: i64,
    pub worker_launches: i64,
    pub relaunches: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TokenMeasure {
    pub kind: String,
    pub requests: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct InterventionMeasure {
    pub operator_turns: i64,
    pub mail_driven_turns: i64,
    pub operator_answers: i64,
    pub reports_filed: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MailPair {
    pub from_seat: String,
    pub to_seat: String,
    pub messages: i64,
    pub reached: i64,
    pub acknowledged: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct OperationMeasure {
    pub commands: i64,
    pub repeats: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct QuestionMeasure {
    pub asked: i64,
    pub answered: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ForkMeasure {
    pub forks: i64,
    pub waited: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RunMeasures {
    pub seats: Vec<SeatMeasure>,
    pub reports: ReportMeasure,
    pub mail: Vec<MailPair>,
    pub operations: OperationMeasure,
    pub questions: QuestionMeasure,
    pub forks: ForkMeasure,
    pub timing: TimingMeasure,
    pub tokens: Vec<TokenMeasure>,
    pub interventions: InterventionMeasure,
}

#[derive(Debug, Clone, Copy)]
pub struct RunPatterns<'a> {
    pub operations: &'a str,
    pub git_read: &'a str,
    pub git_write: &'a str,
}

impl crate::store::persistence::fleet_repo::FleetDb<'_> {
    pub async fn run_measures(
        self,
        run_id: &str,
        patterns: RunPatterns<'_>,
    ) -> Result<RunMeasures, PersistError> {
        crate::metrics::time_db("launch_run_measures", async move {
            let seats = self.seat_measures(run_id, patterns).await?;
            let reports = self.report_measure(run_id).await?;
            let mail = self.mail_pairs(run_id).await?;
            let operations = self.operation_measure(run_id, patterns.operations).await?;
            let questions = self.question_measure(run_id).await?;
            let forks = self.fork_measure(run_id).await?;
            let timing = self.timing_measure(run_id).await?;
            let tokens = self.token_measures(run_id).await?;
            let interventions = self.intervention_measure(run_id).await?;
            Ok(RunMeasures {
                seats,
                reports,
                mail,
                operations,
                questions,
                forks,
                timing,
                tokens,
                interventions,
            })
        })
        .await
    }

    async fn timing_measure(self, run_id: &str) -> Result<TimingMeasure, PersistError> {
        let row = sqlx::query_file!(
            "src/store/persistence/fleet/run_record_sql/timing.sql",
            run_id
        )
        .fetch_one(&self.pool)
        .await?;
        let relaunches = self.relaunch_events(run_id).await?;
        Ok(TimingMeasure {
            time_to_done_minutes: row.time_to_done_minutes,
            stop_minutes: row.stop_minutes,
            relaunch_gap_minutes: row.relaunch_gap_minutes,
            no_seat_minutes: row.no_seat_minutes,
            seat_launches: row.seat_launches,
            hand_starts: row.hand_starts,
            task_count: row.task_count,
            worker_launches: row.worker_launches,
            relaunches,
        })
    }

    pub async fn relaunch_events(self, run_id: &str) -> Result<i64, PersistError> {
        Ok(sqlx::query_scalar::<_, i64>(RELAUNCH_EVENTS)
            .bind(run_id)
            .bind(crate::launch::fleet::supervise::RELAUNCHED)
            .fetch_one(&self.pool)
            .await?)
    }

    async fn token_measures(self, run_id: &str) -> Result<Vec<TokenMeasure>, PersistError> {
        let rows = sqlx::query_file!(
            "src/store/persistence/fleet/run_record_sql/tokens.sql",
            run_id
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| TokenMeasure {
                kind: r.kind,
                requests: r.requests,
                input_tokens: r.input_tokens,
                output_tokens: r.output_tokens,
                cache_read_tokens: r.cache_read_tokens,
                cache_write_tokens: r.cache_write_tokens,
            })
            .collect())
    }

    async fn intervention_measure(self, run_id: &str) -> Result<InterventionMeasure, PersistError> {
        let row = sqlx::query_file!(
            "src/store/persistence/fleet/run_record_sql/interventions.sql",
            run_id
        )
        .fetch_one(&self.pool)
        .await?;
        let typed = row
            .operator_prompts
            .iter()
            .filter(|prompt| super::seat_prompts::typed_by_person(prompt, &row.seat_names))
            .count();
        let typed = i64::try_from(typed).unwrap_or(i64::MAX);
        let opened = i64::try_from(row.operator_prompts.len()).unwrap_or(i64::MAX);
        Ok(InterventionMeasure {
            operator_turns: typed,
            mail_driven_turns: opened - typed,
            operator_answers: row.operator_answers,
            reports_filed: row.reports_filed,
        })
    }

    async fn seat_measures(
        self,
        run_id: &str,
        patterns: RunPatterns<'_>,
    ) -> Result<Vec<SeatMeasure>, PersistError> {
        let rows = sqlx::query_file!(
            "src/store/persistence/fleet/run_record_sql/seats.sql",
            run_id,
            patterns.operations,
            patterns.git_read,
            patterns.git_write,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| SeatMeasure {
                seat: r.seat,
                kind: r.kind,
                requests: r.requests,
                working_minutes: r.working_minutes,
                waiting_minutes: r.waiting_minutes,
                tool_calls: r.tool_calls,
                git_reads: r.git_reads,
                operation_commands: r.cargo_commands,
                minutes_to_first_commit: r.minutes_to_first_commit,
                calls_to_first_commit: r.calls_to_first_commit,
                offered: r.offered,
                used: r.used,
            })
            .collect())
    }

    async fn report_measure(self, run_id: &str) -> Result<ReportMeasure, PersistError> {
        let row = sqlx::query_file!(
            "src/store/persistence/fleet/run_record_sql/reports.sql",
            run_id
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(ReportMeasure {
            reports: row.reports,
            answered: row.answered,
            median_minutes: row.median_minutes,
            longest_minutes: row.longest_minutes,
            late_minutes: row.late_minutes,
        })
    }

    async fn mail_pairs(self, run_id: &str) -> Result<Vec<MailPair>, PersistError> {
        let rows = sqlx::query_file!(
            "src/store/persistence/fleet/run_record_sql/mail.sql",
            run_id
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| MailPair {
                from_seat: r.from_seat,
                to_seat: r.to_seat,
                messages: r.messages,
                reached: r.reached,
                acknowledged: r.acknowledged,
            })
            .collect())
    }

    async fn operation_measure(
        self,
        run_id: &str,
        operation_pattern: &str,
    ) -> Result<OperationMeasure, PersistError> {
        let row = sqlx::query_file!(
            "src/store/persistence/fleet/run_record_sql/cargo_repeats.sql",
            run_id,
            operation_pattern,
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(OperationMeasure {
            commands: row.commands,
            repeats: row.repeats,
        })
    }

    async fn question_measure(self, run_id: &str) -> Result<QuestionMeasure, PersistError> {
        let row = sqlx::query_file!(
            "src/store/persistence/fleet/run_record_sql/questions.sql",
            run_id
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(QuestionMeasure {
            asked: row.asked,
            answered: row.answered,
        })
    }

    async fn fork_measure(self, run_id: &str) -> Result<ForkMeasure, PersistError> {
        let row = sqlx::query_file!(
            "src/store/persistence/fleet/run_record_sql/forks.sql",
            run_id
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(ForkMeasure {
            forks: row.forks,
            waited: row.waited,
        })
    }

    pub async fn insert_run_record(
        self,
        fleet_uuid: Uuid,
        run_id: &str,
        document: &Value,
    ) -> Result<bool, PersistError> {
        let stored = stored_document(document);
        crate::metrics::time_db("launch_insert_run_record", async move {
            let row = sqlx::query_file_scalar!(
                "src/store/persistence/fleet/run_record_sql/insert.sql",
                fleet_uuid,
                run_id,
                stored,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(row.is_some())
        })
        .await
    }

    pub async fn replace_run_record(
        self,
        fleet_uuid: Uuid,
        run_id: &str,
        document: &Value,
    ) -> Result<bool, PersistError> {
        let stored = stored_document(document);
        crate::metrics::time_db("launch_replace_run_record", async move {
            let mut tx = self.pool.begin().await?;
            sqlx::query_file!(
                "src/store/persistence/fleet/run_record_sql/delete.sql",
                fleet_uuid
            )
            .execute(&mut *tx)
            .await?;
            let row = sqlx::query_file_scalar!(
                "src/store/persistence/fleet/run_record_sql/insert.sql",
                fleet_uuid,
                run_id,
                stored,
            )
            .fetch_optional(&mut *tx)
            .await?;
            tx.commit().await?;
            Ok(row.is_some())
        })
        .await
    }

    pub async fn find_run_record(self, fleet_uuid: Uuid) -> Result<Option<Value>, PersistError> {
        crate::metrics::time_db("launch_find_run_record", async move {
            let row = sqlx::query_file_scalar!(
                "src/store/persistence/fleet/run_record_sql/find.sql",
                fleet_uuid
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(row.map(operation_document))
        })
        .await
    }
}

const SCORE_KEYS: [&str; 5] = [
    "fault_minutes",
    "tokens_per_accepted_task",
    "manual_interventions",
    "counts",
    "unmeasured",
];

fn declared_operations_version(document: &Value) -> bool {
    document["measure_version"] == "run-record-v2-declared-operations"
        || document["measure_version"] == "run-record-v3"
}

fn stored_document(document: &Value) -> Value {
    let mut stored = document.clone();
    if declared_operations_version(document) {
        stored["cargo_commands"] = document["operation_commands"].clone();
        stored["cargo_repeats_across_seats"] = document["operation_repeats_across_seats"].clone();
    }
    if document["measure_version"] == "run-record-v3" {
        let score: serde_json::Map<String, Value> = SCORE_KEYS
            .iter()
            .map(|key| ((*key).to_string(), document[*key].clone()))
            .collect();
        stored["fault_minutes_total"] = document["fault_minutes"]["total"].clone();
        stored["tokens_per_accepted_task"] = document["tokens_per_accepted_task"]["total"].clone();
        stored["manual_interventions_total"] = document["manual_interventions"]["total"].clone();
        stored["score"] = Value::Object(score);
    }
    stored
}

fn spread_score(row: &mut Value) {
    let score = row["score"].take();
    if let (Some(fields), Value::Object(parts)) = (row.as_object_mut(), score) {
        fields.remove("score");
        fields.remove("fault_minutes_total");
        fields.remove("manual_interventions_total");
        fields.extend(parts);
    }
}

fn operation_document(mut row: Value) -> Value {
    if row["measure_version"] == "run-record-v3" {
        spread_score(&mut row);
    }
    if !declared_operations_version(&row) {
        return row;
    }
    if let Some(fields) = row.as_object_mut() {
        for (old, new) in [
            ("cargo_commands", "operation_commands"),
            (
                "cargo_repeats_across_seats",
                "operation_repeats_across_seats",
            ),
        ] {
            if let Some(value) = fields.remove(old) {
                fields.insert(new.into(), value);
            }
        }
        fields.insert("operation_coverage".into(), Value::String("exact declared commands observed in session tools; managed receipt executions are recorded separately in tree_checks".into()));
        let bypasses = fields
            .get("operation_commands")
            .cloned()
            .unwrap_or(Value::Null);
        fields.insert("direct_operation_bypasses".into(), bypasses);
        fields.insert("operation_adoption_action".into(), Value::String("Route observed bypasses through fleet_check. No observed bypasses does not establish complete adoption; wrapped and undeclared commands are outside coverage.".into()));
    }
    row
}
