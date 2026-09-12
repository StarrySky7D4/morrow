//! Real Rust guest qualification for the native frozen-input path, using synthetic data only.
use morrow_core::{
    dispatch::HostRuntime,
    plugin_package::catalog,
    shared_object::Descriptor,
    store::{EventBudget, Store},
    ui::Document,
};
use morrow_plugin_runtime::{
    Cancellation,
    package::PreparedPackage,
    shared_objects::{Limits, SharedObjects, TransformRequest},
};
use std::path::Path;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let package = std::env::args()
        .nth(1)
        .ok_or("compiled workbench package required")?;
    let prepared =
        PreparedPackage::new(catalog::read_file(Path::new(&package))?, Default::default())
            .map_err(|e| format!("prepare: {e:?}"))?;
    let dir = tempfile::tempdir()?;
    let mut host = HostRuntime::new(Store::open(
        &dir.path().join("synthetic.db"),
        EventBudget::default(),
    )?)?;
    let producer = prepared.connect_approved(&mut host, &Default::default())?;
    let consumer = prepared.connect_approved(&mut host, &Default::default())?;
    let mut objects = SharedObjects::new(
        &host,
        Limits {
            objects: 1,
            charged_bytes: 65536,
            ..Default::default()
        },
    )?;
    let mut original = b"abc".to_vec();
    let descriptor = objects.publish(&host, &producer, "qualification", &original, 1)?;
    original.fill(b'z');
    let descriptor = Descriptor::decode(&descriptor.encode()?)?;
    let lease = objects.grant(&host, &consumer, &descriptor, "qualification", 10, 2)?;
    let mapping = objects.map(&host, &consumer, &lease, 3)?;
    let mut ticks = [4, 5].into_iter();
    let result = objects.run_transform(
        &mut host,
        &consumer,
        &prepared,
        &mapping,
        TransformRequest {
            task_id: "shared-rust",
            handler: "ui.form",
            input_type: "text.utf8",
            output_type: "morrow.ui.document.v1",
        },
        || ticks.next().expect("pure transform clock"),
        Cancellation::default(),
    )?;
    assert_eq!(result.execution.outcome, Ok(0));
    assert_eq!(result.execution.host_calls, 0);
    assert!(result.response.is_none() && result.failure.is_none());
    let output = result.output.ok_or("missing actual guest output")?;
    let ui = Document::decode(&output.bytes)?;
    assert_eq!(
        ui.nodes().iter().find(|n| n.id == "text").unwrap().text,
        "abc"
    );
    assert_eq!(
        ui.nodes().iter().find(|n| n.id == "preview").unwrap().text,
        "ABC"
    );
    assert_eq!(mapping.bytes(), b"abc");
    assert!(host.store_local().card_ids_local("", 1)?.is_empty());
    objects.retire(&descriptor)?;
    assert!(objects.map(&host, &consumer, &lease, 6).is_err());
    assert_eq!(mapping.bytes(), b"abc");
    assert!(
        objects
            .publish(&host, &producer, "another-scope", b"new", 6)
            .is_err()
    );
    assert_eq!(objects.usage().charged_bytes, 65536);
    drop(mapping);
    assert_eq!(objects.usage().charged_bytes, 0);
    let next = objects.publish(&host, &producer, "another-scope", b"new", 7)?;
    assert_ne!(descriptor.object, next.object);
    host.disconnect(&consumer)?;
    host.disconnect(&producer)?;
    println!(
        "PASS_SCOPED: actual Rust Wasm consumed frozen abc and returned ABC; no cards; retired mapping stayed charged until release; next scope received a new object. Not a full A-to-B dependency/commit/audit pipeline."
    );
    Ok(())
}
