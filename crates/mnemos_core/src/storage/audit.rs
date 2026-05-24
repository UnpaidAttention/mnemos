use crate::error::Result;
use crate::storage::Storage;
use chrono::{DateTime, Utc};
use libsql::params;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: i64,
    pub ts: DateTime<Utc>,
    pub actor: String,
    pub action: String,
    pub memory_id: Option<String>,
    pub details: Option<Value>,
}

pub async fn write_audit(
    storage: &Storage,
    actor: &str,
    action: &str,
    memory_id: Option<&str>,
    details: Option<Value>,
) -> Result<()> {
    let (conn, _guard) = storage.write_conn().await?;
    let details_str = details.map(|v| v.to_string());
    conn.execute(
        "INSERT INTO audit_log (ts, actor, action, memory_id, details)
            VALUES (?, ?, ?, ?, ?)",
        params![
            Utc::now().to_rfc3339(),
            actor.to_string(),
            action.to_string(),
            memory_id.map(String::from),
            details_str,
        ],
    )
    .await?;
    Ok(())
}

pub async fn list_audit(storage: &Storage, memory_id: Option<&str>) -> Result<Vec<AuditEntry>> {
    let conn = storage.conn()?;
    let (sql, args): (&str, Vec<libsql::Value>) = match memory_id {
        Some(id) => (
            "SELECT id, ts, actor, action, memory_id, details
               FROM audit_log WHERE memory_id = ? ORDER BY id ASC",
            vec![id.to_string().into()],
        ),
        None => (
            "SELECT id, ts, actor, action, memory_id, details
               FROM audit_log ORDER BY id ASC",
            vec![],
        ),
    };
    let mut rows = conn.query(sql, args).await?;
    let mut out = Vec::new();
    while let Some(row) = rows.next().await? {
        let ts_str: String = row.get(1)?;
        let details_str: Option<String> = row.get(5)?;
        out.push(AuditEntry {
            id: row.get(0)?,
            ts: DateTime::parse_from_rfc3339(&ts_str)
                .map(|d| d.with_timezone(&Utc))
                .map_err(|e| crate::error::MnemosError::Validation(e.to_string()))?,
            actor: row.get(2)?,
            action: row.get(3)?,
            memory_id: row.get(4)?,
            details: details_str.map(|s| serde_json::from_str(&s)).transpose()?,
        });
    }
    Ok(out)
}
