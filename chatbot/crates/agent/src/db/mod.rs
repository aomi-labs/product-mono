mod contract_store;
mod traits;

// Domain model
#[derive(Debug, Clone)]
pub struct Contract {
    pub address: String,
    pub chain: String,
    pub source_code: String,
    pub abi: serde_json::Value,
}

impl<'r> sqlx::FromRow<'r, sqlx::any::AnyRow> for Contract {
    fn from_row(row: &'r sqlx::any::AnyRow) -> Result<Self, sqlx::Error> {
        use sqlx::Row;
        Ok(Contract {
            address: row.try_get("address")?,
            chain: row.try_get("chain")?,
            source_code: row.try_get("source_code")?,
            abi: serde_json::from_str(row.try_get("abi")?).map_err(|e| {
                sqlx::Error::ColumnDecode {
                    index: "abi".to_string(),
                    source: Box::new(e),
                }
            })?,
        })
    }
}

#[cfg(test)]
impl<'r> sqlx::FromRow<'r, sqlx::sqlite::SqliteRow> for Contract {
    fn from_row(row: &'r sqlx::sqlite::SqliteRow) -> Result<Self, sqlx::Error> {
        use sqlx::Row;
        Ok(Contract {
            address: row.try_get("address")?,
            chain: row.try_get("chain")?,
            source_code: row.try_get("source_code")?,
            abi: serde_json::from_str(row.try_get("abi")?).map_err(|e| {
                sqlx::Error::ColumnDecode {
                    index: "abi".to_string(),
                    source: Box::new(e),
                }
            })?,
        })
    }
}
