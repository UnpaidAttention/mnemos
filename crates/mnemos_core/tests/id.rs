use mnemos_core::id::{new_chunk_id, new_entity_id, new_memory_id, new_session_id, parse_id};

#[test]
fn memory_id_has_correct_prefix() {
    let id = new_memory_id();
    assert!(id.starts_with("mem_"));
    assert_eq!(id.len(), 4 + 26); // "mem_" + 26-char ULID
}

#[test]
fn chunk_session_entity_ids_have_correct_prefixes() {
    assert!(new_chunk_id().starts_with("chunk_"));
    assert!(new_session_id().starts_with("sess_"));
    assert!(new_entity_id().starts_with("ent_"));
}

#[test]
fn ids_are_sortable_by_creation_time() {
    let a = new_memory_id();
    std::thread::sleep(std::time::Duration::from_millis(2));
    let b = new_memory_id();
    assert!(a < b, "ULIDs should sort chronologically");
}

#[test]
fn parse_id_rejects_bad_input() {
    assert!(parse_id("mem_NOT_A_ULID").is_err());
    assert!(parse_id("no_prefix").is_err());
    assert!(parse_id(&new_memory_id()).is_ok());
}
