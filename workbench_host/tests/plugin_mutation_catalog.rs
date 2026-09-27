#![cfg(target_os = "windows")]
//! Catalog mutation metadata is a package declaration, never a budget grant.
use capnp::{message::Builder, serialize};
use morrow_core::plugin_package::{
    MUTATION_BUDGET_FEATURE, MUTATION_FEATURE, Package,
    io::{self, IoCapability},
    proto,
};
use morrow_workbench_host::{
    Workbench, host_capnp as wire,
    plugin_catalog::{MutationBudgetDeclaration, PluginEntry},
    protocol,
};
use std::path::Path;

const ID: &str = "org.example.catalog-mutation-budget";
const MAX_JOB: u64 = 32 * 1024 * 1024;
const MAX_TOTAL: u64 = 256 * 1024 * 1024;

fn package(extended: bool) -> Package {
    let wasm = wat::parse_str(
        r#"(module
          (import "morrow_task_v1" "read_input" (func $read (param i32 i32) (result i32)))
          (import "morrow_mutation_v1" "call" (func $call (param i32 i32 i32 i32) (result i32)))
          (memory (export "memory") 4)
          (func (export "morrow_run") (result i32) (i32.const 0)))"#,
    )
    .unwrap();
    let id = if extended {
        ID
    } else {
        "org.example.catalog-mutation-legacy"
    };
    let mut manifest = Package::manifest_for_task(id, "1.0.0", &wasm, vec![]);
    manifest.required_features = vec![io::FEATURE.into(), MUTATION_FEATURE.into()];
    manifest.mutation_schema_sha256 = morrow_core::mutation::schema_digest().to_vec();
    manifest.io_declaration = Some(io::declaration(
        vec![IoCapability::FileCreate, IoCapability::FileDelete],
        vec!["mutation.run".into()],
    ));
    if extended {
        manifest
            .required_features
            .push(MUTATION_BUDGET_FEATURE.into());
        manifest.mutation_budget = Some(proto::MutationBudget {
            max_job_bytes: MAX_JOB,
            max_bytes: MAX_TOTAL,
        });
    }
    Package::build(manifest, &wasm).unwrap()
}

fn entry(workbench: &Workbench, id: &str) -> PluginEntry {
    let mut page = workbench.catalog_page("", None).unwrap();
    loop {
        if let Some(index) = page.entries.iter().position(|entry| entry.id == id) {
            return page.entries.remove(index);
        }
        assert!(!page.cursor.is_empty());
        page = workbench
            .catalog_page(&page.cursor, Some(page.revision))
            .unwrap();
    }
}

fn wire_row(workbench: &mut Workbench, action: wire::Action, path: Option<&Path>) -> Vec<u8> {
    let mut message = Builder::new_default();
    let mut request = message.init_root::<wire::request::Builder>();
    request.set_version(1);
    request.set_digest(&protocol::digest());
    request.set_action(action);
    if let Some(path) = path {
        request.set_selected_path(path.to_str().unwrap());
    }
    let bytes = protocol::respond(workbench, &serialize::write_message_to_words(&message)).unwrap();
    let response = serialize::read_message(&mut &bytes[..], Default::default()).unwrap();
    let result = response.get_root::<wire::response::Reader>().unwrap();
    assert!(result.get_error().unwrap().is_empty());
    let row = result.get_plugins().unwrap().get(0);
    assert!(row.get_mutation_supported());
    assert!(row.has_mutation_budget());
    let budget = row.get_mutation_budget().unwrap();
    assert_eq!(budget.get_max_job_bytes(), MAX_JOB);
    assert_eq!(budget.get_max_bytes(), MAX_TOTAL);
    assert_eq!(row.get_approved_io().unwrap().len(), 0);
    bytes
}

fn wire_without_budget(
    workbench: &mut Workbench,
    action: wire::Action,
    path: Option<&Path>,
    supported: bool,
) {
    let mut message = Builder::new_default();
    let mut request = message.init_root::<wire::request::Builder>();
    request.set_version(1);
    request.set_digest(&protocol::digest());
    request.set_action(action);
    if let Some(path) = path {
        request.set_selected_path(path.to_str().unwrap());
    }
    let bytes = protocol::respond(workbench, &serialize::write_message_to_words(&message)).unwrap();
    let response = serialize::read_message(&mut &bytes[..], Default::default()).unwrap();
    let result = response.get_root::<wire::response::Reader>().unwrap();
    assert!(result.get_error().unwrap().is_empty());
    let row = result.get_plugins().unwrap().get(0);
    assert_eq!(row.get_mutation_supported(), supported);
    assert!(!row.has_mutation_budget());
}

#[test]
fn inspect_and_catalog_report_declared_mutation_budget_without_approval() {
    let dir = tempfile::tempdir().unwrap();
    let mut workbench = Workbench::open_managed(dir.path(), None).unwrap();
    let extended = package(true);
    let inspected = workbench.inspect_plugin_bytes(extended.archive()).unwrap();
    assert_eq!(inspected.entries.len(), 1);
    let row = &inspected.entries[0];
    assert!(row.mutation_supported);
    assert_eq!(
        row.mutation_budget,
        Some(MutationBudgetDeclaration {
            max_job_bytes: MAX_JOB,
            max_bytes: MAX_TOTAL,
        })
    );
    assert!(row.approved_io.is_empty());

    let file = dir.path().join("incoming.mplugin");
    std::fs::write(&file, extended.archive()).unwrap();
    wire_row(&mut workbench, wire::Action::PluginInspect, Some(&file));
    let revision = workbench.catalog_page("", None).unwrap().revision;
    workbench
        .import_plugin(&file, &extended.digest(), revision)
        .unwrap();
    let installed = entry(&workbench, ID);
    assert!(installed.mutation_supported);
    assert_eq!(installed.mutation_budget, row.mutation_budget);
    assert!(installed.approved_io.is_empty());
    wire_row(&mut workbench, wire::Action::PluginCatalog, None);

    let archive = dir.path().join(format!(
        "plugin-manager/packages/{}.mplugin",
        extended
            .digest()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    ));
    std::fs::remove_file(archive).unwrap();
    let missing = entry(&workbench, ID);
    assert!(!missing.available);
    assert!(!missing.mutation_supported);
    assert!(missing.mutation_budget.is_none());
    wire_without_budget(&mut workbench, wire::Action::PluginCatalog, None, false);
}

#[test]
fn ordinary_mutation_has_no_extended_budget_and_plain_task_has_no_mutation_feature() {
    let dir = tempfile::tempdir().unwrap();
    let mut workbench = Workbench::open_managed(dir.path(), None).unwrap();
    let legacy_package = package(false);
    let legacy = workbench
        .inspect_plugin_bytes(legacy_package.archive())
        .unwrap();
    assert!(legacy.entries[0].mutation_supported);
    assert!(legacy.entries[0].mutation_budget.is_none());
    let legacy_path = dir.path().join("legacy.mplugin");
    std::fs::write(&legacy_path, legacy_package.archive()).unwrap();
    wire_without_budget(
        &mut workbench,
        wire::Action::PluginInspect,
        Some(&legacy_path),
        true,
    );

    let wasm = wat::parse_str(
        r#"(module
          (import "morrow_task_v1" "read_input" (func $read (param i32 i32) (result i32)))
          (memory (export "memory") 1)
          (func (export "morrow_run") (result i32) (i32.const 0)))"#,
    )
    .unwrap();
    let plain = Package::build(
        Package::manifest_for_task("org.example.catalog-plain", "1.0.0", &wasm, vec![]),
        &wasm,
    )
    .unwrap();
    let inspected = workbench.inspect_plugin_bytes(plain.archive()).unwrap();
    assert!(!inspected.entries[0].mutation_supported);
    assert!(inspected.entries[0].mutation_budget.is_none());
    let plain_path = dir.path().join("plain.mplugin");
    std::fs::write(&plain_path, plain.archive()).unwrap();
    wire_without_budget(
        &mut workbench,
        wire::Action::PluginInspect,
        Some(&plain_path),
        false,
    );
}
