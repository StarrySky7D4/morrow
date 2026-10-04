//! The actual selected host's static diagnostic must never initialize a workbench.
use morrow_core::plugin_package::{MAX_PACKAGE_BYTES, Package};
use morrow_workbench_host::sdk_preflight::{COMMAND, MAX_OUTPUT_BYTES};
use prost::Message;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{ffi::OsString, fs, path::Path, process::Command};

#[path = "../../plugin_runtime/tests/support/frozen_sdk.rs"]
mod frozen_sdk;

fn call(cwd: &Path, args: &[OsString], expected_exit: i32) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_morrow-workbench-host"))
        .arg(COMMAND)
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap();
    eprintln!(
        "args={args:?} exit={:?} stdout={} stderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(expected_exit));
    assert!(output.stderr.is_empty());
    assert!(output.stdout.len() <= MAX_OUTPUT_BYTES);
    assert_eq!(output.stdout.last(), Some(&b'\n'));
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["diagnostic"], "package_preflight");
    assert_eq!(value["preparation"], "static_only");
    assert_eq!(value["authority"], "none");
    assert_eq!(value["grants_created"], 0);
    for field in [
        "guest_executed",
        "installed",
        "production_qualified",
        "routes_qualified",
    ] {
        assert_eq!(value[field], false);
    }
    assert_eq!(fs::read_dir(cwd).unwrap().count(), 0, "host created state");
    value
}

fn synthesize(path: &Path, module: &[u8], budget: Option<(u64, u64, u32)>) -> Package {
    let mut manifest = Package::manifest_for("test.preflight", "1.0.0", module, vec![]);
    if let Some((fuel, memory_bytes, host_calls)) = budget {
        let value = manifest.budget.as_mut().unwrap();
        value.fuel = fuel;
        value.memory_bytes = memory_bytes;
        value.host_calls = host_calls;
    }
    let package = Package::build(manifest, module).unwrap();
    fs::write(path, package.archive()).unwrap();
    package
}

fn check_identity(value: &Value, package: &Package) {
    assert_eq!(
        value["package"]["archive_sha256"],
        format!("{:x}", Sha256::digest(package.archive()))
    );
    assert_eq!(
        value["package"]["module_sha256"],
        format!("{:x}", Sha256::digest(package.module()))
    );
    assert_eq!(value["package"]["archive_bytes"], package.archive().len());
    assert_eq!(value["package"]["module_bytes"], package.module().len());
    assert_eq!(value["package"]["id"], package.manifest().package_id);
    assert_eq!(
        value["package"]["version"],
        package.manifest().package_version
    );
}

#[test]
fn original_frozen_packages_prepare_without_execution_or_workbench_state() {
    let cwd = tempfile::tempdir().unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../sdk/compat/guest-v1-rc1");
    let mut stems = vec!["rust-provider".to_owned()];
    for language in ["c", "cpp", "rust"] {
        for profile in ["task", "transform", "ui", "dependency"] {
            stems.push(format!("{language}-{profile}"));
        }
    }
    for stem in stems {
        let package = frozen_sdk::package(&stem);
        let value = call(
            cwd.path(),
            &[root.join(format!("{stem}.mplugin")).into()],
            0,
        );
        assert_eq!(value["status"], "prepared");
        assert_eq!(value["phase"], "static_preparation");
        assert_eq!(value["error"], Value::Null);
        check_identity(&value, &package);
        let defaults = morrow_plugin_runtime::Limits::default();
        let budget = package.manifest().budget.as_ref().unwrap();
        assert_eq!(
            value["effective_limits"],
            json!({
                "fuel": defaults.fuel.min(budget.fuel),
                "memory_bytes": (defaults.memory_bytes as u64).min(budget.memory_bytes),
                "host_calls": defaults.host_calls.min(budget.host_calls)
            })
        );
        assert_eq!(
            fs::read(root.join(format!("{stem}.mplugin"))).unwrap(),
            package.archive()
        );
    }
}

#[test]
fn trap_entry_is_only_prepared_and_declared_limits_are_intersected() {
    let cwd = tempfile::tempdir().unwrap();
    let inputs = tempfile::tempdir().unwrap();
    let path = inputs.path().join("原始 插件 🪐.mplugin");
    let module = wat::parse_str(
        r#"(module (memory(export "memory") 1) (func(export "morrow_run")(result i32) unreachable))"#,
    ).unwrap();
    let package = synthesize(&path, &module, Some((1234, 65536, 0)));
    let value = call(cwd.path(), &[path.clone().into()], 0);
    assert_eq!(value["status"], "prepared");
    assert_eq!(
        value["effective_limits"],
        json!({"fuel":1234,"memory_bytes":65536,"host_calls":0})
    );
    assert_eq!(
        value["package"]["declared_limits"],
        value["effective_limits"]
    );
    check_identity(&value, &package);
    assert_eq!(fs::read(path).unwrap(), package.archive());

    let path = inputs.path().join("larger-declared-budget.mplugin");
    synthesize(&path, &module, Some((100_000_000, 64 * 1024 * 1024, 1024)));
    let value = call(cwd.path(), &[path.into()], 0);
    assert_eq!(value["effective_limits"], value["host_limits"]);
}

// Literal-only LZ4 block for intentionally invalid package fixtures. This does
// not modify, rebuild or reseal any frozen compatibility artifact.
fn invalid_archive(
    manifest: morrow_core::plugin_package::proto::Manifest,
    module: Vec<u8>,
) -> Vec<u8> {
    let raw = morrow_core::plugin_package::proto::Package {
        schema_version: 1,
        manifest: manifest.encode_to_vec(),
        module,
    }
    .encode_to_vec();
    assert!(raw.len() >= 15);
    let mut packed = vec![0xf0];
    let mut remaining = raw.len() - 15;
    while remaining >= 255 {
        packed.push(255);
        remaining -= 255;
    }
    packed.push(remaining as u8);
    packed.extend_from_slice(&raw);
    let mut archive = b"MORROWP1".to_vec();
    archive.extend_from_slice(&1u16.to_le_bytes());
    archive.extend_from_slice(&(raw.len() as u32).to_le_bytes());
    archive.extend_from_slice(&(packed.len() as u32).to_le_bytes());
    archive.extend_from_slice(&Sha256::digest(&raw));
    archive.extend_from_slice(&packed);
    archive
}

#[test]
fn unknown_feature_and_oversized_embedded_module_reject_before_preparation() {
    let cwd = tempfile::tempdir().unwrap();
    let inputs = tempfile::tempdir().unwrap();
    let module = b"\0asm\x01\0\0\0".to_vec();
    let mut manifest = Package::manifest_for("test.unknown-feature", "1.0.0", &module, vec![]);
    manifest
        .required_features
        .push("unknown-future-feature-v9".into());
    let unknown = invalid_archive(manifest, module);
    let mut large = vec![0; morrow_core::plugin_package::MAX_MODULE_BYTES + 1];
    large[..8].copy_from_slice(b"\0asm\x01\0\0\0");
    let manifest = Package::manifest_for("test.large-module", "1.0.0", &large, vec![]);
    for (name, bytes, error) in [
        ("unknown-feature", unknown, "UnsupportedVersion"),
        ("large-module", invalid_archive(manifest, large), "Limit"),
    ] {
        assert!(bytes.len() <= MAX_PACKAGE_BYTES);
        let path = inputs.path().join(format!("{name}.mplugin"));
        fs::write(&path, &bytes).unwrap();
        let value = call(cwd.path(), &[path.into()], 2);
        assert_eq!(value["phase"], "package_read_decode");
        assert_eq!(value["error"]["code"], "package_rejected");
        assert_eq!(value["error"]["message"], error);
        assert_eq!(value["package"], Value::Null);
    }
}

#[test]
fn forbidden_start_import_wrong_entry_and_corrupt_module_are_static_rejections() {
    let cwd = tempfile::tempdir().unwrap();
    let inputs = tempfile::tempdir().unwrap();
    let modules = [
        ("start", wat::parse_str(r#"(module (memory(export "memory") 1) (func $start unreachable) (start $start) (func(export "morrow_run")(result i32)i32.const 0))"#).unwrap()),
        ("import", wat::parse_str(r#"(module (import "env" "file" (func)) (memory(export "memory") 1) (func(export "morrow_run")(result i32)i32.const 0))"#).unwrap()),
        ("entry", wat::parse_str(r#"(module (memory(export "memory") 1) (func(export "morrow_run")))"#).unwrap()),
        ("module", b"\0asm\x01\0\0\0\xff".to_vec()),
    ];
    for (name, module) in modules {
        let path = inputs.path().join(format!("{name}.mplugin"));
        let package = synthesize(&path, &module, None);
        let value = call(cwd.path(), &[path.into()], 2);
        assert_eq!(value["status"], "rejected");
        assert_eq!(value["phase"], "static_preparation");
        assert_eq!(value["error"]["code"], "preparation_rejected");
        assert_eq!(value["effective_limits"], Value::Null);
        check_identity(&value, &package);
    }
}

#[test]
fn wrong_argument_counts_reject_before_package_access() {
    let cwd = tempfile::tempdir().unwrap();
    for args in [
        vec![],
        vec![OsString::new()],
        vec!["does-not-exist.mplugin".into(), "extra".into()],
        vec![cwd.path().into(), "extra".into()],
    ] {
        let value = call(cwd.path(), &args, 2);
        assert_eq!(value["phase"], "arguments");
        assert_eq!(value["error"]["code"], "invalid_arguments");
        assert_eq!(value["package"], Value::Null);
    }
}

#[test]
fn missing_directory_corrupt_and_oversize_archive_are_package_rejections() {
    let cwd = tempfile::tempdir().unwrap();
    let inputs = tempfile::tempdir().unwrap();
    let corrupt = inputs.path().join("corrupt.mplugin");
    fs::write(&corrupt, b"not an archive").unwrap();
    let oversized = inputs.path().join("oversized.mplugin");
    fs::File::create(&oversized)
        .unwrap()
        .set_len(MAX_PACKAGE_BYTES as u64 + 1)
        .unwrap();
    for (path, code) in [
        (inputs.path().join("absent.mplugin"), "package_unavailable"),
        (inputs.path().to_path_buf(), "invalid_file_type"),
        (corrupt, "package_rejected"),
        (oversized, "package_rejected"),
    ] {
        let value = call(cwd.path(), &[path.into()], 2);
        assert_eq!(value["phase"], "package_read_decode");
        assert_eq!(value["error"]["code"], code);
        assert_eq!(value["package"], Value::Null);
        assert_eq!(value["effective_limits"], Value::Null);
    }
}

#[cfg(unix)]
#[test]
fn symlink_and_device_are_rejected_and_non_utf8_regular_path_is_supported() {
    use std::os::unix::{ffi::OsStringExt, fs::symlink};
    let cwd = tempfile::tempdir().unwrap();
    let inputs = tempfile::tempdir().unwrap();
    let source =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../sdk/compat/guest-v1-rc1/rust-task.mplugin");
    let link = inputs.path().join("link.mplugin");
    symlink(&source, &link).unwrap();
    for path in [link, "/dev/null".into()] {
        let value = call(cwd.path(), &[path.into()], 2);
        assert_eq!(value["error"]["code"], "invalid_file_type");
    }
    let path = inputs
        .path()
        .join(OsString::from_vec(b"non-utf8-\xff.mplugin".to_vec()));
    fs::copy(source, &path).unwrap();
    assert_eq!(call(cwd.path(), &[path.into()], 0)["status"], "prepared");
}

#[test]
fn discovery_advertises_an_independent_static_diagnostic_with_original_defaults() {
    let cwd = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_morrow-workbench-host"))
        .arg("--sdk-capabilities")
        .current_dir(cwd.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        value["diagnostic_capabilities"]["package_preflight"],
        morrow_workbench_host::sdk_preflight::advertisement()
    );
    let rejection = call(cwd.path(), &[], 2);
    assert_eq!(
        rejection["host_limits"],
        value["profiles"][0]["runtime_defaults"]
    );
    assert_eq!(fs::read_dir(cwd.path()).unwrap().count(), 0);
}
