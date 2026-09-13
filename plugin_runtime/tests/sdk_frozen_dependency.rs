//! Frozen guest binaries only: this test must never compile or rebuild a package.
#![cfg(all(feature = "packages", target_os = "windows"))]

use morrow_core::{
    content::CardRecord,
    dispatch::HostRuntime,
    lifecycle::{GrantKind, InstancePhase},
    plugin_package::{catalog::Catalog, registry::Registry},
    store::Store,
    task::{Invocation, Transform},
};
use morrow_plugin_runtime::{
    Cancellation,
    dynamic_dependencies::{self, Context, RoutedOutput},
    manager::{ManagedInstance, Manager},
    proposal::{EditProposal, EditTarget},
    shared_objects::SharedObjects,
};
use std::path::Path;

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

#[path = "support/frozen_sdk.rs"]
mod frozen_sdk;

fn reopen(root: &Path) -> Result<Manager> {
    Ok(Manager::new(
        Registry::open(
            &root.join("registry"),
            Catalog::open(&root.join("catalog"))?,
        )?,
        Default::default(),
    ))
}

fn assert_released(objects: &mut SharedObjects) {
    let usage = objects.usage();
    assert_eq!(
        (
            usage.objects,
            usage.leases,
            usage.mappings,
            usage.charged_bytes
        ),
        (0, 0, 0, 0),
        "both success and rejection must release temporary shared input resources"
    );
}

fn run(
    manager: &Manager,
    host: &mut HostRuntime,
    objects: &mut SharedObjects,
    caller: &ManagedInstance,
    provider: &ManagedInstance,
    bytes: &[u8],
) -> Result<RoutedOutput> {
    let input = Invocation::new_transform(
        "frozen-dependency-task",
        Transform {
            handler: "bytes.dependency-wrap".into(),
            input_type: "bytes".into(),
            output_type: "bytes".into(),
            input: bytes.to_vec(),
        },
    )?;
    let result = dynamic_dependencies::run(
        manager,
        host,
        objects,
        caller,
        &[provider],
        &input,
        Context::new("frozen-workspace", 50),
        || 1,
        Cancellation::default(),
    );
    assert_released(objects);
    Ok(result?)
}

fn proposal(output: RoutedOutput, operation: &str, revision: u64) -> Result<EditProposal> {
    Ok(EditProposal::new(
        output,
        EditTarget {
            operation_id: operation,
            card_id: "card",
            expected_revision: revision,
            title: "frozen result",
            preview: "",
            accepted_output_type: "bytes",
        },
    )?)
}

fn exercise(language: &str) -> Result {
    let a = frozen_sdk::package(&format!("{language}-dependency"));
    let b = frozen_sdk::package("rust-provider");
    let caller_id = a.manifest().package_id.as_str();
    let provider_id = b.manifest().package_id.as_str();
    assert_eq!(
        caller_id,
        format!("org.morrow.compat.{language}.dependency")
    );
    assert_eq!(provider_id, "org.morrow.compat.rust.provider");
    let requirement = a.dependency("reverse")?;
    assert_eq!(requirement.handler, "bytes.tag-reverse");
    assert!(!requirement.optional);
    a.check_dependency("reverse", &b)?;

    let root = tempfile::tempdir()?;
    let catalog = Catalog::open(&root.path().join("catalog"))?;
    catalog.install(&a)?;
    catalog.install(&b)?;
    drop(catalog);
    let mut manager = reopen(root.path())?;
    for package in [&a, &b] {
        manager.select(package, manager.revision())?;
    }
    manager.approve(
        caller_id,
        a.digest(),
        [GrantKind::ReadContent, GrantKind::EditContent].into(),
        manager.revision(),
    )?;
    for package in [&a, &b] {
        manager.set_enabled(
            &package.manifest().package_id,
            package.digest(),
            true,
            manager.revision(),
        )?;
    }
    let mut host = HostRuntime::new(Store::open(
        &root.path().join("synthetic.db"),
        Default::default(),
    )?)?;
    assert!(
        manager.connect(caller_id, &mut host).is_err(),
        "required slot has no approval"
    );
    manager.approve_dependency(
        caller_id,
        a.digest(),
        "reverse",
        provider_id,
        b.digest(),
        manager.revision(),
    )?;
    let lock = manager.dependency(caller_id, "reverse").unwrap().clone();
    let revision = manager.revision();
    drop(manager);
    let mut manager = reopen(root.path())?;
    assert_eq!(manager.revision(), revision);
    assert_eq!(manager.dependency(caller_id, "reverse"), Some(&lock));
    let mut caller = manager.connect(caller_id, &mut host)?;
    let provider = manager.connect(provider_id, &mut host)?;
    let mut objects = SharedObjects::new(&host, Default::default())?;

    let ascii = run(
        &manager,
        &mut host,
        &mut objects,
        &caller,
        &provider,
        b"abc",
    )?;
    assert_eq!(ascii.bytes(), b"A[B:CBA]");
    assert_eq!(ascii.output_type(), "bytes");
    assert_eq!(ascii.dependency_calls(), 1);
    // The same original SDK binary handles arbitrary bytes rather than C strings/UTF-8.
    let binary = run(
        &manager,
        &mut host,
        &mut objects,
        &caller,
        &provider,
        b"a\0\xffz",
    )?;
    assert_eq!(binary.bytes(), b"A[B:Z\xff\0A]");
    assert_eq!(binary.dependency_calls(), 1);
    drop(binary);

    let mut seed = host.connect()?;
    host.grant(&mut seed, GrantKind::CreateContent, "card", 100, 1)?;
    host.create_content(
        &seed,
        "seed",
        &CardRecord::new("card", "bytes", 1, "original", b"original".to_vec())?,
        || 1,
    )?;
    host.disconnect(&seed)?;
    let committed = proposal(ascii, "frozen-edit", 1)?;
    assert!(
        committed
            .commit(&mut host, caller.connection(), || 1)
            .is_err(),
        "manifest approval is not a content grant"
    );
    assert_eq!(
        host.store_local().card("card")?.unwrap().body(),
        b"original"
    );
    host.grant(caller.parts_mut().1, GrantKind::EditContent, "card", 100, 1)?;
    let receipt = committed.commit(&mut host, caller.connection(), || 1)?;
    assert_eq!(receipt.revision, 2);
    assert_eq!(
        committed.commit(&mut host, caller.connection(), || 1)?,
        receipt
    );
    assert_eq!(host.store_local().pending(0, 10)?.len(), 2);

    let pending = proposal(
        run(
            &manager,
            &mut host,
            &mut objects,
            &caller,
            &provider,
            b"next",
        )?,
        "revoked-edit",
        2,
    )?;
    manager.set_enabled(provider_id, b.digest(), false, manager.revision())?;
    assert_eq!(
        host.connection_phase(caller.connection())?,
        InstancePhase::Revoked
    );
    assert_eq!(
        host.connection_phase(provider.connection())?,
        InstancePhase::Revoked
    );
    assert!(
        pending
            .commit(&mut host, caller.connection(), || 1)
            .is_err()
    );
    assert!(
        committed
            .commit(&mut host, caller.connection(), || 1)
            .is_err(),
        "receipt retry must still check live authorization"
    );
    assert!(
        run(
            &manager,
            &mut host,
            &mut objects,
            &caller,
            &provider,
            b"abc"
        )
        .is_err()
    );
    assert!(manager.connect(caller_id, &mut host).is_err());
    assert_eq!(manager.dependency(caller_id, "reverse"), Some(&lock));
    assert_eq!(host.store_local().pending(0, 10)?.len(), 2);
    assert_eq!(
        host.store_local().card("card")?.unwrap().body(),
        b"A[B:CBA]"
    );

    manager.set_enabled(provider_id, b.digest(), true, manager.revision())?;
    let mut fresh_caller = manager.connect(caller_id, &mut host)?;
    let fresh_provider = manager.connect(provider_id, &mut host)?;
    assert!(
        pending
            .commit(&mut host, fresh_caller.connection(), || 1)
            .is_err(),
        "old result cannot bind a new instance"
    );
    let fresh = proposal(
        run(
            &manager,
            &mut host,
            &mut objects,
            &fresh_caller,
            &fresh_provider,
            b"next",
        )?,
        "fresh-edit",
        2,
    )?;
    assert!(
        fresh
            .commit(&mut host, fresh_caller.connection(), || 1)
            .is_err(),
        "new instance does not inherit old content grant"
    );
    host.grant(
        fresh_caller.parts_mut().1,
        GrantKind::EditContent,
        "card",
        100,
        1,
    )?;
    assert_eq!(
        fresh
            .commit(&mut host, fresh_caller.connection(), || 1)?
            .revision,
        3
    );
    assert_eq!(
        host.store_local().card("card")?.unwrap().body(),
        b"A[B:TXEN]"
    );
    assert_eq!(host.store_local().pending(0, 10)?.len(), 3);
    for instance in [&caller, &provider, &fresh_caller, &fresh_provider] {
        instance.close(&mut host)?;
    }
    assert_released(&mut objects);
    host.store_local().integrity_check()?;
    Ok(())
}

#[test]
fn frozen_rust_dependency_binary_preserves_routing_and_authorization() -> Result {
    exercise("rust")
}
#[test]
fn frozen_c_dependency_binary_preserves_routing_and_authorization() -> Result {
    exercise("c")
}
#[test]
fn frozen_cpp_dependency_binary_preserves_routing_and_authorization() -> Result {
    exercise("cpp")
}
