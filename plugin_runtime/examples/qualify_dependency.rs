//! Two independently compiled Rust guests, routed by the host into an authorized edit proposal.
use morrow_core::{
    content::CardRecord,
    dispatch::HostRuntime,
    lifecycle::GrantKind,
    plugin_package::{
        Package,
        proto::{Capability, TransformHandler},
    },
    store::Store,
};
use morrow_plugin_runtime::{
    Cancellation,
    dependency::{Dependency, Endpoint, Spec},
    package::PreparedPackage,
    proposal::{EditProposal, EditTarget},
    shared_objects::{Limits, SharedObjects, TransformRequest},
};
fn prepared(path: &str, id: &str, handler: &str, caller: bool) -> PreparedPackage {
    let module = std::fs::read(path).unwrap();
    let mut manifest = Package::manifest_for_transform(
        id,
        "0.1.9-test.28",
        &module,
        vec![TransformHandler {
            handler: handler.into(),
            input_type: "bytes".into(),
            output_type: "bytes".into(),
            max_input_bytes: 65534,
            max_output_bytes: 65536,
        }],
    );
    if caller {
        manifest.requested_capabilities = vec![
            Capability::ReadContent as i32,
            Capability::EditContent as i32,
        ];
    }
    PreparedPackage::new(
        Package::build(manifest, &module).unwrap(),
        Default::default(),
    )
    .unwrap()
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    assert_eq!(args.len(), 2, "caller and provider Rust Wasm required");
    assert_ne!(
        std::fs::read(&args[0])?,
        std::fs::read(&args[1])?,
        "must be distinct modules"
    );
    let a = prepared(&args[0], "org.test.caller", "bytes.ascii-uppercase", true);
    let b = prepared(&args[1], "org.test.provider", "bytes.tag-reverse", false);
    let root = tempfile::tempdir()?;
    let path = root.path().join("synthetic.db");
    let mut host = HostRuntime::new(Store::open(&path, Default::default())?)?;
    let mut admin = host.connect()?;
    host.grant(&mut admin, GrantKind::CreateContent, "card", 100, 1)?;
    host.create_content(
        &admin,
        "seed",
        &CardRecord::new("card", "org.test.bytes", 1, "Original", b"abc".to_vec())?,
        || 1,
    )?;
    host.disconnect(&admin)?;
    let mut caller = a.connect_approved(
        &mut host,
        &[GrantKind::ReadContent, GrantKind::EditContent].into(),
    )?;
    let provider = b.connect_approved(&mut host, &Default::default())?;
    for kind in [GrantKind::ReadContent, GrantKind::EditContent] {
        host.grant(&mut caller, kind, "card", 100, 1)?;
    }
    let original = host.read_content(&caller, "card", || 1)?;
    let mut source = original.body();
    let mut objects = SharedObjects::new(&host, Limits::default())?;
    let input = objects.publish(&host, &caller, "workspace:demo", &source, 1)?;
    source.fill(b'z');
    let input_lease = objects.grant(&host, &caller, &input, "workspace:demo", 100, 1)?;
    let input_map = objects.map(&host, &caller, &input_lease, 1)?;
    let first = objects.run_transform(
        &mut host,
        &caller,
        &a,
        &input_map,
        TransformRequest {
            task_id: "stage-a",
            handler: "bytes.ascii-uppercase",
            input_type: "bytes",
            output_type: "bytes",
        },
        || 1,
        Cancellation::default(),
    )?;
    assert_eq!(first.execution.outcome, Ok(0));
    assert_eq!(first.execution.host_calls, 0);
    let first = first.output.ok_or("caller did not produce output")?;
    assert_eq!(first.bytes, b"ABC");
    let intermediate = objects.publish(&host, &caller, "workspace:demo", &first.bytes, 1)?;
    let own_lease = objects.grant(&host, &caller, &intermediate, "workspace:demo", 100, 1)?;
    let own_map = objects.map(&host, &caller, &own_lease, 1)?;
    let route = Dependency::bind(
        &host,
        Endpoint {
            package: &a,
            connection: &caller,
        },
        Endpoint {
            package: &b,
            connection: &provider,
        },
        Spec {
            handler: "bytes.tag-reverse",
            input_type: "bytes",
            output_type: "bytes",
            scope: "workspace:demo",
            expires: 50,
        },
        1,
    )?;
    let before = objects.usage();
    let output = route.run(
        &mut objects,
        &mut host,
        Endpoint {
            package: &a,
            connection: &caller,
        },
        Endpoint {
            package: &b,
            connection: &provider,
        },
        &own_map,
        "stage-b",
        || 1,
        Cancellation::default(),
    )?;
    assert_eq!(output.bytes(), b"B:CBA");
    assert_eq!(output.input_descriptor(), &intermediate);
    assert_eq!(
        objects.usage(),
        before,
        "provider's temporary access must be released"
    );
    assert_eq!(host.read_content(&caller, "card", || 1)?.body(), b"abc");
    let proposal = EditProposal::new(
        output,
        EditTarget {
            operation_id: "apply-a-b",
            card_id: "card",
            expected_revision: 1,
            title: "Transformed",
            preview: "B:CBA",
            accepted_output_type: "bytes",
        },
    )?;
    let receipt = proposal.commit(&mut host, &caller, || 1)?;
    assert_eq!(receipt.revision, 2);
    assert_eq!(receipt, proposal.commit(&mut host, &caller, || 1)?);
    assert_eq!(host.read_content(&caller, "card", || 1)?.body(), b"B:CBA");
    route.revoke();
    assert!(proposal.commit(&mut host, &caller, || 1).is_err());
    assert_eq!(host.store_local().pending(0, 10)?.len(), 2);
    objects.retire(&input)?;
    objects.retire(&intermediate)?;
    drop(input_map);
    drop(own_map);
    assert_eq!(objects.usage().charged_bytes, 0);
    host.store_local().integrity_check()?;
    host.disconnect(&caller)?;
    host.disconnect(&provider)?;
    drop(host);
    let reopened = Store::open_existing(&path, Default::default())?;
    assert_eq!(
        reopened
            .card("card")?
            .ok_or("missing persisted card")?
            .body(),
        b"B:CBA"
    );
    reopened.integrity_check()?;
    println!(
        "PASS_SCOPED: distinct Rust A abc->ABC, frozen host route to Rust B ->B:CBA, authorized revision-2 edit, identical retry receipt, revoked result rejected, two persistent core events. Host-routed dependency; dynamic guest calls, full provenance audit and replay pending."
    );
    Ok(())
}
