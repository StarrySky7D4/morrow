#![cfg(all(feature = "packages", target_os = "windows"))]
use morrow_core::{
    content::CardRecord,
    dependency_call::Request,
    dispatch::HostRuntime,
    lifecycle::GrantKind,
    plugin_package::{
        Package,
        catalog::Catalog,
        proto::{Capability, DependencyRequirement, TransformHandler},
        registry::Registry,
    },
    store::Store,
    task::{Invocation, Transform},
};
use morrow_plugin_runtime::{
    Cancellation, Limits as RuntimeLimits,
    dynamic_dependencies::{self as dynamic, Context, RoutedOutput},
    manager::{ManagedInstance, Manager},
    proposal::{EditProposal, EditTarget},
    shared_objects::{Limits, SharedObjects},
};
use std::collections::BTreeSet;
const CALLER: &str = "test.dynamic.caller";
fn data(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("\\{b:02x}")).collect()
}
fn invocation() -> Invocation {
    Invocation::new_transform(
        "root-task",
        Transform {
            handler: "root.compose".into(),
            input_type: "root-input".into(),
            output_type: "composed".into(),
            input: b"root binary \0 input".to_vec(),
        },
    )
    .unwrap()
}
const CHECK: &str = r#"(func $check(param $expected i32)(param $actual i32)(param $len i32)(local $i i32)
 (loop $each
  local.get $expected local.get $i i32.add i32.load8_u
  local.get $actual local.get $i i32.add i32.load8_u i32.ne if unreachable end
  local.get $i i32.const 1 i32.add local.tee $i local.get $len i32.lt_u br_if $each))"#;
fn provider(index: usize, recursive: bool, wrong_input: bool) -> Package {
    let root = invocation();
    let prefix = root
        .digest()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let input = if wrong_input {
        b"unexpected".to_vec()
    } else {
        vec![index as u8, 0, 255, 9]
    };
    let task = Invocation::new_transform(
        &format!("{prefix}-{}", index + 1),
        Transform {
            handler: "provider.transform".into(),
            input_type: "bytes".into(),
            output_type: "answer".into(),
            input,
        },
    )
    .unwrap();
    let done = task.output_completion(&[42, index as u8]).unwrap();
    let extra_import = if recursive {
        r#"(import "morrow_dependency_v1" "call" (func $recursive(param i32 i32 i32 i32)(result i32)))"#
    } else {
        ""
    };
    let module = wat::parse_str(format!(
        r#"(module
      (import "morrow_task_v1" "read_input" (func $read(param i32 i32)(result i32)))
      (import "morrow_task_v1" "complete" (func $done(param i32 i32)(result i32)))
      {extra_import} (memory(export "memory") 4) {CHECK}
      (data(i32.const 0) "{}") (data(i32.const 16384) "{}")
      (func(export "morrow_run")(result i32)
        i32.const 65536 i32.const 131072 call $read i32.const {} i32.ne if unreachable end
        i32.const 0 i32.const 65536 i32.const {} call $check
        i32.const 16384 i32.const {} call $done drop i32.const 0))"#,
        data(task.bytes()),
        data(&done),
        task.bytes().len(),
        task.bytes().len(),
        done.len()
    ))
    .unwrap();
    let mut manifest = Package::manifest_for_transform(
        &format!("test.dynamic.p{index}"),
        "1.0.0",
        &module,
        vec![TransformHandler {
            handler: "provider.transform".into(),
            input_type: "bytes".into(),
            output_type: "answer".into(),
            max_input_bytes: 65536,
            max_output_bytes: 65536,
        }],
    );
    if recursive {
        manifest
            .required_features
            .extend(["dependencies-v1".into(), "dependency-calls-v1".into()]);
        manifest.dependency_schema_sha256 = morrow_core::dependency_call::schema_digest().to_vec();
    }
    Package::build(manifest, &module).unwrap()
}
#[derive(Clone, Copy, Default)]
struct Mode {
    duplicate: bool,
    malformed: bool,
    trap_after: bool,
    recursive_provider: bool,
    wrong_provider: bool,
    direct_write: bool,
}
fn caller(count: usize, mode: Mode) -> Package {
    let input = invocation();
    let expected = (0..count).flat_map(|i| [42, i as u8]).collect::<Vec<_>>();
    let done = input.output_completion(&expected).unwrap();
    let mut segments = format!(
        "(data(i32.const 0) \"{}\") (data(i32.const 131072) \"{}\")",
        data(input.bytes()),
        data(&done)
    );
    let mut body = format!(
        "i32.const 262144 i32.const 131072 call $read i32.const {} i32.ne if unreachable end i32.const 0 i32.const 262144 i32.const {} call $check",
        input.bytes().len(),
        input.bytes().len()
    );
    if mode.direct_write {
        let command = morrow_core::runtime::Command::EditContent(
            morrow_core::content_change::ContentChange {
                operation_id: "guest-write".into(),
                card_id: "card".into(),
                expected_revision: 1,
                title: "forbidden".into(),
                body: b"must not save".to_vec(),
                preview_text: "forbidden".into(),
                attachments: None,
            },
        )
        .encode()
        .unwrap();
        segments.push_str(&format!("(data(i32.const 4096) \"{}\")", data(&command)));
        body.push_str(&format!(
            " i32.const 4096 i32.const {} i32.const 393216 i32.const 65536 call $exchange drop",
            command.len()
        ));
    }
    let mut requirements = vec![];
    for i in 0..count {
        let request = Request::new(
            &format!("call{}", if mode.duplicate { 0 } else { i }),
            &format!("slot{i}"),
            &[i as u8, 0, 255, 9],
        )
        .unwrap();
        let response = request.encode_response("answer", &[42, i as u8]).unwrap();
        let raw = if mode.malformed {
            b"invalid frame".as_slice()
        } else {
            request.bytes()
        };
        let rp = 4096 + i * 4096;
        let ep = 65536 + i * 4096;
        segments.push_str(&format!(
            "(data(i32.const {rp}) \"{}\") (data(i32.const {ep}) \"{}\")",
            data(raw),
            data(&response)
        ));
        // Both complete request and complete correlated response are checked in real Wasm.
        body.push_str(&format!(" i32.const {rp} i32.const {} i32.const 393216 i32.const 131072 call $call i32.const {} i32.ne if unreachable end i32.const {ep} i32.const 393216 i32.const {} call $check", raw.len(), response.len(), response.len()));
        requirements.push(DependencyRequirement {
            slot: format!("slot{i}"),
            handler: "provider.transform".into(),
            input_type: "bytes".into(),
            output_type: "answer".into(),
            provider_version: "^1.0".into(),
            optional: false,
        });
    }
    body.push_str(&format!(
        " i32.const 131072 i32.const {} call $done drop {} i32.const 0",
        done.len(),
        if mode.trap_after { "unreachable" } else { "" }
    ));
    let module = wat::parse_str(format!(
        r#"(module
       (import "morrow_task_v1" "read_input" (func $read(param i32 i32)(result i32)))
       (import "morrow_task_v1" "complete" (func $done(param i32 i32)(result i32)))
       (import "morrow_dependency_v1" "call" (func $call(param i32 i32 i32 i32)(result i32)))
       (import "morrow_v1" "exchange" (func $exchange(param i32 i32 i32 i32)(result i32)))
       (memory(export "memory") 8) {CHECK} {segments}
       (func(export "morrow_run")(result i32) {body}))"#
    ))
    .unwrap();
    let mut manifest = Package::manifest_for_transform(
        CALLER,
        "1.0.0",
        &module,
        vec![TransformHandler {
            handler: "root.compose".into(),
            input_type: "root-input".into(),
            output_type: "composed".into(),
            max_input_bytes: 65536,
            max_output_bytes: 65536,
        }],
    );
    manifest
        .required_features
        .extend(["dependencies-v1".into(), "dependency-calls-v1".into()]);
    manifest.dependency_schema_sha256 = morrow_core::dependency_call::schema_digest().to_vec();
    manifest.dependencies = requirements;
    manifest.requested_capabilities = vec![Capability::EditContent as i32];
    Package::build(manifest, &module).unwrap()
}
struct Fixture {
    _root: tempfile::TempDir,
    manager: Manager,
    host: HostRuntime,
    caller: ManagedInstance,
    providers: Vec<ManagedInstance>,
    objects: SharedObjects,
}
impl Fixture {
    fn new(count: usize, mode: Mode) -> Self {
        let root = tempfile::tempdir().unwrap();
        let cp = caller(count, mode);
        let pp = (0..count)
            .map(|i| {
                provider(
                    i,
                    mode.recursive_provider && i == 0,
                    mode.wrong_provider && i == 0,
                )
            })
            .collect::<Vec<_>>();
        let catalog = Catalog::open(&root.path().join("packages")).unwrap();
        catalog.install(&cp).unwrap();
        for p in &pp {
            catalog.install(p).unwrap();
        }
        let mut manager = Manager::new(
            Registry::open(&root.path().join("registry"), catalog).unwrap(),
            RuntimeLimits::default(),
        );
        for p in &pp {
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
        manager.select(&cp, manager.revision()).unwrap();
        manager
            .approve(
                CALLER,
                cp.digest(),
                BTreeSet::from([GrantKind::EditContent]),
                manager.revision(),
            )
            .unwrap();
        for (i, p) in pp.iter().enumerate() {
            manager
                .approve_dependency(
                    CALLER,
                    cp.digest(),
                    &format!("slot{i}"),
                    &p.manifest().package_id,
                    p.digest(),
                    manager.revision(),
                )
                .unwrap();
        }
        manager
            .set_enabled(CALLER, cp.digest(), true, manager.revision())
            .unwrap();
        let mut store = Store::open(&root.path().join("db"), Default::default()).unwrap();
        store
            .create_local(
                "seed",
                &CardRecord::new("card", "test.card", 1, "old", b"old".to_vec()).unwrap(),
            )
            .unwrap();
        let mut host = HostRuntime::new(store).unwrap();
        let caller = manager.connect(CALLER, &mut host).unwrap();
        let providers = pp
            .iter()
            .map(|p| {
                manager
                    .connect(&p.manifest().package_id, &mut host)
                    .unwrap()
            })
            .collect();
        let objects = SharedObjects::new(&host, Limits::default()).unwrap();
        Self {
            _root: root,
            manager,
            host,
            caller,
            providers,
            objects,
        }
    }
    fn run(
        &mut self,
        context: Context<'_>,
        clock: impl FnMut() -> u64,
        cancel: Cancellation,
    ) -> Result<RoutedOutput, morrow_plugin_runtime::dependency::Error> {
        dynamic::run(
            &self.manager,
            &mut self.host,
            &mut self.objects,
            &self.caller,
            &self.providers.iter().collect::<Vec<_>>(),
            &invocation(),
            context,
            clock,
            cancel,
        )
    }
    fn clean(&mut self) {
        let usage = self.objects.usage();
        assert_eq!(usage.objects, 0);
        assert_eq!(usage.charged_bytes, 0);
        assert_eq!(usage.leases, 0);
        assert_eq!(usage.mappings, 0);
    }
    fn unchanged(&self) {
        assert_eq!(
            self.host
                .store_local()
                .card("card")
                .unwrap()
                .unwrap()
                .body(),
            b"old"
        );
        assert_eq!(self.host.store_local().pending(0, 10).unwrap().len(), 1);
    }
    fn grant(&mut self) {
        self.host
            .grant(
                self.caller.parts_mut().1,
                GrantKind::EditContent,
                "card",
                100,
                1,
            )
            .unwrap();
    }
}
fn context() -> Context<'static> {
    Context::new("workspace:a", 10)
}
fn proposal(output: RoutedOutput) -> EditProposal {
    EditProposal::new(
        output,
        EditTarget {
            operation_id: "edit",
            card_id: "card",
            expected_revision: 1,
            title: "composed",
            preview: "dynamic",
            accepted_output_type: "composed",
        },
    )
    .unwrap()
}

#[test]
fn real_guest_sends_two_requests_checks_complete_responses_and_commits_composed_body() {
    let mut f = Fixture::new(2, Mode::default());
    let output = f.run(context(), || 1, Cancellation::default()).unwrap();
    assert_eq!(output.bytes(), [42, 0, 42, 1]);
    assert_eq!(output.dependency_calls(), 2);
    assert_eq!(output.output_type(), "composed");
    output.validate(&f.host, f.caller.connection(), 1).unwrap();
    f.clean();
    f.unchanged();
    let edit = proposal(output);
    assert!(
        edit.commit(&mut f.host, f.caller.connection(), || 1)
            .is_err()
    );
    f.grant();
    let receipt = edit
        .commit(&mut f.host, f.caller.connection(), || 1)
        .unwrap();
    assert_eq!(receipt.revision, 2);
    assert_eq!(
        edit.commit(&mut f.host, f.caller.connection(), || 1)
            .unwrap(),
        receipt
    );
    assert_eq!(
        f.host.store_local().card("card").unwrap().unwrap().body(),
        [42, 0, 42, 1]
    );
}
#[test]
fn zero_dependency_completion_still_requires_manager_ownership_and_original_control_binding() {
    let mut f = Fixture::new(0, Mode::default());
    let other = Fixture::new(0, Mode::default());
    assert!(
        dynamic::run(
            &other.manager,
            &mut f.host,
            &mut f.objects,
            &f.caller,
            &[],
            &invocation(),
            context(),
            || 1,
            Cancellation::default()
        )
        .is_err()
    );
    let output = f.run(context(), || 1, Cancellation::default()).unwrap();
    assert_eq!(output.dependency_calls(), 0);
    assert!(output.bytes().is_empty());
    let mut second = f.manager.connect(CALLER, &mut f.host).unwrap();
    std::mem::swap(f.caller.parts_mut().1, second.parts_mut().1);
    assert!(f.run(context(), || 1, Cancellation::default()).is_err());
    f.clean();
    f.unchanged();
}
#[test]
fn all_used_providers_remain_required_by_output_even_when_only_first_is_revoked() {
    let mut f = Fixture::new(2, Mode::default());
    let output = f.run(context(), || 1, Cancellation::default()).unwrap();
    f.host
        .revocation(f.providers[0].connection())
        .unwrap()
        .revoke();
    assert!(output.validate(&f.host, f.caller.connection(), 1).is_err());
    assert!(output.validate_liveness(1).is_err());
    f.grant();
    assert!(
        proposal(output)
            .commit(&mut f.host, f.caller.connection(), || 1)
            .is_err()
    );
    f.clean();
    f.unchanged();
}
#[test]
fn failure_after_prior_success_cleans_every_published_input_and_never_returns_partial_output() {
    for mode in [
        Mode {
            duplicate: true,
            ..Mode::default()
        },
        Mode {
            malformed: true,
            ..Mode::default()
        },
        Mode {
            trap_after: true,
            ..Mode::default()
        },
        Mode {
            wrong_provider: true,
            ..Mode::default()
        },
        Mode {
            recursive_provider: true,
            ..Mode::default()
        },
    ] {
        let mut f = Fixture::new(2, mode);
        assert!(f.run(context(), || 1, Cancellation::default()).is_err());
        f.clean();
        f.unchanged();
    }
}
#[test]
fn default_eight_call_limit_and_trusted_hard_sixteen_limit_are_enforced() {
    let mut f = Fixture::new(9, Mode::default());
    assert!(f.run(context(), || 1, Cancellation::default()).is_err());
    f.clean();
    let output = f
        .run(
            Context {
                max_calls: 9,
                ..context()
            },
            || 1,
            Cancellation::default(),
        )
        .unwrap();
    assert_eq!(output.dependency_calls(), 9);
    f.clean();
    assert!(
        f.run(
            Context {
                max_calls: 17,
                ..context()
            },
            || 1,
            Cancellation::default()
        )
        .is_err()
    );
    assert!(
        f.run(
            Context {
                max_calls: 0,
                ..context()
            },
            || 1,
            Cancellation::default()
        )
        .is_err()
    );
    f.unchanged();
}
#[test]
fn missing_duplicate_foreign_and_swapped_provider_instances_never_execute() {
    let mut f = Fixture::new(1, Mode::default());
    assert!(
        dynamic::run(
            &f.manager,
            &mut f.host,
            &mut f.objects,
            &f.caller,
            &[],
            &invocation(),
            context(),
            || 1,
            Cancellation::default()
        )
        .is_err()
    );
    assert!(
        dynamic::run(
            &f.manager,
            &mut f.host,
            &mut f.objects,
            &f.caller,
            &[&f.providers[0], &f.providers[0]],
            &invocation(),
            context(),
            || 1,
            Cancellation::default()
        )
        .is_err()
    );
    let other = Fixture::new(1, Mode::default());
    assert!(
        dynamic::run(
            &f.manager,
            &mut f.host,
            &mut f.objects,
            &f.caller,
            &[&other.providers[0]],
            &invocation(),
            context(),
            || 1,
            Cancellation::default()
        )
        .is_err()
    );
    let mut second = f.manager.connect("test.dynamic.p0", &mut f.host).unwrap();
    std::mem::swap(f.providers[0].parts_mut().1, second.parts_mut().1);
    assert!(f.run(context(), || 1, Cancellation::default()).is_err());
    f.clean();
    f.unchanged();
}
#[test]
fn map_capacity_failure_retires_new_object_and_preserves_unrelated_mapping() {
    let mut f = Fixture::new(1, Mode::default());
    f.objects = SharedObjects::new(
        &f.host,
        Limits {
            mappings: 1,
            ..Limits::default()
        },
    )
    .unwrap();
    let desc = f
        .objects
        .publish(&f.host, f.caller.connection(), "workspace:held", b"held", 1)
        .unwrap();
    let lease = f
        .objects
        .grant(
            &f.host,
            f.caller.connection(),
            &desc,
            "workspace:held",
            100,
            1,
        )
        .unwrap();
    let mapping = f
        .objects
        .map(&f.host, f.caller.connection(), &lease, 1)
        .unwrap();
    let before = f.objects.usage();
    assert!(f.run(context(), || 1, Cancellation::default()).is_err());
    assert_eq!(f.objects.usage(), before);
    assert_eq!(mapping.bytes(), b"held");
    f.objects.retire(&desc).unwrap();
    drop(mapping);
    f.clean();
    f.unchanged();
}
#[test]
fn expiry_and_cancellation_before_run_or_at_final_output_boundary_discard_success() {
    let mut base = Fixture::new(2, Mode::default());
    let mut calls = 0;
    base.run(
        context(),
        || {
            calls += 1;
            1
        },
        Cancellation::default(),
    )
    .unwrap();
    for mode in 0..5 {
        let mut f = Fixture::new(2, Mode::default());
        let signal = f.host.revocation(f.providers[0].connection()).unwrap();
        let cancel = Cancellation::default();
        if mode == 0 {
            cancel.cancel();
        }
        let mut ticks = 0;
        assert!(
            f.run(
                context(),
                || {
                    ticks += 1;
                    if ticks == calls {
                        if mode == 2 {
                            signal.revoke();
                        }
                        if mode == 3 {
                            cancel.cancel();
                        }
                    }
                    if mode == 1 || (mode == 4 && ticks == calls) {
                        10
                    } else {
                        1
                    }
                },
                cancel.clone()
            )
            .is_err()
        );
        f.clean();
        f.unchanged();
    }
}
#[test]
fn final_content_guard_checks_each_provider_caller_expiry_and_external_cancellation() {
    let mut base = Fixture::new(2, Mode::default());
    base.grant();
    let edit = proposal(base.run(context(), || 1, Cancellation::default()).unwrap());
    let mut calls = 0;
    edit.commit(&mut base.host, base.caller.connection(), || {
        calls += 1;
        1
    })
    .unwrap();
    assert!(calls >= 3);
    for mode in 0..5 {
        let mut f = Fixture::new(2, Mode::default());
        f.grant();
        let cancel = Cancellation::default();
        let edit = proposal(f.run(context(), || 1, cancel.clone()).unwrap());
        let signal = f
            .host
            .revocation(if mode == 2 {
                f.caller.connection()
            } else {
                f.providers[usize::from(mode == 1)].connection()
            })
            .unwrap();
        let mut ticks = 0;
        assert!(
            edit.commit(&mut f.host, f.caller.connection(), || {
                ticks += 1;
                if ticks == calls {
                    if mode <= 2 {
                        signal.revoke();
                    }
                    if mode == 4 {
                        cancel.cancel();
                    }
                }
                if mode == 3 && ticks == calls { 10 } else { 1 }
            })
            .is_err()
        );
        assert_eq!(ticks, calls);
        f.clean();
        f.unchanged();
    }
}
#[test]
fn completed_commit_is_never_rolled_back_by_later_provider_stop() {
    let mut f = Fixture::new(2, Mode::default());
    f.grant();
    let edit = proposal(f.run(context(), || 1, Cancellation::default()).unwrap());
    let receipt = edit
        .commit(&mut f.host, f.caller.connection(), || 1)
        .unwrap();
    f.providers[0].stop();
    assert!(
        edit.commit(&mut f.host, f.caller.connection(), || 1)
            .is_err()
    );
    assert_eq!(
        f.host
            .store_local()
            .lookup_for_card("card", "edit")
            .unwrap(),
        morrow_core::transaction::Lookup::Committed(receipt)
    );
    assert_eq!(
        f.host.store_local().card("card").unwrap().unwrap().body(),
        [42, 0, 42, 1]
    );
    f.clean();
}

#[test]
fn caller_with_real_edit_grant_cannot_bypass_proposal_through_regular_exchange() {
    let mut f = Fixture::new(
        0,
        Mode {
            direct_write: true,
            ..Mode::default()
        },
    );
    f.grant();
    let result = f.run(context(), || 1, Cancellation::default());
    assert_eq!(
        f.host
            .store_local()
            .lookup_for_card("card", "guest-write")
            .unwrap(),
        morrow_core::transaction::Lookup::Absent
    );
    f.clean();
    f.unchanged();
    assert!(
        result.is_err(),
        "a denied direct exchange must poison the final output"
    );
}
