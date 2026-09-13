//! Real Rust graph under automatic managed-provider pooling and explicit session recovery.
use morrow_core::{
    content::CardRecord,
    dispatch::HostRuntime,
    lifecycle::{GrantKind, InstancePhase},
    plugin_package::{
        DEPENDENCIES_FEATURE, DEPENDENCY_CALLS_FEATURE, Package,
        catalog::Catalog,
        proto::{Capability, DependencyRequirement, TransformHandler},
        registry::Registry,
    },
    store::Store,
    task::{Invocation, Transform},
};
use morrow_plugin_runtime::{
    Cancellation,
    dynamic_dependencies::{Context, GraphLimits, RoutedOutput},
    instance_pool::{Limits, Pool},
    manager::Manager,
    proposal::{EditProposal, EditTarget},
    shared_objects::SharedObjects,
};
const ROOT: &str = "org.test.graph.root";
const MIDDLE: &str = "org.test.graph.middle";
const LEAF: &str = "org.test.graph.leaf";
const EXPECTED: &[u8] = b"A[M[B:CBA]]";
fn package(
    module: &[u8],
    id: &str,
    version: &str,
    handler: &str,
    dependency: Option<(&str, &str)>,
) -> Package {
    let mut manifest = Package::manifest_for_transform(
        id,
        version,
        module,
        vec![TransformHandler {
            handler: handler.into(),
            input_type: "bytes".into(),
            output_type: "bytes".into(),
            // Leaves room for all wrappers even at the caller's maximum declared input size.
            max_input_bytes: 65528,
            max_output_bytes: 65536,
        }],
    );
    if id == ROOT {
        manifest.requested_capabilities = vec![
            Capability::ReadContent as i32,
            Capability::EditContent as i32,
        ];
    }
    if let Some((slot, handler)) = dependency {
        manifest
            .required_features
            .extend([DEPENDENCIES_FEATURE.into(), DEPENDENCY_CALLS_FEATURE.into()]);
        manifest.dependency_schema_sha256 = morrow_core::dependency_call::schema_digest().to_vec();
        manifest.dependencies.push(DependencyRequirement {
            slot: slot.into(),
            handler: handler.into(),
            input_type: "bytes".into(),
            output_type: "bytes".into(),
            provider_version: "^1.0.0".into(),
            optional: false,
        });
    }
    Package::build(manifest, module).unwrap()
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
        "all real shared-object ownership must be released"
    );
}
fn proposal(output: RoutedOutput, operation: &str, revision: u64) -> EditProposal {
    EditProposal::new(
        output,
        EditTarget {
            operation_id: operation,
            card_id: "card",
            expected_revision: revision,
            title: "Nested transform",
            preview: "A[M[B:CBA]]",
            accepted_output_type: "bytes",
        },
    )
    .unwrap()
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let paths: Vec<_> = std::env::args().skip(1).collect();
    assert_eq!(
        paths.len(),
        3,
        "expected actual Rust root, middle and leaf Wasm paths"
    );
    let modules: Vec<_> = paths.iter().map(std::fs::read).collect::<Result<_, _>>()?;
    for first in 0..modules.len() {
        for second in first + 1..modules.len() {
            assert_ne!(
                modules[first], modules[second],
                "three distinct actual modules required"
            );
        }
    }
    let a = package(
        &modules[0],
        ROOT,
        "1.0.0",
        "bytes.dependency-wrap",
        Some(("reverse", "bytes.graph-middle")),
    );
    let middle = package(
        &modules[1],
        MIDDLE,
        "1.0.0",
        "bytes.graph-middle",
        Some(("leaf", "bytes.tag-reverse")),
    );
    let leaf = package(&modules[2], LEAF, "1.0.0", "bytes.tag-reverse", None);
    let directory = tempfile::tempdir()?;
    let catalog = Catalog::open(&directory.path().join("packages"))?;
    for package in [&a, &middle, &leaf] {
        catalog.install(package)?;
    }
    let mut manager = reopen(directory.path());
    for package in [&a, &middle, &leaf] {
        manager.select(package, manager.revision())?;
        manager.set_enabled(
            &package.manifest().package_id,
            package.digest(),
            true,
            manager.revision(),
        )?;
    }
    manager.approve(
        ROOT,
        a.digest(),
        [GrantKind::ReadContent, GrantKind::EditContent].into(),
        manager.revision(),
    )?;
    manager.approve_dependency(
        ROOT,
        a.digest(),
        "reverse",
        MIDDLE,
        middle.digest(),
        manager.revision(),
    )?;
    manager.approve_dependency(
        MIDDLE,
        middle.digest(),
        "leaf",
        LEAF,
        leaf.digest(),
        manager.revision(),
    )?;
    let root_lock = manager.dependency(ROOT, "reverse").unwrap().clone();
    let leaf_lock = manager.dependency(MIDDLE, "leaf").unwrap().clone();
    let revision = manager.revision();
    drop(manager);
    let mut manager = reopen(directory.path());
    assert_eq!(manager.revision(), revision);
    assert_eq!(manager.dependency(ROOT, "reverse"), Some(&root_lock));
    assert_eq!(manager.dependency(MIDDLE, "leaf"), Some(&leaf_lock));
    let path = directory.path().join("synthetic.db");
    let mut host = HostRuntime::new(Store::open(&path, Default::default())?)?;
    let mut seed = host.connect()?;
    host.grant(&mut seed, GrantKind::CreateContent, "card", 100, 1)?;
    host.create_content(
        &seed,
        "seed",
        &CardRecord::new("card", "bytes", 1, "original", b"abc".to_vec())?,
        || 1,
    )?;
    host.disconnect(&seed)?;
    let mut pool = Pool::new(&host, Limits::default())?;
    // The pool resolves and starts the mandatory provider closure. No manual provider connect.
    let first = pool.start(&mut manager, &mut host, ROOT, &[], revision)?;
    let second = pool.start(&mut manager, &mut host, ROOT, &[], revision)?;
    let first_binding = pool.root(&first)?.connection().binding();
    let second_binding = pool.root(&second)?.connection().binding();
    assert_ne!(
        first_binding, second_binding,
        "each session owns a distinct root"
    );
    let middle_binding = pool
        .provider(MIDDLE)
        .ok_or("middle was not automatically started")?
        .connection()
        .binding();
    let leaf_binding = pool
        .provider(LEAF)
        .ok_or("leaf was not automatically started")?
        .connection()
        .binding();
    assert_eq!((pool.usage().sessions, pool.usage().providers), (2, 2));
    for kind in [GrantKind::ReadContent, GrantKind::EditContent] {
        pool.grant_root(&mut host, &second, kind, "card", 100, 1)?;
    }
    let card = host.read_content(pool.root(&second)?.connection(), "card", || 1)?;
    let input = Invocation::new_transform(
        "pool-root",
        Transform {
            handler: "bytes.dependency-wrap".into(),
            input_type: "bytes".into(),
            output_type: "bytes".into(),
            input: card.body(),
        },
    )?;
    pool.close(&mut host, &first)?;
    assert!(pool.root(&first).is_err());
    assert_eq!((pool.usage().sessions, pool.usage().providers), (1, 2));
    assert_eq!(
        pool.provider(MIDDLE).unwrap().connection().binding(),
        middle_binding
    );
    assert_eq!(
        pool.provider(LEAF).unwrap().connection().binding(),
        leaf_binding
    );
    for id in [MIDDLE, LEAF] {
        assert_eq!(
            host.connection_phase(pool.provider(id).unwrap().connection())?,
            InstancePhase::Ready
        );
    }
    let mut objects = SharedObjects::new(&host, Default::default())?;
    let output = pool.run(
        &manager,
        &mut host,
        &mut objects,
        &second,
        &input,
        Context::new("workspace:pool", 50),
        GraphLimits::default(),
        || 1,
        Cancellation::default(),
    )?;
    assert_eq!(output.bytes(), EXPECTED);
    assert_eq!(output.dependency_calls(), 2);
    assert_released(&mut objects);
    assert_eq!(host.store_local().card("card")?.unwrap().body(), b"abc");
    let committed = proposal(output, "pool-edit", 1);
    let receipt = committed.commit(&mut host, pool.root(&second)?.connection(), || 1)?;
    assert_eq!(receipt.revision, 2);
    assert_eq!(
        committed.commit(&mut host, pool.root(&second)?.connection(), || 1)?,
        receipt
    );
    let output = pool.run(
        &manager,
        &mut host,
        &mut objects,
        &second,
        &input,
        Context::new("workspace:pool", 50),
        GraphLimits::default(),
        || 1,
        Cancellation::default(),
    )?;
    assert_eq!(output.bytes(), EXPECTED);
    assert_eq!(output.dependency_calls(), 2);
    let pending = proposal(output, "pool-pending", 2);
    assert_released(&mut objects);
    // This is an explicit managed-instance stop, not an operating-system process crash.
    {
        let original_root = pool.root(&second)?;
        pool.provider(LEAF).ok_or("missing leaf")?.stop();
        assert!(
            pending
                .commit(&mut host, original_root.connection(), || 2)
                .is_err(),
            "actual leaf revocation invalidates the retained result before maintenance"
        );
    }
    pool.maintain(&manager, &mut host)?;
    assert!(
        pool.root(&second).is_err(),
        "maintenance invalidates the affected root session"
    );
    assert!(
        pool.run(
            &manager,
            &mut host,
            &mut objects,
            &second,
            &input,
            Context::new("workspace:pool", 50),
            GraphLimits::default(),
            || 2,
            Cancellation::default()
        )
        .is_err()
    );
    assert_eq!(host.store_local().pending(0, 10)?.len(), 2);
    let fresh = pool.restart(&mut manager, &mut host, &second, revision, 3)?;
    assert_ne!(pool.root(&fresh)?.connection().binding(), second_binding);
    assert_ne!(
        pool.provider(MIDDLE)
            .ok_or("recovery did not start middle")?
            .connection()
            .binding(),
        middle_binding
    );
    assert_ne!(
        pool.provider(LEAF)
            .ok_or("recovery did not start leaf")?
            .connection()
            .binding(),
        leaf_binding
    );
    assert!(pool.root(&second).is_err());
    assert!(
        pool.run(
            &manager,
            &mut host,
            &mut objects,
            &second,
            &input,
            Context::new("workspace:pool", 50),
            GraphLimits::default(),
            || 3,
            Cancellation::default()
        )
        .is_err()
    );
    assert!(
        host.read_content(pool.root(&fresh)?.connection(), "card", || 3)
            .is_err(),
        "new instance must receive fresh object grants"
    );
    assert!(
        pending
            .commit(&mut host, pool.root(&fresh)?.connection(), || 3)
            .is_err()
    );
    assert!(
        committed
            .commit(&mut host, pool.root(&fresh)?.connection(), || 3)
            .is_err()
    );
    assert_eq!(
        host.store_local().pending(0, 10)?.len(),
        2,
        "recovery cannot automatically commit a retained intent"
    );
    // Even a newly computed result does not confer EditContent authority on the fresh root.
    let fresh_output = pool.run(
        &manager,
        &mut host,
        &mut objects,
        &fresh,
        &input,
        Context::new("workspace:pool", 50),
        GraphLimits::default(),
        || 3,
        Cancellation::default(),
    )?;
    assert_eq!(fresh_output.bytes(), EXPECTED);
    assert_eq!(fresh_output.dependency_calls(), 2);
    let fresh_proposal = proposal(fresh_output, "pool-recovered-edit", 2);
    assert!(
        fresh_proposal
            .commit(&mut host, pool.root(&fresh)?.connection(), || 3)
            .is_err()
    );
    for kind in [GrantKind::ReadContent, GrantKind::EditContent] {
        pool.grant_root(&mut host, &fresh, kind, "card", 100, 3)?;
    }
    assert_eq!(
        host.read_content(pool.root(&fresh)?.connection(), "card", || 3)?
            .body(),
        EXPECTED
    );
    let recovered_receipt =
        fresh_proposal.commit(&mut host, pool.root(&fresh)?.connection(), || 3)?;
    assert_eq!(recovered_receipt.revision, 3);
    assert_eq!(
        fresh_proposal.commit(&mut host, pool.root(&fresh)?.connection(), || 3)?,
        recovered_receipt
    );
    assert_eq!(
        host.store_local().pending(0, 10)?.len(),
        3,
        "only seed and two explicit edits"
    );
    assert_eq!(
        manager.revision(),
        revision,
        "instance recovery needs no new package or registry mutation"
    );
    assert_eq!(manager.dependency(ROOT, "reverse"), Some(&root_lock));
    assert_eq!(manager.dependency(MIDDLE, "leaf"), Some(&leaf_lock));
    pool.close(&mut host, &fresh)?;
    assert_eq!((pool.usage().sessions, pool.usage().providers), (0, 0));
    assert_released(&mut objects);
    host.store_local().integrity_check()?;
    drop(host);
    let store = Store::open_existing(&path, Default::default())?;
    assert_eq!(store.card("card")?.unwrap().body(), EXPECTED);
    store.integrity_check()?;
    println!(
        "PASS_SCOPED: actual Rust A/M/B graph automatically started from persisted approved required locks; two isolated roots shared two providers; closing one session preserved the other graph and its commit; explicit leaf instance stop invalidated the retained result and old session; explicit restart created fresh bindings without package changes; old proposals rejected and fresh content grants required; only explicit edits persisted; all pool and shared resources released. This is managed-instance stop/recovery, not OS process crash recovery or automatic task replay; default UI and full audit/replay remain pending."
    );
    Ok(())
}
