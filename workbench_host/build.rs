fn main() {
    println!("cargo:rerun-if-changed=schemas/ui_preferences.proto");
    println!("cargo:rerun-if-changed=schemas/projection.proto");
    println!("cargo:rerun-if-changed=schemas/tasks_migration.proto");
    println!("cargo:rerun-if-changed=schemas/tasks_edit.proto");
    println!("cargo:rerun-if-changed=schemas/cards_edit.proto");
    println!("cargo:rerun-if-changed=schemas/editor_recovery.proto");
    println!("cargo:rerun-if-changed=schemas/editor_draft.proto");
    println!("cargo:rerun-if-changed=schemas/editor_draft_staging.proto");
    println!("cargo:rerun-if-changed=schemas/editor_draft_import_decision.proto");
    println!("cargo:rerun-if-changed=schemas/editor_draft_handoff_proposal.proto");
    println!("cargo:rerun-if-changed=schemas/query_capture.proto");
    let protoc = protoc_bin_vendored::protoc_bin_path().expect("protoc");
    prost_build::Config::new()
        .protoc_executable(&protoc)
        .compile_protos(&["schemas/editor_draft.proto"], &["schemas"])
        .expect("editor draft schema");
    prost_build::Config::new()
        .extern_path(
            ".morrow.workbench.editor_draft.v1",
            "crate::editor_draft::model::proto",
        )
        .protoc_executable(&protoc)
        .compile_protos(
            &[
                "schemas/projection.proto",
                "schemas/query_capture.proto",
                "schemas/ui_preferences.proto",
                "schemas/tasks_migration.proto",
                "schemas/tasks_edit.proto",
                "schemas/cards_edit.proto",
                "schemas/editor_recovery.proto",
                "schemas/editor_draft_staging.proto",
                "schemas/editor_draft_import_decision.proto",
                "schemas/editor_draft_handoff_proposal.proto",
            ],
            &["schemas"],
        )
        .expect("projection schema");
    println!("cargo:rerun-if-changed=schemas/host.capnp");
    println!("cargo:rerun-if-changed=schemas/content_api.capnp");
    println!("cargo:rerun-if-changed=schemas/editor_draft_api.capnp");
    println!("cargo:rerun-if-changed=schemas/editor_draft_staging_api.capnp");
    capnpc::CompilerCommand::new()
        .src_prefix("schemas")
        .file("schemas/host.capnp")
        .file("schemas/content_api.capnp")
        .file("schemas/editor_draft_api.capnp")
        .file("schemas/editor_draft_staging_api.capnp")
        .run()
        .expect("host schema");
}
