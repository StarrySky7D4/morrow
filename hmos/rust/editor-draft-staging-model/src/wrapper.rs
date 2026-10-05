//! Original durable-import schema. No file or database access.
pub mod proto {
    include!(concat!(
        env!("OUT_DIR"),
        "/morrow.workbench.editor_draft_staging.v1.rs"
    ));
}
