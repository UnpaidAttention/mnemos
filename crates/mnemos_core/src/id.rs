use crate::error::{MnemosError, Result};
use ulid::Ulid;

pub fn new_memory_id() -> String {
    format!("mem_{}", Ulid::new())
}

pub fn new_chunk_id() -> String {
    format!("chunk_{}", Ulid::new())
}

pub fn new_session_id() -> String {
    format!("sess_{}", Ulid::new())
}

pub fn new_entity_id() -> String {
    format!("ent_{}", Ulid::new())
}

pub fn new_edge_id() -> String {
    format!("edge_{}", Ulid::new())
}

/// Parse an id of the form `<prefix>_<ulid>`; returns the ULID portion.
pub fn parse_id(id: &str) -> Result<Ulid> {
    let (_prefix, ulid_str) = id
        .split_once('_')
        .ok_or_else(|| MnemosError::Validation(format!("id missing prefix: {id}")))?;
    Ulid::from_string(ulid_str)
        .map_err(|e| MnemosError::Validation(format!("invalid ulid in {id}: {e}")))
}
