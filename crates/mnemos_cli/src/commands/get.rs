use crate::commands::open_vault;
use anyhow::Result;
use std::path::PathBuf;

pub async fn run(vault: Option<PathBuf>, json: bool, id: String) -> Result<()> {
    let vault = open_vault(vault).await?;
    let mem = vault.get(&id).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&mem)?);
    } else {
        println!("ID:         {}", mem.id);
        println!("Tier:       {}", mem.tier);
        println!("Type:       {:?}", mem.kind);
        println!("Title:      {}", mem.title);
        println!("Tags:       {}", mem.tags.join(", "));
        println!("Created:    {}", mem.created_at);
        println!("Valid at:   {}", mem.valid_at);
        if let Some(inv) = mem.invalid_at {
            println!("Invalid at: {inv}");
        }
        println!("Strength:   {:.3}", mem.strength);
        println!("Importance: {:.3}", mem.importance);
        println!("---");
        println!("{}", mem.body);
    }
    Ok(())
}
