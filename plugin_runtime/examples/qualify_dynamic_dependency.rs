//! Actual Wasm caller imports the dependency SDK and uses the correlated Rust provider result.
use morrow_core::{
    content::CardRecord,
    dispatch::HostRuntime,
    lifecycle::{GrantKind, InstancePhase},
    plugin_package::{
        DEPENDENCIES_FEATURE, Package,
        catalog::Catalog,
        proto::{Capability, DependencyRequirement, TransformHandler},
        registry::Registry,
    },
    store::Store,
};
use morrow_plugin_runtime::{
    Cancellation,
    dynamic_dependencies::{self, Context},
    manager::Manager,
    proposal::{EditProposal, EditTarget},
    shared_objects::SharedObjects,
};
fn package(path: &str, caller: bool, version: &str) -> Package {
    let wasm = std::fs::read(path).unwrap();
    let handler = if caller {
        "bytes.dependency-wrap"
    } else {
        "bytes.tag-reverse"
    };
    let mut manifest = Package::manifest_for_transform(
        if caller {
            "org.test.caller"
        } else {
            "org.test.provider"
        },
        version,
        &wasm,
        vec![TransformHandler {
            handler: handler.into(),
            input_type: "bytes".into(),
            output_type: "bytes".into(),
            max_input_bytes: 65531,
            max_output_bytes: 65536,
        }],
    );
    if caller {
        manifest.requested_capabilities = vec![
            Capability::ReadContent as i32,
            Capability::EditContent as i32,
        ];
        manifest.required_features.push(DEPENDENCIES_FEATURE.into());
        manifest
            .required_features
            .push(morrow_core::plugin_package::DEPENDENCY_CALLS_FEATURE.into());
        manifest.dependency_schema_sha256 = morrow_core::dependency_call::schema_digest().to_vec();
        manifest.dependencies.push(DependencyRequirement {
            slot: "reverse".into(),
            handler: "bytes.tag-reverse".into(),
            input_type: "bytes".into(),
            output_type: "bytes".into(),
            provider_version: "^1.0.0".into(),
            optional: false,
        });
    }
    Package::build(manifest, &wasm).unwrap()
}
fn reopen(root: &std::path::Path) -> Manager {
    Manager::new(
        Registry::open(
            &root.join("registry"),
            Catalog::open(&root.join("packages")).unwrap(),
        )
        .unwrap(),
        Default::default(),
    )
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    assert_eq!(args.len(), 2, "caller and provider Rust Wasm required");
    assert_ne!(std::fs::read(&args[0])?, std::fs::read(&args[1])?);
    let a = package(&args[0], true, "1.0.0");
    let b = package(&args[1], false, "1.0.0");
    let upgrade = package(&args[1], false, "1.1.0");
    let root = tempfile::tempdir()?;
    let catalog = Catalog::open(&root.path().join("packages"))?;
    for p in [&a, &b, &upgrade] {
        catalog.install(p)?;
    }
    let mut manager = reopen(root.path());
    for p in [&a, &b] {
        manager.select(p, manager.revision())?;
    }
    manager.approve(
        "org.test.caller",
        a.digest(),
        [GrantKind::ReadContent, GrantKind::EditContent].into(),
        manager.revision(),
    )?;
    for p in [&a, &b] {
        manager.set_enabled(
            &p.manifest().package_id,
            p.digest(),
            true,
            manager.revision(),
        )?;
    }
    let path = root.path().join("synthetic.db");
    let mut host = HostRuntime::new(Store::open(&path, Default::default())?)?;
    assert!(
        manager.connect("org.test.caller", &mut host).is_err(),
        "mandatory dependency was never approved"
    );
    manager.approve_dependency(
        "org.test.caller",
        a.digest(),
        "reverse",
        "org.test.provider",
        b.digest(),
        manager.revision(),
    )?;
    let locked = manager
        .dependency("org.test.caller", "reverse")
        .unwrap()
        .clone();
    let revision = manager.revision();
    drop(manager);
    let mut manager = reopen(root.path());
    assert_eq!(manager.revision(), revision);
    assert_eq!(
        manager.dependency("org.test.caller", "reverse"),
        Some(&locked)
    );
    let mut caller = manager.connect("org.test.caller", &mut host)?;
    let provider = manager.connect("org.test.provider", &mut host)?;
    let mut seed = host.connect()?;
    host.grant(&mut seed, GrantKind::CreateContent, "card", 100, 1)?;
    host.create_content(
        &seed,
        "seed",
        &CardRecord::new("card", "bytes", 1, "original", b"abc".to_vec())?,
        || 1,
    )?;
    host.disconnect(&seed)?;
    for kind in [GrantKind::ReadContent, GrantKind::EditContent] {
        host.grant(caller.parts_mut().1, kind, "card", 100, 1)?;
    }
    let card = host.read_content(caller.connection(), "card", || 1)?;
    let mut objects = SharedObjects::new(&host, Default::default())?;
    let input = morrow_core::task::Invocation::new_transform(
        "dynamic-a",
        morrow_core::task::Transform {
            handler: "bytes.dependency-wrap".into(),
            input_type: "bytes".into(),
            output_type: "bytes".into(),
            input: card.body(),
        },
    )?;
    let output = dynamic_dependencies::run(
        &manager,
        &mut host,
        &mut objects,
        &caller,
        &[&provider],
        &input,
        Context::new("workspace:a", 50),
        || 1,
        Cancellation::default(),
    )?;
    assert_eq!(output.bytes(), b"A[B:CBA]");
    assert_eq!(output.dependency_calls(), 1);
    let usage = objects.usage();
    assert_eq!(
        (
            usage.objects,
            usage.leases,
            usage.mappings,
            usage.charged_bytes
        ),
        (0, 0, 0, 0)
    );
    let proposal = EditProposal::new(
        output,
        EditTarget {
            operation_id: "locked-edit",
            card_id: "card",
            expected_revision: 1,
            title: "transformed",
            preview: "A[B:CBA]",
            accepted_output_type: "bytes",
        },
    )?;
    let receipt = proposal.commit(&mut host, caller.connection(), || 1)?;
    assert_eq!(receipt.revision, 2);
    assert_eq!(
        proposal.commit(&mut host, caller.connection(), || 1)?,
        receipt
    );
    manager.select(&upgrade, manager.revision())?;
    assert_eq!(
        host.connection_phase(caller.connection())?,
        InstancePhase::Revoked
    );
    assert_eq!(
        host.connection_phase(provider.connection())?,
        InstancePhase::Revoked
    );
    assert!(manager.dependency("org.test.caller", "reverse").is_none());
    assert!(
        proposal
            .commit(&mut host, caller.connection(), || 1)
            .is_err()
    );
    assert!(manager.connect("org.test.caller", &mut host).is_err());
    manager.set_enabled(
        "org.test.provider",
        upgrade.digest(),
        true,
        manager.revision(),
    )?;
    assert!(
        manager.connect("org.test.caller", &mut host).is_err(),
        "upgrade cannot silently reapprove a dependency"
    );
    manager.approve_dependency(
        "org.test.caller",
        a.digest(),
        "reverse",
        "org.test.provider",
        upgrade.digest(),
        manager.revision(),
    )?;
    let fresh_caller = manager.connect("org.test.caller", &mut host)?;
    let fresh_provider = manager.connect("org.test.provider", &mut host)?;
    let _fresh_route = manager.bind_locked_dependency(
        &host,
        &fresh_caller,
        &fresh_provider,
        "reverse",
        "workspace:a",
        50,
        1,
    )?;
    assert!(
        manager
            .bind_locked_dependency(
                &host,
                &caller,
                &fresh_provider,
                "reverse",
                "workspace:a",
                50,
                1
            )
            .is_err()
    );
    assert_eq!(
        host.store_local().card("card")?.unwrap().body(),
        b"A[B:CBA]"
    );
    assert_eq!(host.store_local().pending(0, 10)?.len(), 2);
    caller.close(&mut host)?;
    provider.close(&mut host)?;
    fresh_caller.close(&mut host)?;
    fresh_provider.close(&mut host)?;
    assert_eq!(objects.usage().charged_bytes, 0);
    host.store_local().integrity_check()?;
    println!(
        "PASS_SCOPED: actual Wasm caller issued its own dependency request and verified provider response; host-resolved reopened lock; caller wrapped provider output as A[B:CBA]; temporary objects/leases/maps returned to zero; core edit committed once; provider upgrade invalidated retained proposal. Automatic startup, nested calls, UI and full audit/replay remain pending."
    );
    Ok(())
}
