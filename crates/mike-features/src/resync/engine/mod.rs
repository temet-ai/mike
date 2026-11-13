use sqlx::PgPool;

pub mod id_map;
pub mod insert;
pub mod replay;
pub mod row;
pub mod tables_agents_memory;
pub mod tables_capture;

const BATCH_SIZE: i64 = 500;

const VECTOR_COLUMNS: &[&str] = &[
    "memory.operator_turn_chunks.chunk_embedding",
    "memory.lessons.embedding",
];

fn is_vector_column(table: &str, column: &str) -> bool {
    VECTOR_COLUMNS.contains(&format!("{table}.{column}").as_str())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Class {
    ResolveOrInsert,
    InsertAlways,
}

#[derive(Clone, Copy, Debug)]
struct Fk {
    column: &'static str,
    references: Option<&'static str>,
    deferred: bool,
}

impl Fk {
    const fn remap(column: &'static str, references: &'static str) -> Self {
        Self {
            column,
            references: Some(references),
            deferred: false,
        }
    }
    const fn self_ref(column: &'static str) -> Self {
        Self {
            column,
            references: None,
            deferred: false,
        }
    }
    const fn deferred(column: &'static str, references: Option<&'static str>) -> Self {
        Self {
            column,
            references,
            deferred: true,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct NaturalKeySpec {
    column: &'static str,
    dest_expr: &'static str,
    case_insensitive: bool,
}

impl NaturalKeySpec {
    const fn column(column: &'static str) -> Self {
        Self {
            column,
            dest_expr: column,
            case_insensitive: false,
        }
    }
    const fn lower_of(column: &'static str, dest_expr: &'static str) -> Self {
        Self {
            column,
            dest_expr,
            case_insensitive: true,
        }
    }
    fn normalize(&self, raw: String) -> String {
        if self.case_insensitive {
            raw.to_lowercase()
        } else {
            raw
        }
    }
}

pub struct TableSpec {
    name: &'static str,
    columns: &'static [&'static str],
    fks: &'static [Fk],
    natural_key: Option<NaturalKeySpec>,
    class: Class,
    on_conflict: Option<&'static str>,
}

impl TableSpec {
    fn pass1_fks(&self) -> impl Iterator<Item = &Fk> {
        self.fks.iter().filter(|f| !f.deferred)
    }
    fn deferred_fks(&self) -> impl Iterator<Item = &Fk> {
        self.fks.iter().filter(|f| f.deferred)
    }
}

fn all_tables() -> impl Iterator<Item = &'static TableSpec> {
    tables_capture::TABLES_CAPTURE
        .iter()
        .chain(tables_agents_memory::TABLES_AGENTS_MEMORY)
}

#[derive(Debug, Default)]
pub struct SyncReport {
    pub per_table: Vec<(String, u64)>,
    pub total_rows: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum ResyncError {
    #[error("resync database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error(
        "resync invariant: {table}.{column} = {local_id} (local) has no id_map \
         entry in {references} — parent not replayed before child"
    )]
    UnmappedFk {
        table: String,
        column: String,
        references: String,
        local_id: i64,
    },

    #[error("resync descriptor invariant violated: {0}")]
    Descriptor(String),

    #[error("resync local reset failed: {0}")]
    Reset(#[from] crate::store::persistence::PersistError),
}

pub struct ResyncEngine {
    source: PgPool,
    dest: PgPool,
}

impl ResyncEngine {
    pub fn new(source: PgPool, dest: PgPool) -> Self {
        Self { source, dest }
    }

    pub async fn replay_pending(&self) -> Result<SyncReport, ResyncError> {
        let mut report = SyncReport::default();

        for spec in all_tables() {
            let inserted = self.replay_table(spec).await?;
            if inserted > 0 {
                report.per_table.push((spec.name.to_string(), inserted));
                report.total_rows += inserted;
            }
        }

        for spec in all_tables() {
            if spec.deferred_fks().next().is_some() {
                self.fixup_deferred_fks(spec).await?;
            }
        }

        Ok(report)
    }

    pub async fn has_pending(&self) -> Result<bool, ResyncError> {
        for spec in all_tables() {
            let cursor = self.read_cursor(spec.name).await?;
            let mut qb = sqlx::QueryBuilder::<sqlx::Postgres>::new("SELECT max(id) FROM ");
            qb.push(spec.name);
            let max_id: Option<i64> = qb.build_query_scalar().fetch_one(&self.source).await?;
            if max_id.is_some_and(|m| m > cursor) {
                return Ok(true);
            }
        }
        Ok(false)
    }
}
