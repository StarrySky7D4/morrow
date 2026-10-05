//! Static admission and legacy isolation for the opt-in directory request ABI.
//! No native selection, Store, account, protected Session or product qualification.
#![cfg(all(feature = "packages", windows))]

use morrow_core::plugin_package::{self, Package, io, proto};
use morrow_plugin_runtime::{Cancellation, Fault, Limits, Runner, package::PreparedPackage};

fn module(imports: &[(&str, &str, usize)]) -> Vec<u8> {
    let mut text = String::from("(module ");
    for (name, function, count) in imports {
        let params = std::iter::repeat_n("i32", *count).collect::<Vec<_>>().join(" ");
        text.push_str(&format!("(import \"{name}\" \"{function}\" (func (param {params}) (result i32))) "));
    }
    text.push_str("(import \"morrow_task_v1\" \"read_input\" (func (param i32 i32) (result i32))) ");
    text.push_str("(import \"morrow_task_v1\" \"complete\" (func (param i32 i32) (result i32))) ");
    text.push_str("(memory (export \"memory\") 4) (func (export \"morrow_run\") (result i32) i32.const 0))");
    wat::parse_str(text).unwrap()
}

fn directory_module() -> Vec<u8> {
    module(&[("morrow_fs_directory_v1", "call", 4)])
}

fn manifest(module: &[u8]) -> proto::Manifest {
    let mut value = Package::manifest_for_task("org.example.directory-profile", "1.0.0", module, vec![]);
    value.required_features = vec![io::FEATURE.into(), plugin_package::DIRECTORY_REQUEST_FEATURE.into()];
    value.io_declaration = Some(io::declaration(vec![io::IoCapability::FileList], vec!["directory.list".into()]));
    value
}

#[test]
fn exact_opt_in_prepares_without_creating_authority_or_running_a_guest() {
    let bytes = directory_module();
    let package = Package::build(manifest(&bytes), &bytes).unwrap();
    assert!(package.capabilities().is_empty());
    assert_eq!(package.io_declaration().unwrap().requested_capabilities, vec![io::IoCapability::FileList.number()]);
    let expected_archive = package.archive().to_vec();
    let prepared = PreparedPackage::new(package, Limits::default()).unwrap();
    assert_eq!(prepared.package().archive(), expected_archive.as_slice());
}

#[test]
fn every_legacy_factory_refuses_the_new_import() {
    let bytes = directory_module();
    let factories: [fn(&[u8], Limits) -> Result<Runner, Fault>; 6] = [
        Runner::new, Runner::new_task, Runner::new_dependency_task,
        Runner::new_io_task, Runner::new_mutation_task, Runner::new_channel_task,
    ];
    for factory in factories {
        assert!(matches!(factory(&bytes, Limits::default()), Err(Fault::UnsupportedAbi)));
    }
}

#[test]
fn directory_factory_refuses_absent_future_or_wrong_import_signatures() {
    for imports in [
        vec![], vec![("morrow_fs_directory_v2", "call", 4)],
        vec![("morrow_fs_directory_v1", "read", 4)],
        vec![("morrow_fs_directory_v1", "call", 3)],
        vec![("morrow_fs_directory_v1", "call", 5)],
    ] {
        assert!(matches!(Runner::new_directory_task(&module(&imports), Limits::default()), Err(Fault::UnsupportedAbi)));
    }
    assert!(Runner::new_directory_task(&directory_module(), Limits::default()).is_ok());
}

#[test]
fn directory_import_does_not_enable_any_second_extra_import_or_wasi() {
    for other in ["morrow_io_v1", "morrow_mutation_v1", "morrow_dependency_v1", "morrow_channel_v1", "wasi_snapshot_preview1"] {
        let bytes = module(&[("morrow_fs_directory_v1", "call", 4), (other, "call", 4)]);
        assert!(matches!(Runner::new_directory_task(&bytes, Limits::default()), Err(Fault::UnsupportedAbi)));
    }
}

#[test]
fn a_directory_runner_cannot_be_driven_by_an_unbound_synchronous_callback() {
    let runner = Runner::new_directory_task(&directory_module(), Limits::default()).unwrap();
    let run = runner.run_task(&[1], &mut |_| panic!("a bare callback must not acquire directory authority"), Cancellation::default());
    assert_eq!(run.report.outcome, Err(Fault::UnsupportedAbi));
    assert_eq!(run.report.host_calls, 0);
    assert!(run.completion.is_none());
}

#[test]
fn package_feature_and_import_must_match_in_both_directions() {
    let directory = directory_module();
    let mut old = manifest(&directory);
    old.required_features.retain(|feature| feature != plugin_package::DIRECTORY_REQUEST_FEATURE);
    let old = Package::build(old, &directory).unwrap();
    assert!(matches!(PreparedPackage::new(old, Limits::default()), Err(Fault::UnsupportedAbi)));
    let legacy_io = module(&[("morrow_io_v1", "call", 4)]);
    let new = Package::build(manifest(&legacy_io), &legacy_io).unwrap();
    assert!(matches!(PreparedPackage::new(new, Limits::default()), Err(Fault::UnsupportedAbi)));
}

#[test]
fn directory_only_declarations_reject_read_mutation_network_and_content_ceilings() {
    let bytes = directory_module();
    for capabilities in [
        vec![io::IoCapability::FileRead],
        vec![io::IoCapability::FileList, io::IoCapability::FileRead],
        vec![io::IoCapability::FileList, io::IoCapability::FileCreate],
        vec![io::IoCapability::FileList, io::IoCapability::HttpRequest],
    ] {
        let mut value = manifest(&bytes);
        value.io_declaration = Some(io::declaration(capabilities, vec!["directory.list".into()]));
        assert!(Package::build(value, &bytes).is_err());
    }
    let mut content = manifest(&bytes);
    content.requested_capabilities.push(1);
    assert!(Package::build(content, &bytes).is_err());
}

#[test]
fn unknown_profile_and_changed_legacy_contracts_do_not_fallback() {
    let bytes = directory_module();
    let mut future = manifest(&bytes);
    future.required_features[1] = "fs-directory-request-v2".into();
    assert!(matches!(Package::build(future, &bytes), Err(morrow_core::Error::UnsupportedVersion)));
    let mut changed = manifest(&bytes);
    changed.io_declaration.as_mut().unwrap().io_schema_sha256[0] ^= 1;
    assert!(matches!(Package::build(changed, &bytes), Err(morrow_core::Error::UnsupportedVersion)));
    let mut changed = manifest(&bytes);
    changed.task_schema_sha256[0] ^= 1;
    assert!(matches!(Package::build(changed, &bytes), Err(morrow_core::Error::UnsupportedVersion)));
}

#[test]
fn original_io_resource_job_and_byte_ceilings_still_bound_the_new_profile() {
    let bytes = directory_module();
    for invalid in [0, io::MAX_RESOURCES + 1] {
        let mut value = manifest(&bytes);
        value.io_declaration.as_mut().unwrap().budget.as_mut().unwrap().max_resources = invalid;
        assert!(Package::build(value, &bytes).is_err());
    }
    for invalid in [0, io::MAX_JOB_BYTES + 1] {
        let mut value = manifest(&bytes);
        value.io_declaration.as_mut().unwrap().budget.as_mut().unwrap().max_job_bytes = invalid;
        assert!(Package::build(value, &bytes).is_err());
    }
    assert_eq!(morrow_plugin_runtime::MAX_DIRECTORY_REQUEST_BYTES, 512);
    assert_eq!(morrow_plugin_runtime::MAX_DIRECTORY_RESPONSE_BYTES, 65536);
    assert_eq!(morrow_plugin_runtime::MAX_TASK_BYTES, 131072);
}
