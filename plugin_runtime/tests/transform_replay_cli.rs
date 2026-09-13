#![cfg(all(feature = "packages", not(target_arch = "wasm32")))]
//! Real CLI processes replay independently after the capture host and catalog are gone.
use morrow_core::{
    dispatch::HostRuntime,
    plugin_package::{Package, catalog::Catalog, proto::TransformHandler, registry::Registry},
    store::Store,
    task::{Invocation, Transform},
    task_evidence::{self, Evidence},
};
use morrow_plugin_runtime::{Limits, instance_pool::Pool, manager::Manager};
use std::{
    collections::BTreeSet,
    fs,
    process::{Command, Output},
};

fn invocation() -> Invocation {
    Invocation::new_transform(
        "cli-task",
        Transform {
            handler: "bytes.copy".into(),
            input_type: "bytes".into(),
            output_type: "bytes".into(),
            input: b"abc".to_vec(),
        },
    )
    .unwrap()
}
fn capture(batch: bool) -> Evidence {
    let input = invocation();
    let completion = input.output_completion(b"abc").unwrap();
    let escaped: String = completion.iter().map(|b| format!("\\{b:02x}")).collect();
    let wasm = wat::parse_str(format!(
        r#"(module
        (import "morrow_task_v1" "read_input" (func $read (param i32 i32) (result i32)))
        (import "morrow_task_v1" "complete" (func $done (param i32 i32) (result i32)))
        (memory (export "memory") 4)
        (data (i32.const 160000) "{escaped}")
        (func (export "morrow_run") (result i32)
            i32.const 0 i32.const 131072 call $read i32.const {} i32.ne if unreachable end
            i32.const 160000 i32.const {} call $done drop i32.const 0))"#,
        input.bytes().len(),
        completion.len()
    ))
    .unwrap();
    let manifest = Package::manifest_for_transform(
        "test.cli-replay",
        "1.0.0",
        &wasm,
        vec![TransformHandler {
            handler: "bytes.copy".into(),
            input_type: "bytes".into(),
            output_type: "bytes".into(),
            max_input_bytes: 65536,
            max_output_bytes: 65536,
        }],
    );
    let package = Package::build(manifest, &wasm).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let catalog = Catalog::open(&dir.path().join("catalog")).unwrap();
    catalog.install(&package).unwrap();
    let budget = Limits {
        fuel: 100_000,
        memory_bytes: 4 * 65536,
        host_calls: 4,
    };
    let mut manager = Manager::new(
        Registry::open(&dir.path().join("registry"), catalog).unwrap(),
        budget,
    );
    manager.select(&package, manager.revision()).unwrap();
    manager
        .approve(
            "test.cli-replay",
            package.digest(),
            BTreeSet::new(),
            manager.revision(),
        )
        .unwrap();
    manager
        .set_enabled(
            "test.cli-replay",
            package.digest(),
            true,
            manager.revision(),
        )
        .unwrap();
    let mut host =
        HostRuntime::new(Store::open(&dir.path().join("db"), Default::default()).unwrap()).unwrap();
    let mut pool = Pool::new(&host, Default::default()).unwrap();
    let revision = manager.revision();
    let session = pool
        .start(&mut manager, &mut host, "test.cli-replay", &[], revision)
        .unwrap();
    let evidence = if batch {
        pool.record_transform_batch(
            &manager,
            &mut host,
            &session,
            &[invocation(), invocation()],
            "test.cli-intent",
            b"ordered",
            1_000_000_000,
        )
        .unwrap()
        .into_parts()
        .1
    } else {
        pool.record_transform(&manager, &mut host, &session, &input)
            .unwrap()
            .into_parts()
            .1
    };
    pool.close_all(&mut host).unwrap();
    drop(pool);
    drop(host);
    drop(manager);
    dir.close().unwrap();
    evidence
}
fn invoke(container: &[u8], digest: [u8; 32]) -> Output {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("capture.morrowevidence");
    fs::write(&path, container).unwrap();
    let pin: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    let mut command = Command::new(env!("CARGO_BIN_EXE_morrow-transform-replay"));
    command.arg(path).arg(pin).current_dir(dir.path());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW for the console verifier.
    }
    command.output().unwrap()
}
fn assert_result(output: &Output, code: i32, text: &str) {
    assert_eq!(
        output.status.code(),
        Some(code),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains(text),
        "{:?}",
        output
    );
}
#[test]
fn v1_actual_process_matches_after_capture_host_is_removed() {
    let evidence = capture(false);
    let output = invoke(evidence.container(), evidence.digest());
    assert_result(&output, 0, "MATCH: pure-transform observations=1/1");
    assert!(output.stderr.is_empty());
}
#[test]
fn v2_actual_process_matches_at_fixed_total_policy_boundary() {
    let evidence = capture(true);
    let output = invoke(evidence.container(), evidence.digest());
    assert_result(&output, 0, "MATCH: pure-transform observations=2/2");
    assert!(output.stderr.is_empty());
}
#[test]
fn tampered_container_and_wrong_pin_are_refused_before_replay() {
    for batch in [false, true] {
        let evidence = capture(batch);
        let mut pin = evidence.digest();
        pin[0] ^= 1;
        let mut container = evidence.container().to_vec();
        container[0] ^= 1;
        for output in [
            invoke(evidence.container(), pin),
            invoke(&container, evidence.digest()),
        ] {
            assert_eq!(output.status.code(), Some(1), "{output:?}");
            assert!(output.stdout.is_empty());
            assert!(String::from_utf8_lossy(&output.stderr).contains("Replay refused:"));
        }
    }
}
#[test]
fn v2_declared_total_cannot_raise_trusted_cli_policy() {
    let evidence = capture(true);
    let mut data = evidence.data().clone();
    data.batch.as_mut().unwrap().total_fuel = 1_000_000_001;
    let evidence = task_evidence::encode(data).unwrap();
    let output = invoke(evidence.container(), evidence.digest());
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Policy"));
}
#[test]
fn correctly_pinned_v1_mismatch_has_distinct_exit_code() {
    let evidence = capture(false);
    let mut data = evidence.data().clone();
    data.completion = invocation()
        .output_completion(b"different observation")
        .unwrap();
    let evidence = task_evidence::encode(data).unwrap();
    let output = invoke(evidence.container(), evidence.digest());
    assert_result(&output, 2, "MISMATCH: pure-transform observations=1/1");
    assert!(output.stderr.is_empty());
}
#[test]
fn correctly_pinned_v2_mismatch_reports_only_executed_prefix() {
    let evidence = capture(true);
    let mut data = evidence.data().clone();
    data.batch.as_mut().unwrap().observations[0].completion = invocation()
        .output_completion(b"different observation")
        .unwrap();
    let evidence = task_evidence::encode(data).unwrap();
    let output = invoke(evidence.container(), evidence.digest());
    assert_result(&output, 2, "MISMATCH: pure-transform observations=1/2");
    assert!(output.stderr.is_empty());
}
