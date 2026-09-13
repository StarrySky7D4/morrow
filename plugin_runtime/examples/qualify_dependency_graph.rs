//! Real Rust A -> middle M -> leaf B imports, followed by one guarded content commit.
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
    dynamic_dependencies::{self, Context, GraphLimits, RoutedOutput},
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
        "expected actual Rust caller, middle and leaf Wasm paths"
    );
    let modules: Vec<_> = paths.iter().map(std::fs::read).collect::<Result<_, _>>()?;
    for first in 0..modules.len() {
        for second in first + 1..modules.len() {
            assert_ne!(
                modules[first], modules[second],
                "qualification requires three distinct compiled modules"
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
    let upgrade = package(&modules[2], LEAF, "1.1.0", "bytes.tag-reverse", None);
    let root = tempfile::tempdir()?;
    let catalog = Catalog::open(&root.path().join("packages"))?;
    for package in [&a, &middle, &leaf, &upgrade] {
        catalog.install(package)?;
    }
    let mut manager = reopen(root.path());
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
    let store_path = root.path().join("synthetic.db");
    let mut host = HostRuntime::new(Store::open(&store_path, Default::default())?)?;
    assert!(manager.connect(ROOT, &mut host).is_err());
    manager.approve_dependency(
        ROOT,
        a.digest(),
        "reverse",
        MIDDLE,
        middle.digest(),
        manager.revision(),
    )?;
    assert!(
        manager.connect(ROOT, &mut host).is_err(),
        "root requires middle's mandatory leaf approval too"
    );
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
    let mut manager = reopen(root.path());
    assert_eq!(manager.revision(), revision);
    assert_eq!(manager.dependency(ROOT, "reverse"), Some(&root_lock));
    assert_eq!(manager.dependency(MIDDLE, "leaf"), Some(&leaf_lock));
    let mut caller = manager.connect(ROOT, &mut host)?;
    let middle_instance = manager.connect(MIDDLE, &mut host)?;
    let leaf_instance = manager.connect(LEAF, &mut host)?;
    assert!(
        middle_instance
            .package()
            .package()
            .capabilities()
            .is_empty()
    );
    assert!(leaf_instance.package().package().capabilities().is_empty());
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
    let input = Invocation::new_transform(
        "graph-root",
        Transform {
            handler: "bytes.dependency-wrap".into(),
            input_type: "bytes".into(),
            output_type: "bytes".into(),
            input: card.body(),
        },
    )?;
    let mut objects = SharedObjects::new(&host, Default::default())?;
    let output = dynamic_dependencies::run_graph(
        &manager,
        &mut host,
        &mut objects,
        &caller,
        &[&middle_instance, &leaf_instance],
        &input,
        Context::new("workspace:graph", 50),
        GraphLimits::default(),
        || 1,
        Cancellation::default(),
    )?;
    assert_eq!(output.bytes(), EXPECTED);
    assert_eq!(
        output.dependency_calls(),
        2,
        "count both actual imports, including middle -> leaf"
    );
    assert_released(&mut objects);
    assert_eq!(
        host.read_content(caller.connection(), "card", || 1)?.body(),
        b"abc",
        "completing the guest graph must not commit content"
    );
    let committed = proposal(output, "graph-edit", 1);
    let receipt = committed.commit(&mut host, caller.connection(), || 1)?;
    assert_eq!(receipt.revision, 2);
    assert_eq!(
        committed.commit(&mut host, caller.connection(), || 1)?,
        receipt
    );
    // Retain an uncommitted intent as well as the committed receipt; leaf upgrade must deny both.
    let pending_output = dynamic_dependencies::run_graph(
        &manager,
        &mut host,
        &mut objects,
        &caller,
        &[&middle_instance, &leaf_instance],
        &input,
        Context::new("workspace:graph", 50),
        GraphLimits::default(),
        || 1,
        Cancellation::default(),
    )?;
    assert_eq!(pending_output.bytes(), EXPECTED);
    assert_eq!(pending_output.dependency_calls(), 2);
    assert_released(&mut objects);
    let pending = proposal(pending_output, "graph-after-upgrade", 2);
    manager.select(&upgrade, manager.revision())?;
    for instance in [&caller, &middle_instance, &leaf_instance] {
        assert_eq!(
            host.connection_phase(instance.connection())?,
            InstancePhase::Revoked
        );
    }
    assert_eq!(manager.dependency(ROOT, "reverse"), Some(&root_lock));
    assert!(manager.dependency(MIDDLE, "leaf").is_none());
    assert!(
        committed
            .commit(&mut host, caller.connection(), || 1)
            .is_err()
    );
    assert!(
        pending
            .commit(&mut host, caller.connection(), || 1)
            .is_err()
    );
    assert!(manager.connect(ROOT, &mut host).is_err());
    assert_eq!(host.store_local().card("card")?.unwrap().body(), EXPECTED);
    assert_eq!(
        host.store_local().pending(0, 10)?.len(),
        2,
        "seed and one edit only"
    );
    caller.close(&mut host)?;
    middle_instance.close(&mut host)?;
    leaf_instance.close(&mut host)?;
    assert_released(&mut objects);
    host.store_local().integrity_check()?;
    drop(host);
    let store = Store::open_existing(&store_path, Default::default())?;
    assert_eq!(store.card("card")?.unwrap().body(), EXPECTED);
    store.integrity_check()?;
    println!(
        "PASS_SCOPED: three distinct Wasm guests performed A -> M -> B imports and returned A[M[B:CBA]]; two calls per graph; persisted explicit locks reopened; all temporary shared resources released; one guarded edit with identical retry receipt; leaf upgrade revoked root/middle/leaf and denied retained committed and uncommitted proposals. Windows local qualification only; automatic startup, default UI and full dependency audit/replay remain pending."
    );
    Ok(())
}
