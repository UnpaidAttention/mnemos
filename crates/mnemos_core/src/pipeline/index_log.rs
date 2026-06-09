use crate::error::Result;
use crate::paths::Paths;
use crate::storage::Storage;
use chrono::Utc;
use std::path::Path;

/// Regenerate index.md and log.md in the vault root.
pub async fn update_index_log(storage: &Storage, paths: &Paths) -> Result<()> {
    let conn = storage.conn()?;

    // 1. Generate index.md
    let mut rows = conn
        .query(
            "SELECT id, tier, kind, title, file_path, created_at, workspace, source_tool 
             FROM memories 
             WHERE invalid_at IS NULL 
             ORDER BY tier, created_at DESC",
            (),
        )
        .await?;

    let mut working_items = Vec::new();
    let mut episodic_items = Vec::new();
    let mut semantic_items = Vec::new();
    let mut procedural_items = Vec::new();
    let mut reflection_items = Vec::new();

    while let Some(row) = rows.next().await? {
        let id: String = row.get(0)?;
        let tier: String = row.get(1)?;
        let kind: String = row.get(2)?;
        let title: String = row.get(3)?;
        let file_path: String = row.get(4)?;
        let created_at: String = row.get(5)?;
        let workspace: Option<String> = row.get(6)?;
        let source_tool: Option<String> = row.get(7)?;

        let relative_path = Path::new(&file_path)
            .strip_prefix(&paths.root)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or(file_path);

        let ws_str = workspace.map_or("".to_string(), |w| format!(" | workspace: `{}`", w));
        let tool_str = source_tool.map_or("".to_string(), |t| format!(" | tool: `{}`", t));
        let item_line = format!(
            "- [[{}|{}]] (id: `{}` | type: `{}`{}{} | created: {})",
            relative_path, title, id, kind, ws_str, tool_str, created_at
        );

        match tier.as_str() {
            "working" => working_items.push(item_line),
            "episodic" => episodic_items.push(item_line),
            "semantic" => semantic_items.push(item_line),
            "procedural" => procedural_items.push(item_line),
            "reflection" => reflection_items.push(item_line),
            _ => semantic_items.push(item_line),
        }
    }

    // List entities
    let mut entity_rows = conn
        .query(
            "SELECT id, name, kind, description, file_path FROM entities ORDER BY name",
            (),
        )
        .await?;
    let mut entity_items = Vec::new();
    while let Some(row) = entity_rows.next().await? {
        let id: String = row.get(0)?;
        let name: String = row.get(1)?;
        let kind: String = row.get(2)?;
        let desc: Option<String> = row.get(3)?;
        let file_path: Option<String> = row.get(4)?;

        let link_path = file_path
            .as_deref()
            .and_then(|fp| {
                Path::new(fp)
                    .strip_prefix(&paths.root)
                    .ok()
                    .map(|p| p.to_string_lossy().to_string())
            })
            .unwrap_or_else(|| format!("files/entities/{}.md", name));

        let desc_str = desc.map_or("".to_string(), |d| format!(" - {}", d));
        entity_items.push(format!(
            "- [[{}|{}]] (id: `{}` | type: `{}`){}",
            link_path, name, id, kind, desc_str
        ));
    }

    let mut index_content = String::new();
    index_content.push_str("# Mnemos Vault Index\n\n");
    index_content.push_str(&format!("Generated on: {}\n\n", Utc::now().to_rfc3339()));

    index_content.push_str("## Working Memory\n");
    if working_items.is_empty() {
        index_content.push_str("No working memories.\n\n");
    } else {
        for line in working_items {
            index_content.push_str(&format!("{}\n", line));
        }
        index_content.push('\n');
    }

    index_content.push_str("## Procedural Memory\n");
    if procedural_items.is_empty() {
        index_content.push_str("No procedural rules.\n\n");
    } else {
        for line in procedural_items {
            index_content.push_str(&format!("{}\n", line));
        }
        index_content.push('\n');
    }

    index_content.push_str("## Reflections & Syntheses\n");
    if reflection_items.is_empty() {
        index_content.push_str("No reflections.\n\n");
    } else {
        for line in reflection_items {
            index_content.push_str(&format!("{}\n", line));
        }
        index_content.push('\n');
    }

    index_content.push_str("## Semantic Memory (Facts)\n");
    if semantic_items.is_empty() {
        index_content.push_str("No semantic facts.\n\n");
    } else {
        for line in semantic_items {
            index_content.push_str(&format!("{}\n", line));
        }
        index_content.push('\n');
    }

    index_content.push_str("## Episodic Memory (Transcripts)\n");
    if episodic_items.is_empty() {
        index_content.push_str("No episodic memories.\n\n");
    } else {
        for line in episodic_items {
            index_content.push_str(&format!("{}\n", line));
        }
        index_content.push('\n');
    }

    index_content.push_str("## Entities\n");
    if entity_items.is_empty() {
        index_content.push_str("No entities registered.\n\n");
    } else {
        for line in entity_items {
            index_content.push_str(&format!("{}\n", line));
        }
        index_content.push('\n');
    }

    let index_path = paths.root.join("index.md");
    tokio::fs::write(&index_path, index_content).await?;

    // 2. Generate log.md
    let mut log_rows = conn
        .query(
            "SELECT ts, actor, action, memory_id, details 
             FROM audit_log 
             ORDER BY ts DESC 
             LIMIT 250",
            (),
        )
        .await?;

    let mut log_content = String::new();
    log_content.push_str("# Mnemos Activity Log\n\n");
    log_content.push_str(&format!("Last updated: {}\n\n", Utc::now().to_rfc3339()));
    log_content.push_str("| Timestamp | Actor | Action | Memory ID | Details |\n");
    log_content.push_str("| :--- | :--- | :--- | :--- | :--- |\n");

    while let Some(row) = log_rows.next().await? {
        let ts: String = row.get(0)?;
        let actor: String = row.get(1)?;
        let action: String = row.get(2)?;
        let memory_id: Option<String> = row.get(3)?;
        let details: Option<String> = row.get(4)?;

        let mem_str = memory_id.unwrap_or_else(|| "—".to_string());
        let details_str = details.unwrap_or_else(|| "—".to_string());
        log_content.push_str(&format!(
            "| {} | {} | {} | {} | {} |\n",
            ts, actor, action, mem_str, details_str
        ));
    }

    let log_path = paths.root.join("log.md");
    tokio::fs::write(&log_path, log_content).await?;

    Ok(())
}
