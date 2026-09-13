use super::*;
use morrow_core::plugin_package::{Package, proto::TransformHandler};

#[path = "../../tests/support/frozen_sdk.rs"]
mod frozen_sdk;

fn package_path(stem: &str) -> std::path::PathBuf {
    // Validate the sealed baseline first; neither this test nor the CLI rebuilds it.
    let _ = frozen_sdk::package(stem);
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../sdk/compat/guest-v1-rc1")
        .join(format!("{stem}.mplugin"))
}
fn args(package: &Path, handler: &str, input: &Path, output: &Path) -> Vec<OsString> {
    vec![
        "transform".into(),
        package.into(),
        handler.into(),
        "bytes".into(),
        "bytes".into(),
        input.into(),
        output.into(),
    ]
}
fn error(args: &[OsString], expected: u8) -> CliError {
    let error = run(args).unwrap_err();
    assert_eq!(error.code, expected, "{}", error.message);
    error
}
fn assert_no_output(path: &Path) {
    assert!(
        fs::symlink_metadata(path).is_err(),
        "failed task left an output file"
    );
}

#[test]
fn frozen_three_language_checks_and_binary_transforms() {
    for language in ["rust", "c", "cpp"] {
        let package = package_path(&format!("{language}-transform"));
        let message = run(&["check".into(), package.clone().into_os_string()]).unwrap();
        assert!(message.contains("not executed, installed, or authorized"));
        assert!(message.contains("runtime_protocol=7"));
        assert!(message.contains("handler=bytes.reverse"));
        assert!(message.contains("effective_budget fuel="));
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("input");
        for (index, bytes) in [vec![], b"a\0\xffz".to_vec(), vec![0x81; MAX_VALUE_BYTES]]
            .iter()
            .enumerate()
        {
            fs::write(&input, bytes).unwrap();
            let output = dir.path().join(format!("output-{index}"));
            let message = run(&args(&package, "bytes.reverse", &input, &output)).unwrap();
            let mut expected = bytes.clone();
            expected.reverse();
            assert_eq!(fs::read(&output).unwrap(), expected);
            assert!(message.contains(&format!(
                "bytes={} sha256={:x}",
                expected.len(),
                Sha256::digest(&expected)
            )));
        }
    }
}

#[test]
fn frozen_business_failures_are_not_empty_successes() {
    for language in ["rust", "c", "cpp"] {
        let package = package_path(&format!("{language}-transform"));
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("input");
        let output = dir.path().join("output");
        fs::write(&input, [0xff]).unwrap();
        let failed = error(&args(&package, "bytes.require-ascii", &input, &output), 3);
        assert!(failed.message.contains("code=unsupportedInput"));
        assert!(failed.message.contains("Input contains non-ASCII bytes"));
        assert_no_output(&output);
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}

#[test]
fn mismatched_handler_and_types_reject_without_output() {
    let package = package_path("rust-transform");
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input");
    let output = dir.path().join("output");
    fs::write(&input, b"abc").unwrap();
    for index in [2, 3, 4] {
        let mut args = args(&package, "bytes.reverse", &input, &output);
        args[index] = "unregistered".into();
        assert!(error(&args, 4).message.contains("TaskProtocol"));
        assert_no_output(&output);
    }
}

#[test]
fn existing_output_and_same_input_path_are_never_overwritten() {
    let package = package_path("rust-transform");
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input");
    let output = dir.path().join("output");
    fs::write(&input, b"abc").unwrap();
    fs::write(&output, b"cba").unwrap();
    assert!(
        error(&args(&package, "bytes.reverse", &input, &output), 5)
            .message
            .contains("already exists")
    );
    assert_eq!(fs::read(&output).unwrap(), b"cba");
    error(&args(&package, "bytes.reverse", &input, &input), 5);
    assert_eq!(fs::read(&input).unwrap(), b"abc");
    assert_eq!(save_output(&output, b"overwrite").unwrap_err().code, 5);
    assert_eq!(fs::read(&output).unwrap(), b"cba");
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
}

#[test]
fn mandatory_dependencies_require_the_managed_dependency_host() {
    let package = package_path("rust-dependency");
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input");
    let output = dir.path().join("output");
    fs::write(&input, b"abc").unwrap();
    let result = error(&args(&package, "bytes.dependency-wrap", &input, &output), 2);
    assert!(
        result
            .message
            .contains("explicitly approved dependency host")
    );
    assert_no_output(&output);
}

#[test]
fn arguments_and_extra_input_byte_are_bounded() {
    for args in [
        vec![],
        vec!["check".into()],
        vec!["transform".into()],
        vec!["unknown".into()],
    ] {
        assert_eq!(error(&args, 2).message, USAGE);
    }
    let package = package_path("rust-transform");
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input");
    let output = dir.path().join("output");
    fs::write(&input, vec![0; MAX_VALUE_BYTES + 1]).unwrap();
    assert!(
        error(&args(&package, "bytes.reverse", &input, &output), 2)
            .message
            .contains("exceeds 65536")
    );
    assert_no_output(&output);
    fs::write(&input, b"abc").unwrap();
    let mut invalid = args(&package, "bytes.reverse", &input, &output);
    invalid[2] = "bad\nidentity".into();
    assert!(
        error(&invalid, 2)
            .message
            .contains("invalid transform arguments")
    );
    assert_no_output(&output);
    let mut extra = args(&package, "bytes.reverse", &input, &output);
    extra.push("extra".into());
    assert_eq!(error(&extra, 2).message, USAGE);
}

fn negative_package(path: &Path, module: &[u8]) {
    let manifest = Package::manifest_for_transform(
        "test.plugin-check.negative",
        "1.0.0",
        module,
        vec![TransformHandler {
            handler: "bytes.reverse".into(),
            input_type: "bytes".into(),
            output_type: "bytes".into(),
            max_input_bytes: 65536,
            max_output_bytes: 65536,
        }],
    );
    // Only malformed/rejected test modules are synthesized, never replacement compatibility guests.
    let package = Package::build(manifest, module).unwrap();
    fs::write(path, package.archive()).unwrap();
}

#[test]
fn check_rejects_forbidden_import_start_wrong_entry_and_invalid_wasm() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("negative.mplugin");
    let mut modules = [
        r#"(module (import "env" "file" (func)) (memory(export "memory") 1) (func(export "morrow_run")(result i32)i32.const 0))"#,
        r#"(module (memory(export "memory") 1) (func $start) (start $start) (func(export "morrow_run")(result i32)i32.const 0))"#,
        r#"(module (memory(export "memory") 1) (func(export "morrow_run")))"#,
    ].map(|source| wat::parse_str(source).unwrap()).to_vec();
    modules.push(b"\0asm\x01\0\0\0\xff".to_vec());
    for module in modules {
        negative_package(&path, &module);
        assert!(
            error(&["check".into(), path.clone().into_os_string()], 2)
                .message
                .contains("prepare rejected")
        );
    }
}

#[test]
fn trap_after_successful_prepare_is_execution_failure_without_output() {
    let dir = tempfile::tempdir().unwrap();
    let package = dir.path().join("trap.mplugin");
    let input = dir.path().join("input");
    let output = dir.path().join("output");
    negative_package(&package, &wat::parse_str(r#"(module (memory(export "memory") 1) (func(export "morrow_run")(result i32)unreachable))"#).unwrap());
    assert!(run(&["check".into(), package.clone().into_os_string()]).is_ok());
    fs::write(&input, b"abc").unwrap();
    assert!(
        error(&args(&package, "bytes.reverse", &input, &output), 4)
            .message
            .contains("Trap")
    );
    assert_no_output(&output);
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
}
