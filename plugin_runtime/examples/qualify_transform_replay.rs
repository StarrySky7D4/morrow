//! Actual Rust SDK transforms captured online, then replayed without the original host or catalog.
//! PASS_SCOPED: unsigned observations, not sealed audit or a host content-projection replay.
use morrow_core::{
    dispatch::HostRuntime,
    plugin_package::{Package, catalog::Catalog, proto::TransformHandler, registry::Registry},
    store::Store,
    task::{FailureCode, Invocation, Transform},
    task_evidence,
};
use morrow_plugin_runtime::{Limits, instance_pool::Pool, manager::Manager, replay};
use std::{io::Write, path::Path};

const ID: &str = "org.test.replay.rust-transform";
fn invocation(id: &str, handler: &str, input: &[u8]) -> morrow_core::Result<Invocation> {
    Invocation::new_transform(
        id,
        Transform {
            handler: handler.into(),
            input_type: "bytes".into(),
            output_type: "bytes".into(),
            input: input.to_vec(),
        },
    )
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
fn write_new(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 2 {
        return Err(
            "usage: qualify_transform_replay ACTUAL_RUST_TRANSFORM.wasm NEW_EVIDENCE_PATH".into(),
        );
    }
    let destination = Path::new(&args[1]);
    let failure_destination = destination.with_extension("failure.morrowevidence");
    if destination == failure_destination || destination.exists() || failure_destination.exists() {
        return Err("two distinct new evidence output paths are required".into());
    }
    let directory = tempfile::tempdir()?;
    let original_root = directory.path().to_path_buf();
    let (success_bytes, success_digest, failure_bytes, failure_digest) = {
        let module = std::fs::read(&args[0])?;
        let manifest = Package::manifest_for_transform(
            ID,
            "1.0.0",
            &module,
            ["bytes.reverse", "bytes.require-ascii"]
                .into_iter()
                .map(|handler| TransformHandler {
                    handler: handler.into(),
                    input_type: "bytes".into(),
                    output_type: "bytes".into(),
                    max_input_bytes: 65536,
                    max_output_bytes: 65536,
                })
                .collect(),
        );
        let package = Package::build(manifest, &module)?;
        assert!(package.capabilities().is_empty());
        let catalog = Catalog::open(&original_root.join("packages"))?;
        catalog.install(&package)?;
        let mut manager = Manager::new(
            Registry::open(&original_root.join("registry"), catalog)?,
            Limits::default(),
        );
        manager.select(&package, manager.revision())?;
        manager.approve(ID, package.digest(), Default::default(), manager.revision())?;
        manager.set_enabled(ID, package.digest(), true, manager.revision())?;
        let mut host = HostRuntime::new(Store::open(
            &original_root.join("synthetic.db"),
            Default::default(),
        )?)?;
        let mut pool = Pool::new(&host, Default::default())?;
        let revision = manager.revision();
        let session = pool.start(&mut manager, &mut host, ID, &[], revision)?;
        let reverse = invocation("rust-replay-reverse", "bytes.reverse", b"Abc\0\xff")?;
        let success = pool.record_transform(&manager, &mut host, &session, &reverse)?;
        let report = success.report();
        assert_eq!(report.execution.outcome, Ok(0));
        assert_eq!(report.execution.host_calls, 0);
        assert!(report.response.is_none() && report.failure.is_none());
        let output = report.output.as_ref().expect("real Rust reverse output");
        assert_eq!(output.type_id, "bytes");
        assert_eq!(output.bytes, b"\xff\0cbA");
        let rejected = invocation(
            "rust-replay-ascii-failure",
            "bytes.require-ascii",
            "世界".as_bytes(),
        )?;
        let failure = pool.record_transform(&manager, &mut host, &session, &rejected)?;
        let report = failure.report();
        assert_eq!(report.execution.outcome, Ok(0));
        assert_eq!(report.execution.host_calls, 0);
        assert!(report.response.is_none() && report.output.is_none());
        let reason = report
            .failure
            .as_ref()
            .expect("real correlated Rust business failure");
        assert_eq!(reason.code, FailureCode::UnsupportedInput);
        assert_eq!(reason.message, "Input contains non-ASCII bytes");
        assert!(
            pool.root(&session).is_ok(),
            "business rejection keeps the root healthy"
        );
        assert_eq!(
            host.store_local().card_ids_local("", 128)?,
            Vec::<String>::new()
        );
        assert!(host.store_local().pending(0, 1)?.is_empty());
        host.store_local().integrity_check()?;
        let artifacts = (
            success.evidence().container().to_vec(),
            success.evidence().digest(),
            failure.evidence().container().to_vec(),
            failure.evidence().digest(),
        );
        pool.close(&mut host, &session)?;
        assert_eq!((pool.usage().sessions, pool.usage().providers), (0, 0));
        artifacts
    }; // Every live Pool, Session, HostRuntime, Manager, Package and capture is dropped here.
    directory.close()?;
    assert!(
        !original_root.exists(),
        "the original catalog and DB have been removed"
    );
    let success = task_evidence::decode(&success_bytes, success_digest)?;
    let failure = task_evidence::decode(&failure_bytes, failure_digest)?;
    let success_replay = replay::replay(&success, Limits::default())?;
    assert!(
        success_replay.matches,
        "exact pure execution observation must match"
    );
    assert_eq!(
        success_replay.report.output.as_ref().unwrap().bytes,
        b"\xff\0cbA"
    );
    assert!(success_replay.report.failure.is_none());
    let failure_replay = replay::replay(&failure, Limits::default())?;
    assert!(
        failure_replay.matches,
        "structured business failure must match exactly"
    );
    assert!(failure_replay.report.output.is_none());
    let reason = failure_replay.report.failure.as_ref().unwrap();
    assert_eq!(reason.code, FailureCode::UnsupportedInput);
    assert_eq!(reason.message, "Input contains non-ASCII bytes");
    write_new(destination, success.container())?;
    write_new(&failure_destination, failure.container())?;
    println!(
        "PASS_SCOPED actual Rust SDK: reverse output + structured ASCII rejection; original host/DB/catalog removed before decode/replay; zero cards/events; pool resources released."
    );
    println!("UNSIGNED: no signed audit association; no content commit or host projection replay.");
    println!(
        "success={} evidence_digest={}",
        destination.display(),
        hex(&success_digest)
    );
    println!(
        "failure={} evidence_digest={}",
        failure_destination.display(),
        hex(&failure_digest)
    );
    Ok(())
}
