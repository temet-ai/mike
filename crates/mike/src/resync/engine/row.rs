use std::collections::HashMap;

use sqlx::postgres::PgRow;
use sqlx::query_builder::Separated;
use sqlx::{Postgres, Row};

use super::{ResyncError, TableSpec, is_vector_column};

#[derive(Clone, PartialEq, Eq, Hash)]
pub(super) struct NaturalKey(pub(super) String);

pub(super) struct SourceRow {
    pub(super) local_id: i64,
    cells: HashMap<&'static str, Cell>,
    fk_values: HashMap<&'static str, Option<i64>>,
}

impl SourceRow {
    pub(super) fn parse(spec: &TableSpec, row: PgRow) -> Result<Self, ResyncError> {
        use sqlx::Column;
        use sqlx::TypeInfo;

        let local_id: i64 = row.try_get("id")?;
        let mut cells: HashMap<&'static str, Cell> = HashMap::new();
        let fk_cols: std::collections::HashSet<&'static str> =
            spec.fks.iter().map(|f| f.column).collect();
        let mut fk_values: HashMap<&'static str, Option<i64>> = HashMap::new();

        for &col in spec.columns {
            let cell = if is_vector_column(spec.name, col) {
                Cell::Vector(row.try_get(col)?)
            } else {
                let ty = row
                    .columns()
                    .iter()
                    .find(|c| c.name() == col)
                    .map(|c| c.type_info().name().to_string())
                    .unwrap_or_default();
                decode_cell(&row, col, &ty)?
            };
            if fk_cols.contains(col) {
                let v: Option<i64> = match &cell {
                    Cell::I64(v) => *v,
                    _ => None,
                };
                fk_values.insert(col, v);
            }
            cells.insert(col, cell);
        }

        Ok(SourceRow {
            local_id,
            cells,
            fk_values,
        })
    }

    pub(super) fn column(&self, name: &str) -> Result<&Cell, ResyncError> {
        self.cells.get(name).ok_or_else(|| {
            ResyncError::Descriptor(format!("column '{name}' absent from parsed source row"))
        })
    }
    pub(super) fn column_text(&self, name: &str) -> Result<String, ResyncError> {
        self.column(name)?.as_text()
    }
    pub(super) fn fk_value(&self, name: &str) -> Option<i64> {
        self.fk_values.get(name).copied().flatten()
    }
    pub(super) fn push_cell(
        &self,
        col: &str,
        cells: &mut Separated<'_, Postgres, &'static str>,
    ) -> Result<(), ResyncError> {
        self.column(col)?.push_into(cells);
        Ok(())
    }
}

pub(super) enum Cell {
    Bool(Option<bool>),
    I16(Option<i16>),
    I32(Option<i32>),
    I64(Option<i64>),
    F64(Option<f64>),
    Text(Option<String>),
    Json(Option<serde_json::Value>),
    Bytea(Option<Vec<u8>>),
    Timestamptz(Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>),
    Uuid(Option<sqlx::types::Uuid>),
    Interval(Option<sqlx::postgres::types::PgInterval>),
    TextArray(Option<Vec<String>>),
    Vector(Option<String>),
}

impl Cell {
    fn as_text(&self) -> Result<String, ResyncError> {
        match self {
            Cell::Text(Some(s)) => Ok(s.clone()),
            Cell::Uuid(Some(u)) => Ok(u.to_string()),
            other => Err(ResyncError::Descriptor(format!(
                "natural key cell is not non-null text/uuid: {other:?}"
            ))),
        }
    }

    fn push_into(&self, cells: &mut Separated<'_, Postgres, &'static str>) {
        match self {
            Cell::Bool(v) => cells.push_bind(v),
            Cell::I16(v) => cells.push_bind(v),
            Cell::I32(v) => cells.push_bind(v),
            Cell::I64(v) => cells.push_bind(v),
            Cell::F64(v) => cells.push_bind(v),
            Cell::Text(v) => cells.push_bind(v),
            Cell::Json(v) => cells.push_bind(v),
            Cell::Bytea(v) => cells.push_bind(v),
            Cell::Timestamptz(v) => cells.push_bind(v),
            Cell::Uuid(v) => cells.push_bind(v),
            Cell::Interval(v) => cells.push_bind(v),
            Cell::TextArray(v) => cells.push_bind(v),
            Cell::Vector(v) => cells.push_bind(v),
        };
    }
}

impl std::fmt::Debug for Cell {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let kind = match self {
            Cell::Bool(_) => "Bool",
            Cell::I16(_) => "I16",
            Cell::I32(_) => "I32",
            Cell::I64(_) => "I64",
            Cell::F64(_) => "F64",
            Cell::Text(_) => "Text",
            Cell::Json(_) => "Json",
            Cell::Bytea(_) => "Bytea",
            Cell::Timestamptz(_) => "Timestamptz",
            Cell::Uuid(_) => "Uuid",
            Cell::Interval(_) => "Interval",
            Cell::TextArray(_) => "TextArray",
            Cell::Vector(_) => "Vector",
        };
        write!(f, "Cell::{kind}")
    }
}

fn decode_cell(row: &PgRow, col: &str, ty: &str) -> Result<Cell, ResyncError> {
    let cell = match ty {
        "BOOL" => Cell::Bool(row.try_get(col)?),
        "INT2" => Cell::I16(row.try_get(col)?),
        "INT4" => Cell::I32(row.try_get(col)?),
        "INT8" => Cell::I64(row.try_get(col)?),
        "FLOAT8" => Cell::F64(row.try_get(col)?),
        "TEXT" | "VARCHAR" | "BPCHAR" | "NAME" => Cell::Text(row.try_get(col)?),
        "JSON" | "JSONB" => Cell::Json(row.try_get(col)?),
        "BYTEA" => Cell::Bytea(row.try_get(col)?),
        "TIMESTAMPTZ" => Cell::Timestamptz(row.try_get(col)?),
        "UUID" => Cell::Uuid(row.try_get(col)?),
        "INTERVAL" => Cell::Interval(row.try_get(col)?),
        "TEXT[]" | "VARCHAR[]" => Cell::TextArray(row.try_get(col)?),
        other => {
            return Err(ResyncError::Database(sqlx::Error::Decode(
                format!("resync: unhandled column type '{other}' for column '{col}'").into(),
            )));
        }
    };
    Ok(cell)
}
