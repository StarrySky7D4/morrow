//! Actual three-language task consumers, not a product history disclosure route.
use morrow_blob_upload_history_v1::{INPUT_TYPE, Metadata, Packet, SCHEMA_DIGEST};
use morrow_core::{
    content::CardRecord,
    dispatch::HostRuntime,
    plugin_package::{Package, catalog::Catalog, proto::TransformHandler, registry::Registry},
    store::Store,
    task::{Invocation, Transform},
};
use morrow_plugin_runtime::{Limits, instance_pool::Pool, manager::Manager};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, path::PathBuf};

fn fixture(language: &str) {
    let module =
        std::fs::read(std::env::var(format!("MORROW_{language}_HISTORY_WASM")).unwrap()).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&module)),
        std::env::var(format!("MORROW_{language}_HISTORY_SHA256")).unwrap()
    );
    let bytes = std::fs::read(std::env::var("MORROW_HISTORY_ACK_VECTOR").unwrap()).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        std::env::var("MORROW_HISTORY_ACK_SHA256").unwrap()
    );
    let decoded = Packet::decode_for(&bytes, [1; 32], [9; 32]).unwrap();
    assert!(decoded.record().original_unknown());
    assert_eq!(decoded.record().revision(), 3);
    let mut reserved = decoded.record().clone();
    reserved.metadata = Metadata::Reserved;
    let reserved = Packet::new([9; 32], reserved).unwrap().encode().unwrap();
    let wrong_nonce = Packet::new([8; 32], decoded.record().clone())
        .unwrap()
        .encode()
        .unwrap();
    let mut other = decoded.record().clone();
    other.submission = [2; 32];
    let wrong_selection = Packet::new([9; 32], other).unwrap().encode().unwrap();
    let mut trailing = bytes.clone();
    trailing.extend([0; 8]);
    let truncated = bytes[..bytes.len() - 1].to_vec();
    let mut contract = bytes.clone();
    let at = contract
        .windows(32)
        .position(|w| w == SCHEMA_DIGEST)
        .unwrap();
    contract[at] ^= 1;
    let cases = [
        ("acknowledged-unknown", bytes.clone(), true),
        ("reserved-despite-staged", reserved, true),
        ("wrong-nonce", wrong_nonce, false),
        ("wrong-selection", wrong_selection, false),
        ("trailing", trailing, false),
        ("truncated", truncated, false),
        ("wrong-schema", contract, false),
        ("empty", Vec::new(), false),
        ("too-large", vec![0; 8193], false),
    ];
    let root = PathBuf::from(std::env::var("MORROW_HISTORY_GUEST_ROOT").unwrap()).join(language);
    assert!(!root.exists());
    std::fs::create_dir(&root).unwrap();
    let id = format!("test.history.{}", language.to_ascii_lowercase());
    let manifest = Package::manifest_for_transform(
        &id,
        "0.1.0",
        &module,
        vec![TransformHandler {
            handler: "history.fixture.inspect".into(),
            input_type: INPUT_TYPE.into(),
            output_type: INPUT_TYPE.into(),
            max_input_bytes: 16384,
            max_output_bytes: 8192,
        }],
    );
    let package = Package::build(manifest, &module).unwrap();
    let catalog = Catalog::open(&root.join("catalog")).unwrap();
    catalog.install(&package).unwrap();
    let mut manager = Manager::new(
        Registry::open(&root.join("registry"), catalog).unwrap(),
        Limits::default(),
    );
    manager.select(&package, manager.revision()).unwrap();
    let mut store = Store::open(&root.join("store"), Default::default()).unwrap();
    let seed = CardRecord::new(
        "untouched",
        "history.fixture",
        1,
        "Unrelated",
        b"keep".to_vec(),
    )
    .unwrap();
    store.create_local("seed", &seed).unwrap();
    let mut host = HostRuntime::new(store).unwrap();
    let mut pool = Pool::new(&host, Default::default()).unwrap();
    let unapproved_revision = manager.revision();
    assert!(
        pool.start(&mut manager, &mut host, &id, &[], unapproved_revision)
            .is_err()
    );
    manager
        .approve(&id, package.digest(), BTreeSet::new(), manager.revision())
        .unwrap();
    manager
        .set_enabled(&id, package.digest(), true, manager.revision())
        .unwrap();
    let revision = manager.revision();
    let session = pool
        .start(&mut manager, &mut host, &id, &[], revision)
        .unwrap();
    for (index, (name, input, accepted)) in cases.into_iter().enumerate() {
        let invocation = Invocation::new_transform(
            &format!("history-{index}"),
            Transform {
                handler: "history.fixture.inspect".into(),
                input_type: INPUT_TYPE.into(),
                output_type: INPUT_TYPE.into(),
                input: input.clone(),
            },
        )
        .unwrap();
        let report = pool
            .run_task(&manager, &mut host, &session, &invocation, || 0)
            .unwrap();
        assert_eq!(report.execution.outcome, Ok(0), "{language}/{name}");
        assert_eq!(report.execution.host_calls, 0);
        assert!(report.response.is_none());
        if accepted {
            assert!(report.failure.is_none());
            let output = report.output.unwrap();
            assert_eq!(output.type_id, INPUT_TYPE);
            assert_eq!(output.bytes, input);
            assert!(
                Packet::decode_for(&output.bytes, [1; 32], [9; 32])
                    .unwrap()
                    .record()
                    .original_unknown()
            );
        } else {
            assert!(report.output.is_none());
            assert_eq!(report.failure.unwrap().message, "History fixture rejected");
        }
        assert_eq!(
            host.store_local()
                .card("untouched")
                .unwrap()
                .unwrap()
                .encode(),
            seed.encode()
        );
        eprintln!("{language}/{name}: actual correlated task completion; no content calls");
    }
    manager
        .set_enabled(&id, package.digest(), false, manager.revision())
        .unwrap();
    let revoked = Invocation::new_transform(
        "revoked",
        Transform {
            handler: "history.fixture.inspect".into(),
            input_type: INPUT_TYPE.into(),
            output_type: INPUT_TYPE.into(),
            input: bytes,
        },
    )
    .unwrap();
    assert!(
        pool.run_task(&manager, &mut host, &session, &revoked, || 0)
            .is_err()
    );
    pool.close_all(&mut host).unwrap();
    assert_eq!(
        host.store_local()
            .card("untouched")
            .unwrap()
            .unwrap()
            .encode(),
        seed.encode()
    );
}

#[test]
fn rust_history_task_checks_packet_and_approval() {
    fixture("RUST");
}
#[test]
fn c_history_task_checks_packet_and_approval() {
    fixture("C");
}
#[test]
fn cpp_history_task_checks_packet_move_and_approval() {
    fixture("CPP");
}
