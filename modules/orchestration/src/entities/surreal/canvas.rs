use surrealdb::Error;
use surrealdb::types as surrealdb_types;
use surrealdb::types::{Kind, RecordId, SurrealValue, Value, kind};

#[derive(Debug, Clone)]
pub struct CanvasId(pub RecordId);

impl CanvasId {
    const TABLE: &'static str = "canvas";
}

impl SurrealValue for CanvasId {
    fn kind_of() -> Kind {
        kind!(record<canvas>)
    }

    fn is_value(value: &Value) -> bool {
        matches!(value, Value::RecordId(r) if r.table.as_str() == Self::TABLE)
    }

    fn into_value(self) -> Value {
        Value::RecordId(self.0)
    }

    fn from_value(value: Value) -> Result<Self, Error>
    where
        Self: Sized,
    {
        match value {
            Value::RecordId(r) if r.table.as_str() == Self::TABLE => Ok(Self(r)),
            other => Err(Error::internal(format!(
                "expected canvas record, got {other:?}"
            ))),
        }
    }
}

#[derive(Debug, Clone, SurrealValue)]
pub struct CanvasEntity {
    pub id: CanvasId,
    pub name: String,
    pub description: String,
}
