#![cfg(all(feature = "packages", target_os = "windows"))]
use morrow_core::{
    content::CardRecord,
    dependency_call::Request,
    dispatch::HostRuntime,
    lifecycle::{GrantKind, InstancePhase},
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
    dynamic_dependencies::{Context, GraphLimits, RoutedOutput},
    instance_pool::{Limits as PoolLimits, Pool, Session},
    manager::{Manager, OptionalDependency},
    proposal::{EditProposal, EditTarget},
    shared_objects::{Limits, SharedObjects},
};
use std::collections::BTreeSet;
#[derive(Clone)]
struct Node {
    edges: Vec<usize>,
    optional: bool,
    duplicate: bool,
    trap: bool,
    small_output: bool,
}
fn node(edges: &[usize]) -> Node {
    Node {
        edges: edges.to_vec(),
        optional: false,
        duplicate: false,
        trap: false,
        small_output: false,
    }
}
fn id(index: usize) -> String {
    format!("test.graph.n{index}")
}
fn output(index: usize) -> Vec<u8> {
    vec![index as u8, 42, 0, 255]
}
fn root_input() -> Invocation {
    Invocation::new_transform(
        "root-graph",
        Transform {
            handler: "graph.transform".into(),
            input_type: "bytes".into(),
            output_type: "answer".into(),
            input: b"root\0graph".to_vec(),
        },
    )
    .unwrap()
}
struct Visit {
    input: Invocation,
    requests: Vec<Request>,
    responses: Vec<Vec<u8>>,
}
fn walk(
    nodes: &[Node],
    visits: &mut [Vec<Visit>],
    at: usize,
    input: Invocation,
    stack: &mut Vec<usize>,
) {
    assert!(stack.len() < 20);
    stack.push(at);
    let prefix = input
        .digest()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let mut requests = vec![];
    let mut responses = vec![];
    for (local, &child) in nodes[at].edges.iter().enumerate() {
        // Identical call IDs in separate guest tasks are intentionally valid.
        let request = Request::new(
            &format!("call{}", if nodes[at].duplicate { 0 } else { local }),
            &format!("edge{local}"),
            &[at as u8, child as u8, local as u8, 0, 255],
        )
        .unwrap();
        responses.push(request.encode_response("answer", &output(child)).unwrap());
        let nested = Invocation::new_transform(
            &format!("{prefix}-{}", local + 1),
            Transform {
                handler: "graph.transform".into(),
                input_type: "bytes".into(),
                output_type: "answer".into(),
                input: request.input().to_vec(),
            },
        )
        .unwrap();
        if !stack.contains(&child) {
            walk(nodes, visits, child, nested, stack);
        }
        requests.push(request);
    }
    visits[at].push(Visit {
        input,
        requests,
        responses,
    });
    stack.pop();
}
fn data(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("\\{b:02x}")).collect()
}
fn allocate(segments: &mut String, cursor: &mut usize, bytes: &[u8]) -> usize {
    let position = *cursor;
    *cursor = (*cursor + bytes.len() + 7) & !7;
    assert!(*cursor < 240000);
    segments.push_str(&format!("(data(i32.const {position}) \"{}\")", data(bytes)));
    position
}
const EQUAL: &str = r#"(func $equal(param $a i32)(param $b i32)(param $len i32)(result i32)(local $i i32)
 (loop $compare local.get $a local.get $i i32.add i32.load8_u local.get $b local.get $i i32.add i32.load8_u
 i32.ne if i32.const 0 return end local.get $i i32.const 1 i32.add local.tee $i local.get $len i32.lt_u br_if $compare)
 i32.const 1)"#;
fn package(index: usize, node: &Node, visits: &[Visit]) -> Package {
    let mut segments = String::new();
    let mut cursor = 0;
    let mut branches = String::new();
    for visit in visits {
        let p = allocate(&mut segments, &mut cursor, visit.input.bytes());
        let mut body = String::new();
        for (request, response) in visit.requests.iter().zip(&visit.responses) {
            let q = allocate(&mut segments, &mut cursor, request.bytes());
            let r = allocate(&mut segments, &mut cursor, response);
            body.push_str(&format!("i32.const {q} i32.const {} i32.const 393216 i32.const 131072 call $call i32.const {} i32.ne if unreachable end i32.const {r} i32.const 393216 i32.const {} call $equal i32.eqz if unreachable end ",request.bytes().len(),response.len(),response.len()));
        }
        let done = visit.input.output_completion(&output(index)).unwrap();
        let d = allocate(&mut segments, &mut cursor, &done);
        body.push_str(&format!(
            "i32.const {d} i32.const {} call $done drop {} i32.const 0 return",
            done.len(),
            if node.trap { "unreachable" } else { "" }
        ));
        // Every actual Invocation and every correlated response is inspected by the guest.
        branches.push_str(&format!("(if (i32.and (i32.eq (local.get $length)(i32.const {})) (call $equal (i32.const {p})(i32.const 262144)(i32.const {}))) (then {body}))",visit.input.bytes().len(),visit.input.bytes().len()));
    }
    let dependency_import = if node.edges.is_empty() {
        ""
    } else {
        r#"(import "morrow_dependency_v1" "call" (func $call(param i32 i32 i32 i32)(result i32)))"#
    };
    let module = wat::parse_str(format!(
        r#"(module
      (import "morrow_task_v1" "read_input" (func $read(param i32 i32)(result i32)))
      (import "morrow_task_v1" "complete" (func $done(param i32 i32)(result i32)))
      {dependency_import} (memory(export "memory") 8) {EQUAL} {segments}
      (func(export "morrow_run")(result i32)(local $length i32)
      i32.const 262144 i32.const 131072 call $read local.set $length {branches} unreachable))"#
    ))
    .unwrap();
    let mut manifest = Package::manifest_for_transform(
        &id(index),
        "1.0.0",
        &module,
        vec![TransformHandler {
            handler: "graph.transform".into(),
            input_type: "bytes".into(),
            output_type: "answer".into(),
            max_input_bytes: 65536,
            max_output_bytes: if node.small_output { 1 } else { 65536 },
        }],
    );
    if !node.edges.is_empty() {
        manifest
            .required_features
            .extend(["dependencies-v1".into(), "dependency-calls-v1".into()]);
        manifest.dependency_schema_sha256 = morrow_core::dependency_call::schema_digest().to_vec();
        manifest.dependencies = node
            .edges
            .iter()
            .enumerate()
            .map(|(i, _)| DependencyRequirement {
                slot: format!("edge{i}"),
                handler: "graph.transform".into(),
                input_type: "bytes".into(),
                output_type: "answer".into(),
                provider_version: "^1.0".into(),
                optional: node.optional,
            })
            .collect();
    }
    manifest.requested_capabilities = vec![Capability::EditContent as i32];
    Package::build(manifest, &module).unwrap()
}

struct Fixture {
    _root: tempfile::TempDir,
    manager: Manager,
    host: HostRuntime,
    objects: SharedObjects,
    pool: Pool,
    packages: Vec<Package>,
}
impl Fixture {
    fn new(optional: bool, invalid_leaf: bool) -> Self {
        let mut nodes = vec![node(&[1]), node(&[2]), node(&[])];
        nodes[0].optional = optional;
        Self::with_nodes(nodes, invalid_leaf)
    }
    fn with_nodes(nodes: Vec<Node>, invalid_leaf: bool) -> Self {
        let root = tempfile::tempdir().unwrap();
        let mut visits = (0..nodes.len()).map(|_| vec![]).collect::<Vec<_>>();
        walk(&nodes, &mut visits, 0, root_input(), &mut vec![]);
        let mut packages = nodes
            .iter()
            .zip(&visits)
            .enumerate()
            .map(|(i, (node, v))| package(i, node, v))
            .collect::<Vec<_>>();
        if invalid_leaf {
            let module=wat::parse_str(r#"(module (memory(export "memory") 1)(func $start)(start $start)(func(export "morrow_run")(result i32)i32.const 0))"#).unwrap();
            packages[2] = Package::build(
                Package::manifest_for_transform(
                    &id(2),
                    "1.0.0",
                    &module,
                    packages[2].manifest().transform_handlers.clone(),
                ),
                &module,
            )
            .unwrap();
        }
        let catalog = Catalog::open(&root.path().join("packages")).unwrap();
        for p in &packages {
            catalog.install(p).unwrap();
        }
        let mut manager = Manager::new(
            Registry::open(&root.path().join("registry"), catalog).unwrap(),
            RuntimeLimits::default(),
        );
        for p in &packages {
            manager.select(p, manager.revision()).unwrap();
        }
        manager
            .approve(
                &id(0),
                packages[0].digest(),
                BTreeSet::from([GrantKind::EditContent]),
                manager.revision(),
            )
            .unwrap();
        for (i, node) in nodes.iter().enumerate() {
            for (local, &child) in node.edges.iter().enumerate() {
                manager
                    .approve_dependency(
                        &id(i),
                        packages[i].digest(),
                        &format!("edge{local}"),
                        &id(child),
                        packages[child].digest(),
                        manager.revision(),
                    )
                    .unwrap();
            }
        }
        for p in &packages {
            manager
                .set_enabled(
                    &p.manifest().package_id,
                    p.digest(),
                    true,
                    manager.revision(),
                )
                .unwrap();
        }
        let mut store = Store::open(&root.path().join("db"), Default::default()).unwrap();
        store
            .create_local(
                "seed",
                &CardRecord::new("card", "test.card", 1, "old", b"old".to_vec()).unwrap(),
            )
            .unwrap();
        let host = HostRuntime::new(store).unwrap();
        let objects = SharedObjects::new(&host, Limits::default()).unwrap();
        let pool = Pool::new(&host, PoolLimits::default()).unwrap();
        Self {
            _root: root,
            manager,
            host,
            objects,
            pool,
            packages,
        }
    }
    fn start(&mut self, optional: &[OptionalDependency]) -> Session {
        let revision = self.manager.revision();
        self.pool
            .start(
                &mut self.manager,
                &mut self.host,
                &id(0),
                optional,
                revision,
            )
            .unwrap()
    }
    fn run(&mut self, s: &Session, now: u64) -> Result<RoutedOutput, ()> {
        self.pool
            .run(
                &self.manager,
                &mut self.host,
                &mut self.objects,
                s,
                &root_input(),
                Context::new("scope:a", 100),
                GraphLimits::default(),
                || now,
                Cancellation::default(),
            )
            .map_err(|_| ())
    }
    fn maintain(&mut self) {
        self.pool.maintain(&self.manager, &mut self.host).unwrap();
    }
    fn clean(&mut self) {
        let u = self.objects.usage();
        assert_eq!(u.objects, 0);
        assert_eq!(u.charged_bytes, 0);
        assert_eq!(u.leases, 0);
        assert_eq!(u.mappings, 0);
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
    fn grant(&mut self, s: &Session, now: u64) {
        self.pool
            .grant_root(&mut self.host, s, GrantKind::EditContent, "card", 100, now)
            .unwrap();
    }
}
fn proposal(value: RoutedOutput) -> EditProposal {
    EditProposal::new(
        value,
        EditTarget {
            operation_id: "edit",
            card_id: "card",
            expected_revision: 1,
            title: "pool",
            preview: "pool",
            accepted_output_type: "answer",
        },
    )
    .unwrap()
}

#[test]
fn sessions_share_providers_but_have_separate_root_bindings_and_content_grants() {
    let mut f = Fixture::new(false, false);
    let first = f.start(&[]);
    let second = f.start(&[]);
    assert_eq!(f.pool.usage().sessions, 2);
    assert_eq!(f.pool.usage().providers, 2);
    assert_ne!(
        f.pool.root(&first).unwrap().connection().binding(),
        f.pool.root(&second).unwrap().connection().binding()
    );
    let first_output = f.run(&first, 1).unwrap();
    let second_output = f.run(&second, 1).unwrap();
    assert_eq!(first_output.bytes(), output(0));
    assert_eq!(second_output.dependency_calls(), 2);
    f.grant(&first, 1);
    assert!(
        proposal(second_output)
            .commit(
                &mut f.host,
                f.pool.root(&second).unwrap().connection(),
                || 1
            )
            .is_err()
    );
    let edit = proposal(first_output);
    assert!(
        edit.commit(
            &mut f.host,
            f.pool.root(&second).unwrap().connection(),
            || 1
        )
        .is_err()
    );
    assert_eq!(
        edit.commit(&mut f.host, f.pool.root(&first).unwrap().connection(), || 1)
            .unwrap()
            .revision,
        2
    );
    f.clean();
}
#[test]
fn dropping_session_revokes_root_immediately_and_preserves_other_provider_consumers() {
    let mut f = Fixture::new(false, false);
    let first = f.start(&[]);
    let second = f.start(&[]);
    let binding = f.pool.root(&first).unwrap().connection().binding();
    let old = f.run(&first, 1).unwrap();
    drop(first);
    assert_ne!(f.host.binding_phase(binding), Ok(InstancePhase::Ready));
    assert!(old.validate_liveness(1).is_err());
    f.maintain();
    assert_eq!(f.pool.usage().sessions, 1);
    assert_eq!(f.pool.usage().providers, 2);
    assert!(f.run(&second, 1).is_ok());
    drop(second);
    f.maintain();
    assert_eq!(f.pool.usage().sessions, 0);
    assert_eq!(f.pool.usage().providers, 0);
    f.clean();
    f.unchanged();
}
#[test]
fn explicit_close_and_pool_drop_revoke_existing_results_without_rolling_back_commits() {
    let mut f = Fixture::new(false, false);
    let s = f.start(&[]);
    let old = f.run(&s, 1).unwrap();
    f.pool.close(&mut f.host, &s).unwrap();
    assert!(old.validate_liveness(1).is_err());
    assert!(f.pool.root(&s).is_err());
    f.maintain();
    assert_eq!(f.pool.usage().providers, 0);
    let s = f.start(&[]);
    f.grant(&s, 1);
    let value = f.run(&s, 1).unwrap();
    let edit = proposal(value);
    let receipt = edit
        .commit(&mut f.host, f.pool.root(&s).unwrap().connection(), || 1)
        .unwrap();
    let binding = f.pool.root(&s).unwrap().connection().binding();
    let replacement = Pool::new(&f.host, PoolLimits::default()).unwrap();
    drop(std::mem::replace(&mut f.pool, replacement));
    assert_ne!(f.host.binding_phase(binding), Ok(InstancePhase::Ready));
    assert_eq!(
        f.host
            .store_local()
            .lookup_for_card("card", "edit")
            .unwrap(),
        morrow_core::transaction::Lookup::Committed(receipt)
    );
}
#[test]
fn stale_revision_and_foreign_pool_or_host_never_change_live_session() {
    let mut f = Fixture::new(false, false);
    let s = f.start(&[]);
    let rev = f.manager.revision();
    assert!(
        f.pool
            .start(&mut f.manager, &mut f.host, &id(0), &[], rev - 1)
            .is_err()
    );
    assert!(
        f.pool
            .restart(&mut f.manager, &mut f.host, &s, rev - 1, 1)
            .is_err()
    );
    let mut foreign = Fixture::new(false, false);
    assert!(foreign.pool.root(&s).is_err());
    assert!(foreign.pool.close(&mut foreign.host, &s).is_err());
    assert!(
        f.pool
            .grant_root(
                &mut foreign.host,
                &s,
                GrantKind::EditContent,
                "card",
                100,
                1
            )
            .is_err()
    );
    assert!(f.pool.close(&mut foreign.host, &s).is_err());
    assert!(f.pool.maintain(&f.manager, &mut foreign.host).is_err());
    assert!(f.run(&s, 1).is_ok());
    assert_eq!(f.pool.usage().sessions, 1);
    assert_eq!(f.pool.usage().providers, 2);
    f.clean();
}
#[test]
fn session_and_provider_limits_reject_without_leaking_partial_roots() {
    let mut f = Fixture::new(false, false);
    f.pool = Pool::new(
        &f.host,
        PoolLimits {
            max_sessions: 1,
            ..PoolLimits::default()
        },
    )
    .unwrap();
    let s = f.start(&[]);
    let rev = f.manager.revision();
    assert!(
        f.pool
            .start(&mut f.manager, &mut f.host, &id(0), &[], rev)
            .is_err()
    );
    assert!(f.run(&s, 1).is_ok());
    drop(s);
    f.maintain();
    f.pool = Pool::new(
        &f.host,
        PoolLimits {
            max_providers: 1,
            ..PoolLimits::default()
        },
    )
    .unwrap();
    assert!(
        f.pool
            .start(&mut f.manager, &mut f.host, &id(0), &[], rev)
            .is_err()
    );
    assert_eq!(f.pool.usage().sessions, 0);
    assert_eq!(f.pool.usage().providers, 0);
    let mut available = vec![];
    while let Ok(c) = f.host.connect() {
        available.push(c);
    }
    assert_eq!(available.len(), 128);
    for c in &available {
        f.host.disconnect(c).unwrap();
    }
    f.unchanged();
}
#[test]
fn preparation_failure_restores_every_host_slot_and_pool_reference() {
    let mut f = Fixture::new(false, true);
    let rev = f.manager.revision();
    for _ in 0..3 {
        assert!(
            f.pool
                .start(&mut f.manager, &mut f.host, &id(0), &[], rev)
                .is_err()
        );
        assert_eq!(f.pool.usage().sessions, 0);
        assert_eq!(f.pool.usage().providers, 0);
    }
    let mut available = vec![];
    while let Ok(c) = f.host.connect() {
        available.push(c);
    }
    assert_eq!(
        available.len(),
        128,
        "failed provider prepare must close any previously created root/provider records"
    );
    for c in &available {
        f.host.disconnect(c).unwrap();
    }
    f.clean();
    f.unchanged();
}
#[test]
fn host_capacity_failure_rolls_back_partial_start_and_allows_fresh_start_after_space_is_freed() {
    let mut f = Fixture::new(false, false);
    let occupied = (0..127)
        .map(|_| f.host.connect().unwrap())
        .collect::<Vec<_>>();
    let rev = f.manager.revision();
    assert!(
        f.pool
            .start(&mut f.manager, &mut f.host, &id(0), &[], rev)
            .is_err()
    );
    assert_eq!(f.pool.usage().sessions, 0);
    assert_eq!(f.pool.usage().providers, 0);
    let last = f
        .host
        .connect()
        .expect("the one remaining host slot was leaked");
    assert!(f.host.connect().is_err());
    f.host.disconnect(&last).unwrap();
    for c in &occupied {
        f.host.disconnect(c).unwrap();
    }
    let s = f.start(&[]);
    assert!(f.run(&s, 1).is_ok());
    f.clean();
}
#[test]
fn required_provider_configuration_change_invalidates_all_dependent_sessions_and_outputs() {
    let mut f = Fixture::new(false, false);
    let one = f.start(&[]);
    let two = f.start(&[]);
    let old = f.run(&one, 1).unwrap();
    f.manager
        .set_enabled(&id(2), f.packages[2].digest(), false, f.manager.revision())
        .unwrap();
    f.maintain();
    assert!(old.validate_liveness(1).is_err());
    assert!(f.run(&one, 1).is_err());
    assert!(f.run(&two, 1).is_err());
    f.clean();
    f.unchanged();
}
#[test]
fn restart_returns_fresh_ungranted_root_never_replays_and_enforces_history_and_cooldown() {
    let mut f = Fixture::new(false, false);
    let original = f.start(&[]);
    f.grant(&original, 1);
    let old = f.run(&original, 1).unwrap();
    let rev = f.manager.revision();
    assert!(
        f.pool
            .restart(&mut f.manager, &mut f.host, &original, rev, 1)
            .is_err(),
        "a ready root must not restart"
    );
    f.pool.root(&original).unwrap().stop();
    let mut current = f
        .pool
        .restart(&mut f.manager, &mut f.host, &original, rev, 1)
        .unwrap();
    drop(original);
    assert!(old.validate_liveness(1).is_err());
    assert!(f.run(&current, 1).is_ok());
    f.unchanged();
    let edit = proposal(f.run(&current, 1).unwrap());
    assert!(
        edit.commit(
            &mut f.host,
            f.pool.root(&current).unwrap().connection(),
            || 1
        )
        .is_err(),
        "old root content grants must not survive restart"
    );
    f.pool.root(&current).unwrap().stop();
    assert!(
        f.pool
            .restart(&mut f.manager, &mut f.host, &current, rev, 1)
            .is_err()
    );
    for now in [2, 3] {
        let next = f
            .pool
            .restart(&mut f.manager, &mut f.host, &current, rev, now)
            .unwrap();
        drop(current);
        current = next;
        assert!(f.run(&current, now).is_ok());
        f.pool.root(&current).unwrap().stop();
    }
    assert!(
        f.pool
            .restart(&mut f.manager, &mut f.host, &current, rev, 4)
            .is_err(),
        "restart allowance is shared by replacement session history"
    );
    f.clean();
    f.unchanged();
}

#[test]
fn optional_unselected_has_no_providers_and_selected_optional_failure_preserves_ready_root() {
    let mut f = Fixture::new(true, false);
    let bare = f.start(&[]);
    assert_eq!(f.pool.usage().providers, 0);
    assert!(
        f.run(&bare, 1).is_err(),
        "the guest cannot request an unselected optional provider"
    );
    assert_eq!(
        f.host
            .connection_phase(f.pool.root(&bare).unwrap().connection()),
        Ok(InstancePhase::Ready)
    );
    let selected = f.start(&[OptionalDependency {
        caller: id(0),
        slot: "edge0".into(),
    }]);
    assert_eq!(f.pool.usage().providers, 2);
    let old = f.run(&selected, 1).unwrap();
    f.manager
        .set_enabled(&id(2), f.packages[2].digest(), false, f.manager.revision())
        .unwrap();
    f.maintain();
    assert!(old.validate_liveness(1).is_err());
    assert_eq!(
        f.host
            .connection_phase(f.pool.root(&bare).unwrap().connection()),
        Ok(InstancePhase::Ready)
    );
    assert_eq!(
        f.host
            .connection_phase(f.pool.root(&selected).unwrap().connection()),
        Ok(InstancePhase::Ready)
    );
    assert!(f.run(&selected, 1).is_err());
    f.clean();
    f.unchanged();
}
#[test]
fn stale_restart_after_root_stop_keeps_original_retry_budget_and_recovery_is_explicit() {
    let mut f = Fixture::new(false, false);
    let s = f.start(&[]);
    f.pool.root(&s).unwrap().stop();
    let rev = f.manager.revision();
    assert!(
        f.pool
            .restart(&mut f.manager, &mut f.host, &s, rev - 1, 1)
            .is_err()
    );
    let restored = f
        .pool
        .restart(&mut f.manager, &mut f.host, &s, rev, 1)
        .unwrap();
    f.unchanged();
    assert!(f.run(&restored, 1).is_ok());
    f.clean();
}

#[test]
fn another_manager_on_same_host_cannot_start_run_maintain_or_restart_this_pool() {
    let mut f = Fixture::new(false, false);
    let s = f.start(&[]);
    let mut foreign = Fixture::new(false, false);
    let rev = foreign.manager.revision();
    assert!(
        f.pool
            .start(&mut foreign.manager, &mut f.host, &id(0), &[], rev)
            .is_err()
    );
    assert!(f.pool.maintain(&foreign.manager, &mut f.host).is_err());
    assert!(
        f.pool
            .run(
                &foreign.manager,
                &mut f.host,
                &mut f.objects,
                &s,
                &root_input(),
                Context::new("scope:a", 100),
                GraphLimits::default(),
                || 1,
                Cancellation::default()
            )
            .is_err()
    );
    assert!(
        f.pool
            .restart(&mut foreign.manager, &mut f.host, &s, rev, 1)
            .is_err()
    );
    assert!(f.run(&s, 1).is_ok());
    assert_eq!(f.pool.usage().sessions, 1);
    assert_eq!(f.pool.usage().providers, 2);
    f.clean();
}
#[test]
fn real_leaf_trap_stops_required_consumers_but_preserves_optional_root() {
    for optional in [false, true] {
        let mut nodes = vec![node(&[1]), node(&[2]), node(&[])];
        nodes[0].optional = optional;
        nodes[2].trap = true;
        let mut f = Fixture::with_nodes(nodes, false);
        let choices = if optional {
            vec![OptionalDependency {
                caller: id(0),
                slot: "edge0".into(),
            }]
        } else {
            vec![]
        };
        let one = f.start(&choices);
        let two = f.start(&choices);
        assert!(f.run(&one, 1).is_err());
        assert_eq!(
            f.pool.usage().providers,
            0,
            "failed leaf and required intermediate must be reclaimed"
        );
        for s in [&one, &two] {
            if optional {
                assert_eq!(
                    f.host
                        .connection_phase(f.pool.root(s).unwrap().connection()),
                    Ok(InstancePhase::Ready)
                );
            } else {
                assert!(f.pool.root(s).is_err());
            }
        }
        f.clean();
        f.unchanged();
    }
}
#[test]
fn real_root_trap_does_not_stop_other_root_or_their_shared_providers() {
    let mut nodes = vec![node(&[1]), node(&[2]), node(&[])];
    nodes[0].trap = true;
    let mut f = Fixture::with_nodes(nodes, false);
    let one = f.start(&[]);
    let two = f.start(&[]);
    assert!(f.run(&one, 1).is_err());
    assert!(f.pool.root(&one).is_err());
    assert_eq!(
        f.host
            .connection_phase(f.pool.root(&two).unwrap().connection()),
        Ok(InstancePhase::Ready)
    );
    assert_eq!(f.pool.usage().providers, 2);
    f.clean();
    f.unchanged();
}
#[test]
fn graph_policy_limit_and_external_cancellation_do_not_quarantine_healthy_instances() {
    let mut f = Fixture::new(false, false);
    let s = f.start(&[]);
    assert!(
        f.pool
            .run(
                &f.manager,
                &mut f.host,
                &mut f.objects,
                &s,
                &root_input(),
                Context::new("scope:a", 100),
                GraphLimits {
                    max_depth: 1,
                    max_total_calls: 16
                },
                || 1,
                Cancellation::default()
            )
            .is_err()
    );
    assert!(f.pool.root(&s).is_ok());
    assert_eq!(f.pool.usage().providers, 2);
    let cancel = Cancellation::default();
    cancel.cancel();
    assert!(
        f.pool
            .run(
                &f.manager,
                &mut f.host,
                &mut f.objects,
                &s,
                &root_input(),
                Context::new("scope:a", 100),
                GraphLimits::default(),
                || 1,
                cancel
            )
            .is_err()
    );
    assert!(f.run(&s, 1).is_ok());
    f.clean();
    f.unchanged();
}

#[test]
fn failed_shared_provider_does_not_stop_separate_session_root_with_same_package_id() {
    let mut f = Fixture::with_nodes(vec![node(&[1]), node(&[])], false);
    let dependent = f.start(&[]);
    let rev = f.manager.revision();
    let independent = f
        .pool
        .start(&mut f.manager, &mut f.host, &id(1), &[], rev)
        .unwrap();
    let root_binding = f.pool.root(&independent).unwrap().connection().binding();
    assert_ne!(
        root_binding,
        f.pool.provider(&id(1)).unwrap().connection().binding()
    );
    f.pool.provider(&id(1)).unwrap().stop();
    f.maintain();
    assert!(f.pool.root(&dependent).is_err());
    assert_eq!(
        f.pool.root(&independent).unwrap().connection().binding(),
        root_binding
    );
    assert_eq!(f.host.binding_phase(root_binding), Ok(InstancePhase::Ready));
    assert_eq!(f.pool.usage().providers, 0);
    f.clean();
    f.unchanged();
}
#[test]
fn externally_retired_root_and_provider_records_are_cleaned_without_blocking_other_sessions() {
    let mut f = Fixture::new(false, false);
    let one = f.start(&[]);
    let two = f.start(&[]);
    f.host
        .disconnect(f.pool.root(&one).unwrap().connection())
        .unwrap();
    f.maintain();
    f.pool.close(&mut f.host, &one).unwrap();
    assert!(f.run(&two, 1).is_ok());
    f.pool.provider(&id(2)).unwrap().close(&mut f.host).unwrap();
    f.maintain();
    assert!(f.pool.root(&two).is_err());
    assert_eq!(f.pool.usage().providers, 0);
    let rev = f.manager.revision();
    let restored = f
        .pool
        .restart(&mut f.manager, &mut f.host, &two, rev, 1)
        .unwrap();
    drop(two);
    assert!(f.run(&restored, 1).is_ok());
    f.pool.root(&restored).unwrap().close(&mut f.host).unwrap();
    f.pool.close(&mut f.host, &restored).unwrap();
    f.maintain();
    assert_eq!(f.pool.usage().sessions, 0);
    assert_eq!(f.pool.usage().providers, 0);
    f.clean();
    f.unchanged();
}

#[test]
fn revocation_during_run_is_propagated_and_reclaimed_before_denied_is_returned() {
    let mut f = Fixture::with_nodes(vec![node(&[1]), node(&[])], false);
    let one = f.start(&[]);
    let two = f.start(&[]);
    let revision = f.manager.revision();
    let independent = f
        .pool
        .start(&mut f.manager, &mut f.host, &id(1), &[], revision)
        .unwrap();
    let healthy = f.pool.root(&independent).unwrap().connection().binding();
    let old = f.run(&one, 1).unwrap();
    let signal = f
        .host
        .revocation(f.pool.provider(&id(1)).unwrap().connection())
        .unwrap();
    let mut ticks = 0;
    let result = f.pool.run(
        &f.manager,
        &mut f.host,
        &mut f.objects,
        &one,
        &root_input(),
        Context::new("scope:a", 100),
        GraphLimits::default(),
        || {
            ticks += 1;
            if ticks == 2 {
                signal.revoke();
            }
            1
        },
        Cancellation::default(),
    );
    assert!(ticks >= 2);
    assert!(
        matches!(
            result,
            Err(morrow_plugin_runtime::instance_pool::Error::Execution(
                morrow_plugin_runtime::dependency::Error::Denied
            ))
        ),
        "external revocation must remain Denied, not be reclassified as a guest trap"
    );
    // No explicit maintain call here: returning from run must already close required roots.
    assert!(f.pool.root(&one).is_err());
    assert!(f.pool.root(&two).is_err());
    assert!(old.validate_liveness(1).is_err());
    assert_eq!(f.pool.usage().providers, 0);
    assert_eq!(
        f.pool.root(&independent).unwrap().connection().binding(),
        healthy
    );
    assert_eq!(f.host.binding_phase(healthy), Ok(InstancePhase::Ready));
    f.clean();
    f.unchanged();
}
