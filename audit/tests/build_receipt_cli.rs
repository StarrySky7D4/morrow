//! Real CLI and Cargo processes against tiny synthetic projects. These tests do
//! not qualify the application or network implementation; the production capture
//! is a separately recorded run using the actual project source snapshot.
use morrow_audit::build_receipt;
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};
use tempfile::TempDir;

fn git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
fn fixture(compile: bool) -> TempDir {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project with spaces");
    fs::create_dir_all(root.join("network_node/src")).unwrap();
    fs::write(
        root.join("network_node/Cargo.toml"),
        r#"[workspace]
[package]
name="fixture-node"
version="0.0.0"
edition="2021"
[features]
plugin-adapter=[]
[[bin]]
name="morrow-api-node"
path="src/main.rs"
"#,
    )
    .unwrap();
    fs::write(
        root.join("network_node/src/main.rs"),
        if compile {
            "fn main() { println!(\"synthetic receipt fixture; not Morrow\"); }"
        } else {
            "fn main() { not_valid_rust!(); }"
        },
    )
    .unwrap();
    // --locked is part of the production recipe and must work for the fixture too.
    let output = Command::new("cargo")
        .args([
            "generate-lockfile",
            "--offline",
            "--manifest-path",
            "network_node/Cargo.toml",
        ])
        .current_dir(&root)
        .output()
        .unwrap();
    assert!(output.status.success());
    git(&root, &["init", "-q"]);
    git(&root, &["add", "."]);
    git(
        &root,
        &[
            "-c",
            "user.name=Receipt Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-q",
            "-m",
            "synthetic source",
        ],
    );
    temp
}
fn cli(args: &[&std::ffi::OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_morrow-build-receipt"))
        .args(args)
        .output()
        .unwrap()
}
fn capture(temp: &TempDir) -> Output {
    cli(&[
        "capture".as_ref(),
        temp.path().join("project with spaces").as_os_str(),
        temp.path().join("run").as_os_str(),
    ])
}
fn verify(run: &Path) -> Output {
    cli(&["verify".as_ref(), run.as_os_str()])
}
#[test]
fn real_cargo_build_is_bound_to_preserved_source_logs_and_artifact() {
    let temp = fixture(true);
    let output = capture(&temp);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let run = temp.path().join("run");
    let receipt =
        build_receipt::decode(&fs::read(run.join("build-receipt.pb.lz4")).unwrap()).unwrap();
    assert!(receipt.build_succeeded);
    assert_eq!(receipt.commands[0].exit_code, 0);
    assert_eq!(receipt.artifacts.len(), 1);
    assert!(verify(&run).status.success());
    // Later original checkout edits do not alter the source actually compiled.
    fs::write(
        temp.path()
            .join("project with spaces/network_node/src/main.rs"),
        "later original edit",
    )
    .unwrap();
    assert!(verify(&run).status.success());
    let artifact = run.join(&receipt.artifacts[0].path);
    let original = fs::read(&artifact).unwrap();
    fs::write(&artifact, b"unrelated old artifact").unwrap();
    assert_eq!(verify(&run).status.code(), Some(2));
    fs::write(&artifact, original).unwrap();
    assert!(verify(&run).status.success());
    let report = cli(&["report".as_ref(), run.as_os_str()]);
    assert!(report.status.success());
    let text = String::from_utf8(report.stdout).unwrap();
    assert!(text.contains("Source-set SHA-256"));
    assert!(text.contains("No guest execution"));
    fs::write(run.join("source/unlisted-source.rs"), b"extra input").unwrap();
    assert_eq!(verify(&run).status.code(), Some(2));
}
#[test]
fn compiler_failure_is_recorded_and_not_promoted_to_build_success() {
    let temp = fixture(false);
    let output = capture(&temp);
    assert_eq!(
        output.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let run = temp.path().join("run");
    let receipt =
        build_receipt::decode(&fs::read(run.join("build-receipt.pb.lz4")).unwrap()).unwrap();
    assert!(!receipt.build_succeeded);
    assert!(receipt.artifacts.is_empty());
    assert_ne!(receipt.commands[0].exit_code, 0);
    assert_eq!(verify(&run).status.code(), Some(1));
    fs::write(run.join("logs/build.stderr"), b"concealed failure").unwrap();
    assert_eq!(verify(&run).status.code(), Some(2));
}
#[test]
fn existing_output_and_incomplete_capture_are_never_overwritten_or_qualified() {
    let temp = fixture(true);
    let run = temp.path().join("run");
    fs::create_dir(&run).unwrap();
    fs::write(run.join("keep"), b"existing output").unwrap();
    assert_eq!(capture(&temp).status.code(), Some(2));
    assert_eq!(fs::read(run.join("keep")).unwrap(), b"existing output");
    assert_eq!(verify(&run).status.code(), Some(2));
    assert!(!run.join("build-receipt.pb.lz4").exists());
}
