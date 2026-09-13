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
    dependency::Error,
    dynamic_dependencies::{self as dynamic, Context, GraphLimits, RoutedOutput},
    manager::{ManagedInstance, Manager},
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
    instances: Vec<ManagedInstance>,
    objects: SharedObjects,
}
impl Fixture {
    fn new(nodes: Vec<Node>) -> Self {
        let root = tempfile::tempdir().unwrap();
        let mut visits = (0..nodes.len()).map(|_| vec![]).collect::<Vec<_>>();
        walk(&nodes, &mut visits, 0, root_input(), &mut vec![]);
        let packages = nodes
            .iter()
            .zip(&visits)
            .enumerate()
            .map(|(i, (node, v))| package(i, node, v))
            .collect::<Vec<_>>();
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
        let mut host = HostRuntime::new(store).unwrap();
        let instances = (0..nodes.len())
            .map(|i| manager.connect(&id(i), &mut host).unwrap())
            .collect();
        let objects = SharedObjects::new(&host, Limits::default()).unwrap();
        Self {
            _root: root,
            manager,
            host,
            instances,
            objects,
        }
    }
    fn run(
        &mut self,
        limits: GraphLimits,
        clock: impl FnMut() -> u64,
        cancel: Cancellation,
    ) -> Result<RoutedOutput, Error> {
        dynamic::run_graph(
            &self.manager,
            &mut self.host,
            &mut self.objects,
            &self.instances[0],
            &self.instances.iter().collect::<Vec<_>>(),
            &root_input(),
            Context::new("scope:a", 10),
            limits,
            clock,
            cancel,
        )
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
    fn grant(&mut self) {
        self.host
            .grant(
                self.instances[0].parts_mut().1,
                GrantKind::EditContent,
                "card",
                100,
                1,
            )
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
            title: "graph",
            preview: "graph",
            accepted_output_type: "answer",
        },
    )
    .unwrap()
}
fn chain() -> Vec<Node> {
    vec![node(&[1]), node(&[2]), node(&[])]
}

#[test]
fn three_level_real_wasm_validates_every_input_and_response_then_root_proposal_commits() {
    let mut f = Fixture::new(chain());
    let value = f
        .run(GraphLimits::default(), || 1, Cancellation::default())
        .unwrap();
    assert_eq!(value.bytes(), output(0));
    assert_eq!(value.dependency_calls(), 2);
    value
        .validate(&f.host, f.instances[0].connection(), 1)
        .unwrap();
    f.clean();
    f.unchanged();
    f.grant();
    let edit = proposal(value);
    let receipt = edit
        .commit(&mut f.host, f.instances[0].connection(), || 1)
        .unwrap();
    assert_eq!(receipt.revision, 2);
    assert_eq!(
        edit.commit(&mut f.host, f.instances[0].connection(), || 1)
            .unwrap(),
        receipt
    );
}
#[test]
fn sequential_diamond_reuses_leaf_after_return_and_call_ids_are_local_to_each_task() {
    let mut f = Fixture::new(vec![node(&[1, 2]), node(&[3]), node(&[3]), node(&[])]);
    let value = f
        .run(
            GraphLimits {
                max_depth: 2,
                max_total_calls: 4,
            },
            || 1,
            Cancellation::default(),
        )
        .unwrap();
    assert_eq!(value.dependency_calls(), 4);
    assert_eq!(value.bytes(), output(0));
    value
        .validate(&f.host, f.instances[0].connection(), 1)
        .unwrap();
    f.clean();
    // A second fresh task must not retain the previous runtime active stack or local IDs.
    assert!(
        f.run(
            GraphLimits {
                max_depth: 2,
                max_total_calls: 4
            },
            || 1,
            Cancellation::default()
        )
        .is_ok()
    );
    f.clean();
}
#[test]
fn depth_counts_edges_and_global_calls_do_not_reset_at_each_nested_guest() {
    let mut f = Fixture::new(chain());
    assert!(matches!(
        f.run(
            GraphLimits {
                max_depth: 1,
                max_total_calls: 16
            },
            || 1,
            Cancellation::default()
        ),
        Err(Error::Limit)
    ));
    f.clean();
    assert!(
        f.run(
            GraphLimits {
                max_depth: 2,
                max_total_calls: 2
            },
            || 1,
            Cancellation::default()
        )
        .is_ok()
    );
    f.clean();
    let mut diamond = Fixture::new(vec![node(&[1, 2]), node(&[3]), node(&[3]), node(&[])]);
    assert!(matches!(
        diamond.run(
            GraphLimits {
                max_depth: 2,
                max_total_calls: 3
            },
            || 1,
            Cancellation::default()
        ),
        Err(Error::Limit)
    ));
    diamond.clean();
    diamond.unchanged();
    assert!(
        diamond
            .run(
                GraphLimits {
                    max_depth: 2,
                    max_total_calls: 4
                },
                || 1,
                Cancellation::default()
            )
            .is_ok()
    );
    diamond.clean();
}
#[test]
fn graph_limit_validation_rejects_zero_and_hard_cap_excess_before_any_allocation() {
    let mut f = Fixture::new(chain());
    for limits in [
        GraphLimits {
            max_depth: 0,
            max_total_calls: 16,
        },
        GraphLimits {
            max_depth: 9,
            max_total_calls: 16,
        },
        GraphLimits {
            max_depth: 4,
            max_total_calls: 0,
        },
        GraphLimits {
            max_depth: 4,
            max_total_calls: 65,
        },
    ] {
        assert!(matches!(
            f.run(limits, || 1, Cancellation::default()),
            Err(Error::Limit)
        ));
        f.clean();
    }
    f.unchanged();
}
#[test]
fn optional_manifest_cycle_is_rejected_by_active_package_stack_at_runtime() {
    let mut nodes = vec![node(&[1]), node(&[0])];
    for n in &mut nodes {
        n.optional = true;
    }
    let mut f = Fixture::new(nodes);
    assert!(
        matches!(
            f.run(GraphLimits::default(), || 1, Cancellation::default()),
            Err(Error::Denied)
        ),
        "must reject active package before entering a mismatched guest invocation"
    );
    f.clean();
    f.unchanged();
}
#[test]
fn active_package_reentry_is_denied_even_when_it_targets_another_managed_instance() {
    let mut nodes = vec![node(&[1]), node(&[0])];
    for n in &mut nodes {
        n.optional = true;
    }
    let mut f = Fixture::new(nodes);
    let another_root = f.manager.connect(&id(0), &mut f.host).unwrap();
    let result = dynamic::run_graph(
        &f.manager,
        &mut f.host,
        &mut f.objects,
        &f.instances[0],
        &[&another_root, &f.instances[1]],
        &root_input(),
        Context::new("scope:a", 10),
        GraphLimits::default(),
        || 1,
        Cancellation::default(),
    );
    assert!(matches!(result, Err(Error::Denied)));
    f.clean();
    f.unchanged();
}
#[test]
fn duplicate_call_id_within_one_guest_is_rejected_but_prior_nested_results_do_not_leak() {
    let mut root = node(&[1, 1]);
    root.duplicate = true;
    let mut f = Fixture::new(vec![root, node(&[])]);
    assert!(matches!(
        f.run(GraphLimits::default(), || 1, Cancellation::default()),
        Err(Error::Limit)
    ));
    f.clean();
    f.unchanged();
}
#[test]
fn deep_trap_or_oversize_output_after_successful_sibling_cleans_all_resources() {
    for small in [false, true] {
        let mut leaf = node(&[]);
        leaf.trap = !small;
        leaf.small_output = small;
        let mut f = Fixture::new(vec![node(&[1, 2]), node(&[]), node(&[3]), leaf]);
        assert!(
            f.run(GraphLimits::default(), || 1, Cancellation::default())
                .is_err()
        );
        f.clean();
        f.unchanged();
    }
}
#[test]
fn deepest_leaf_stop_invalidates_root_output_without_revoking_root_connection() {
    let mut f = Fixture::new(chain());
    let value = f
        .run(GraphLimits::default(), || 1, Cancellation::default())
        .unwrap();
    f.instances[2].stop();
    assert_eq!(
        f.host.connection_phase(f.instances[0].connection()),
        Ok(InstancePhase::Ready)
    );
    assert!(
        value
            .validate(&f.host, f.instances[0].connection(), 1)
            .is_err()
    );
    assert!(value.validate_liveness(1).is_err());
    f.grant();
    assert!(
        proposal(value)
            .commit(&mut f.host, f.instances[0].connection(), || 1)
            .is_err()
    );
    f.clean();
    f.unchanged();
}
#[test]
fn deepest_leaf_revocation_or_expiry_at_final_root_delivery_discards_completed_graph() {
    let mut baseline = Fixture::new(chain());
    let mut calls = 0;
    baseline
        .run(
            GraphLimits::default(),
            || {
                calls += 1;
                1
            },
            Cancellation::default(),
        )
        .unwrap();
    for expire in [false, true] {
        let mut f = Fixture::new(chain());
        let signal = f.host.revocation(f.instances[2].connection()).unwrap();
        let mut ticks = 0;
        assert!(
            f.run(
                GraphLimits::default(),
                || {
                    ticks += 1;
                    if ticks == calls && !expire {
                        signal.revoke();
                    }
                    if ticks == calls && expire { 10 } else { 1 }
                },
                Cancellation::default()
            )
            .is_err()
        );
        assert_eq!(ticks, calls);
        f.clean();
        f.unchanged();
    }
}
#[test]
fn final_commit_guard_rechecks_leaf_cancellation_root_cancellation_and_deadline() {
    let mut baseline = Fixture::new(chain());
    baseline.grant();
    let value = baseline
        .run(GraphLimits::default(), || 1, Cancellation::default())
        .unwrap();
    let mut calls = 0;
    proposal(value)
        .commit(
            &mut baseline.host,
            baseline.instances[0].connection(),
            || {
                calls += 1;
                1
            },
        )
        .unwrap();
    assert!(calls >= 3);
    for mode in 0..3 {
        let mut f = Fixture::new(chain());
        f.grant();
        let cancel = Cancellation::default();
        let value = f.run(GraphLimits::default(), || 1, cancel.clone()).unwrap();
        let edit = proposal(value);
        let mut ticks = 0;
        assert!(
            edit.commit(&mut f.host, f.instances[0].connection(), || {
                ticks += 1;
                if ticks == calls {
                    if mode == 0 {
                        f.instances[2].stop();
                    }
                    if mode == 1 {
                        cancel.cancel();
                    }
                }
                if mode == 2 && ticks == calls { 10 } else { 1 }
            })
            .is_err()
        );
        assert_eq!(ticks, calls);
        f.clean();
        f.unchanged();
    }
}
#[test]
fn committed_root_edit_survives_later_leaf_stop_and_original_single_level_api_stays_strict() {
    let mut f = Fixture::new(chain());
    assert!(
        dynamic::run(
            &f.manager,
            &mut f.host,
            &mut f.objects,
            &f.instances[0],
            &f.instances.iter().collect::<Vec<_>>(),
            &root_input(),
            Context::new("scope:a", 10),
            || 1,
            Cancellation::default()
        )
        .is_err()
    );
    f.clean();
    f.grant();
    let edit = proposal(
        f.run(GraphLimits::default(), || 1, Cancellation::default())
            .unwrap(),
    );
    let receipt = edit
        .commit(&mut f.host, f.instances[0].connection(), || 1)
        .unwrap();
    f.instances[2].stop();
    assert!(
        edit.commit(&mut f.host, f.instances[0].connection(), || 1)
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
        output(0)
    );
    f.clean();
}

#[test]
fn actual_eight_edge_chain_completes_and_ninth_edge_is_rejected_with_full_cleanup() {
    let chain_of = |edges: usize| {
        (0..=edges)
            .map(|i| if i < edges { node(&[i + 1]) } else { node(&[]) })
            .collect()
    };
    let limits = GraphLimits {
        max_depth: 8,
        max_total_calls: 64,
    };
    let mut exact = Fixture::new(chain_of(8));
    let value = exact.run(limits, || 1, Cancellation::default()).unwrap();
    assert_eq!(value.dependency_calls(), 8);
    assert_eq!(value.bytes(), output(0));
    value
        .validate(&exact.host, exact.instances[0].connection(), 1)
        .unwrap();
    exact.clean();
    exact.unchanged();
    let mut excess = Fixture::new(chain_of(9));
    assert!(matches!(
        excess.run(limits, || 1, Cancellation::default()),
        Err(Error::Limit)
    ));
    excess.clean();
    excess.unchanged();
}
#[test]
fn actual_sixty_four_calls_complete_and_sixty_fifth_exceeds_shared_budget() {
    let fanout = |extra: bool| {
        // Root invokes eight branches; each invokes the same leaf seven times.
        // 8 + 8*7 = 64 real calls. Only the last branch gains an eighth leaf call.
        // Every guest stays within Context's unchanged per-task limit of eight.
        let mut nodes = vec![node(&(1..=8).collect::<Vec<_>>())];
        for branch in 0..8 {
            nodes.push(node(&vec![9; if extra && branch == 7 { 8 } else { 7 }]));
        }
        nodes.push(node(&[]));
        nodes
    };
    let limits = GraphLimits {
        max_depth: 8,
        max_total_calls: 64,
    };
    let mut exact = Fixture::new(fanout(false));
    let value = exact.run(limits, || 1, Cancellation::default()).unwrap();
    assert_eq!(value.dependency_calls(), 64);
    assert_eq!(value.bytes(), output(0));
    value
        .validate(&exact.host, exact.instances[0].connection(), 1)
        .unwrap();
    exact.clean();
    exact.unchanged();
    let mut excess = Fixture::new(fanout(true));
    assert!(matches!(
        excess.run(limits, || 1, Cancellation::default()),
        Err(Error::Limit)
    ));
    excess.clean();
    excess.unchanged();
}
