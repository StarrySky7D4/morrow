#![cfg(all(feature = "packages", not(target_arch = "wasm32")))]
use morrow_core::{
    Error,
    dispatch::HostRuntime,
    lifecycle::InstancePhase,
    plugin_package::{
        Package,
        catalog::Catalog,
        proto::{DependencyRequirement, TransformHandler},
        registry::Registry,
    },
    store::Store,
};
use morrow_plugin_runtime::manager::{ActivationPlan, Manager, ManagerError, OptionalDependency};
use std::{collections::BTreeSet, fs};
fn package(id: &str, version: &str, dependencies: &[(&str, bool)]) -> Package {
    let module = wat::parse_str(r#"(module (memory (export "memory") 1) (func (export "morrow_run") (result i32) i32.const 0))"#).unwrap();
    let mut manifest = Package::manifest_for_transform(
        id,
        version,
        &module,
        vec![TransformHandler {
            handler: "transform".into(),
            input_type: "bytes".into(),
            output_type: "bytes".into(),
            max_input_bytes: 1024,
            max_output_bytes: 1024,
        }],
    );
    if !dependencies.is_empty() {
        manifest.required_features.push("dependencies-v1".into());
        manifest.dependencies = dependencies
            .iter()
            .map(|(slot, optional)| DependencyRequirement {
                slot: (*slot).into(),
                handler: "transform".into(),
                input_type: "bytes".into(),
                output_type: "bytes".into(),
                provider_version: "^1.0".into(),
                optional: *optional,
            })
            .collect();
    }
    Package::build(manifest, &module).unwrap()
}
struct Fixture {
    root: tempfile::TempDir,
    manager: Manager,
}
impl Fixture {
    fn new(packages: &[Package]) -> Self {
        let root = tempfile::tempdir().unwrap();
        let catalog = Catalog::open(&root.path().join("packages")).unwrap();
        for p in packages {
            catalog.install(p).unwrap();
        }
        let mut manager = Manager::new(
            Registry::open(&root.path().join("registry"), catalog).unwrap(),
            Default::default(),
        );
        for p in packages {
            manager.select(p, manager.revision()).unwrap();
            manager
                .set_enabled(
                    &p.manifest().package_id,
                    p.digest(),
                    true,
                    manager.revision(),
                )
                .unwrap();
        }
        Self { root, manager }
    }
    fn lock(&mut self, caller: &str, slot: &str, provider: &str) {
        let a = self.manager.selection(caller).unwrap().digest;
        let b = self.manager.selection(provider).unwrap().digest;
        self.manager
            .approve_dependency(caller, a, slot, provider, b, self.manager.revision())
            .unwrap();
    }
    fn plan(
        &self,
        root: &str,
        optional: &[OptionalDependency],
    ) -> Result<ActivationPlan, ManagerError> {
        self.manager
            .activation_plan(root, optional, self.manager.revision())
    }
    fn persisted(&self) -> Vec<u8> {
        fs::read(self.root.path().join("registry/selection.morrow")).unwrap()
    }
}
fn optional(caller: &str, slot: &str) -> OptionalDependency {
    OptionalDependency {
        caller: caller.into(),
        slot: slot.into(),
    }
}
fn ids(plan: &ActivationPlan) -> Vec<&str> {
    plan.packages().iter().map(|p| p.id.as_str()).collect()
}
fn set(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|s| (*s).into()).collect()
}

#[test]
fn required_diamond_is_deduplicated_provider_first_and_plan_is_read_only() {
    let packages = [
        package("a", "1.0.0", &[("b", false), ("c", false)]),
        package("b", "1.0.0", &[("d", false)]),
        package("c", "1.0.0", &[("d", false)]),
        package("d", "1.0.0", &[]),
    ];
    let mut f = Fixture::new(&packages);
    for (caller, slot, provider) in [
        ("a", "b", "b"),
        ("a", "c", "c"),
        ("b", "d", "d"),
        ("c", "d", "d"),
    ] {
        f.lock(caller, slot, provider);
    }
    let before = f.persisted();
    let revision = f.manager.revision();
    let mut host =
        HostRuntime::new(Store::open(&f.root.path().join("store.db"), Default::default()).unwrap())
            .unwrap();
    let instance = f.manager.connect("a", &mut host).unwrap();
    let plan = f.plan("a", &[]).unwrap();
    assert_eq!(ids(&plan), vec!["d", "b", "c", "a"]);
    assert_eq!(plan.required(), &set(&["a", "b", "c", "d"]));
    assert_eq!(
        plan.required_edges(),
        &[
            ("a".into(), "b".into()),
            ("a".into(), "c".into()),
            ("b".into(), "d".into()),
            ("c".into(), "d".into())
        ]
    );
    assert_eq!(plan.root(), "a");
    assert_eq!(plan.revision(), revision);
    assert!(plan.optionals().is_empty());
    for planned in plan.packages() {
        assert_eq!(
            planned.digest,
            f.manager.selection(&planned.id).unwrap().digest
        );
    }
    assert_eq!(f.persisted(), before);
    assert_eq!(f.manager.revision(), revision);
    assert_eq!(
        host.connection_phase(instance.connection()),
        Ok(InstancePhase::Ready)
    );
    let cloned = plan.clone();
    assert_eq!(ids(&cloned), ids(&plan));
    instance.close(&mut host).unwrap();
}

#[test]
fn plain_task_root_is_supported_and_legacy_abi_is_rejected() {
    let module = b"\0asm\x01\0\0\0";
    let plain = Package::build(
        Package::manifest_for_task("plain", "1.0.0", module, vec![]),
        module,
    )
    .unwrap();
    let legacy = Package::build(
        Package::manifest_for("legacy", "1.0.0", module, vec![]),
        module,
    )
    .unwrap();
    let f = Fixture::new(&[plain, legacy]);
    let plan = f.plan("plain", &[]).unwrap();
    assert_eq!(ids(&plan), vec!["plain"]);
    assert_eq!(plan.required(), &set(&["plain"]));
    assert!(matches!(
        f.plan("legacy", &[]),
        Err(ManagerError::Core(Error::UnsupportedVersion))
    ));
}

#[test]
fn missing_lock_or_disabled_required_provider_cannot_produce_a_plan() {
    let mut f = Fixture::new(&[
        package("a", "1.0.0", &[("b", false)]),
        package("b", "1.0.0", &[]),
    ]);
    let before = f.persisted();
    assert!(f.plan("a", &[]).is_err());
    assert_eq!(f.persisted(), before);
    f.lock("a", "b", "b");
    let b = f.manager.selection("b").unwrap().digest;
    f.manager
        .set_enabled("b", b, false, f.manager.revision())
        .unwrap();
    let before = f.persisted();
    assert!(f.plan("a", &[]).is_err());
    assert_eq!(f.persisted(), before);
}

#[test]
fn optional_requests_expand_only_reachable_approved_slots_and_their_required_subtrees() {
    let mut f = Fixture::new(&[
        package("a", "1.0.0", &[("b", true), ("ignored", true)]),
        package("b", "1.0.0", &[("c", false), ("d", true)]),
        package("c", "1.0.0", &[]),
        package("d", "1.0.0", &[]),
    ]);
    f.lock("a", "b", "b");
    f.lock("b", "c", "c");
    f.lock("b", "d", "d");
    assert_eq!(ids(&f.plan("a", &[]).unwrap()), vec!["a"]);
    let requests = [optional("b", "d"), optional("a", "b")];
    let plan = f.plan("a", &requests).unwrap();
    assert_eq!(
        ids(&plan).into_iter().collect::<BTreeSet<_>>(),
        BTreeSet::from(["a", "b", "c", "d"])
    );
    assert_eq!(plan.required(), &set(&["a"]));
    assert_eq!(plan.required_edges(), &[("b".into(), "c".into())]);
    assert!(
        ids(&plan).iter().position(|id| *id == "c") < ids(&plan).iter().position(|id| *id == "b")
    );
    assert_eq!(plan.optionals(), requests);
    assert!(f.plan("a", &[optional("a", "ignored")]).is_err());
    let b = f.manager.selection("b").unwrap().digest;
    f.manager
        .set_enabled("b", b, false, f.manager.revision())
        .unwrap();
    assert!(f.plan("a", &[]).is_ok());
    assert!(f.plan("a", &[optional("a", "b")]).is_err());
}

#[test]
fn unrelated_missing_duplicate_or_required_optional_requests_are_rejected_without_mutation() {
    let mut f = Fixture::new(&[
        package("a", "1.0.0", &[("b", false)]),
        package("b", "1.0.0", &[]),
        package("x", "1.0.0", &[("b", true)]),
    ]);
    f.lock("a", "b", "b");
    f.lock("x", "b", "b");
    let before = f.persisted();
    for requests in [
        vec![optional("x", "b")],
        vec![optional("a", "missing")],
        vec![optional("a", "b")],
        vec![optional("x", "b"), optional("x", "b")],
        vec![optional("../bad", "b")],
    ] {
        assert!(f.plan("a", &requests).is_err());
        assert_eq!(f.persisted(), before);
    }
    assert!(matches!(
        f.manager.activation_plan(
            "../bad",
            &[optional("x", "b"), optional("x", "b")],
            f.manager.revision() - 1
        ),
        Err(ManagerError::Core(Error::RevisionConflict))
    ));
}

#[test]
fn optional_cycles_are_included_but_only_required_edges_control_order() {
    let mut f = Fixture::new(&[
        package("a", "1.0.0", &[("b", true)]),
        package("b", "1.0.0", &[("a", true)]),
    ]);
    f.lock("a", "b", "b");
    f.lock("b", "a", "a");
    let plan = f
        .plan("a", &[optional("b", "a"), optional("a", "b")])
        .unwrap();
    assert_eq!(ids(&plan), vec!["a", "b"]);
    assert!(plan.required_edges().is_empty());
    assert_eq!(plan.required(), &set(&["a"]));
    let mut mixed = Fixture::new(&[
        package("a", "1.0.0", &[("b", true)]),
        package("b", "1.0.0", &[("a", false)]),
    ]);
    mixed.lock("a", "b", "b");
    mixed.lock("b", "a", "a");
    let plan = mixed.plan("a", &[optional("a", "b")]).unwrap();
    assert_eq!(ids(&plan), vec!["a", "b"]);
    assert_eq!(plan.required_edges(), &[("b".into(), "a".into())]);
    assert_eq!(plan.required(), &set(&["a"]));
}

#[test]
fn upgrade_does_not_mutate_old_plan_or_reuse_its_approval() {
    let old = package("b", "1.0.0", &[]);
    let next = package("b", "1.1.0", &[]);
    let mut f = Fixture::new(&[package("a", "1.0.0", &[("b", false)]), old]);
    f.lock("a", "b", "b");
    let plan = f.plan("a", &[]).unwrap();
    let digest = plan.packages()[0].digest;
    Catalog::open(&f.root.path().join("packages"))
        .unwrap()
        .install(&next)
        .unwrap();
    f.manager.select(&next, f.manager.revision()).unwrap();
    assert!(matches!(
        f.manager.activation_plan("a", &[], plan.revision()),
        Err(ManagerError::Core(Error::RevisionConflict))
    ));
    assert!(f.plan("a", &[]).is_err());
    assert_eq!(plan.packages()[0].digest, digest);
    f.manager
        .set_enabled("b", next.digest(), true, f.manager.revision())
        .unwrap();
    assert!(f.plan("a", &[]).is_err());
    f.lock("a", "b", "b");
    let fresh = f.plan("a", &[]).unwrap();
    assert_eq!(fresh.packages()[0].digest, next.digest());
    assert_ne!(fresh.packages()[0].digest, digest);
}

#[test]
fn sixty_four_unique_nodes_are_allowed_and_the_next_is_rejected() {
    let names: Vec<_> = (0..65).map(|i| format!("node{i:02}")).collect();
    let packages: Vec<_> = (0..65)
        .map(|i| {
            package(
                &names[i],
                "1.0.0",
                if i == 64 { &[] } else { &[("next", false)] },
            )
        })
        .collect();
    let mut f = Fixture::new(&packages);
    for i in (0..64).rev() {
        f.lock(&names[i], "next", &names[i + 1]);
    }
    let plan = f.plan(&names[1], &[]).unwrap();
    assert_eq!(plan.packages().len(), 64);
    assert_eq!(plan.required().len(), 64);
    assert_eq!(plan.packages().first().unwrap().id, names[64]);
    assert_eq!(plan.packages().last().unwrap().id, names[1]);
    assert!(matches!(
        f.plan(&names[0], &[]),
        Err(ManagerError::Core(Error::Limit))
    ));
}
