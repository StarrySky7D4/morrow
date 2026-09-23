use capnp::{message::Builder, serialize};
use morrow_core::{
    content::CardRecord,
    dispatch::HostRuntime,
    plugin_package::{Package, catalog::Catalog, proto::TransformHandler, registry::Registry},
    store::Store,
    task::{Invocation, Transform},
};
use morrow_plugin_runtime::{Limits, instance_pool::Pool, manager::Manager, replay};
use morrow_workbench_plugin::{
    Idea, persistence, tasks_capnp as wire, tasks_v2::Baseline, tasks_v2_codec,
};
use std::collections::BTreeSet;

fn request(action: wire::Action, properties: &[u8], revision: u64) -> Vec<u8> {
    let mut message = Builder::new_default();
    {
        let mut r = message.init_root::<wire::request::Builder>();
        r.set_version(2);
        r.set_digest(&tasks_v2_codec::digest());
        r.set_action(action);
        r.set_card_id("card");
        r.set_title("Original");
        r.set_revision(revision);
        r.set_base_revision(1);
        r.set_source_digest(&Baseline::capture("card", 1, properties).unwrap().sha256);
        r.set_properties(properties);
        r.set_text("推进中");
        if action != wire::Action::Migrate {
            let p =
                morrow_workbench_plugin::tasks_v2::decode("card", "Original", properties).unwrap();
            r.set_task_id(p.tasks[0].id.as_str());
            r.set_complete(true);
            if action == wire::Action::Add {
                r.set_task_id("owner-generated-fixed-id");
                r.set_text("new");
            }
            if action == wire::Action::Reorder {
                let mut order = r.reborrow().init_order(p.tasks.len() as u32);
                for (index, task) in p.tasks.iter().rev().enumerate() {
                    order.set(index as u32, task.id.as_str());
                }
            }
        }
    }
    serialize::write_message_to_words(&message)
}
#[test]
fn actual_rust_guest_migration_stage_and_replay_leave_source_untouched() {
    guest_scenario(2);
    guest_scenario(128);
}

fn guest_scenario(count: usize) {
    let module =
        std::fs::read(std::env::var("MORROW_WORKBENCH_WASM").expect("compiled guest path"))
            .unwrap();
    let manifest = Package::manifest_for_transform(
        "test.tasks-v2",
        morrow_workbench_plugin::PACKAGE_VERSION,
        &module,
        vec![TransformHandler {
            handler: "workbench.tasks.v2".into(),
            input_type: "morrow.workbench.tasks.request.v2".into(),
            output_type: "morrow.workbench.tasks.response.v2".into(),
            max_input_bytes: 65536,
            max_output_bytes: 65536,
        }],
    );
    let package = Package::build(manifest, &module).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let catalog = Catalog::open(&dir.path().join("catalog")).unwrap();
    catalog.install(&package).unwrap();
    let limits = Limits::default();
    let mut manager = Manager::new(
        Registry::open(&dir.path().join("registry"), catalog).unwrap(),
        limits,
    );
    manager.select(&package, manager.revision()).unwrap();
    manager
        .approve(
            "test.tasks-v2",
            package.digest(),
            BTreeSet::new(),
            manager.revision(),
        )
        .unwrap();
    manager
        .set_enabled("test.tasks-v2", package.digest(), true, manager.revision())
        .unwrap();
    let old = persistence::encode(
        &Idea {
            id: "card".into(),
            title: "Original".into(),
            category: "进行中".into(),
            stage: "计划中".into(),
            todos: vec!["same".into(); count],
            completed: vec!["same".into()],
            ..Default::default()
        },
        None,
    )
    .unwrap();
    let source = CardRecord::new("card", "org.morrow.idea", 1, "Original", old.clone()).unwrap();
    let mut store = Store::open(&dir.path().join("store"), Default::default()).unwrap();
    store.create_local("seed", &source).unwrap();
    let mut host = HostRuntime::new(store).unwrap();
    let mut pool = Pool::new(&host, Default::default()).unwrap();
    let revision = manager.revision();
    let session = pool
        .start(&mut manager, &mut host, "test.tasks-v2", &[], revision)
        .unwrap();
    let mut current = old.clone();
    for (index, action) in [
        wire::Action::Migrate,
        wire::Action::SetStage,
        wire::Action::Reorder,
        wire::Action::Rename,
        wire::Action::SetCompletion,
        wire::Action::Remove,
        wire::Action::Add,
        wire::Action::CompleteAllAndSetStage,
        wire::Action::Project,
    ]
    .into_iter()
    .enumerate()
    {
        let input = request(action, &current, 1);
        let expected = tasks_v2_codec::process(&input).unwrap();
        let invocation = Invocation::new_transform(
            &format!("task-{index}"),
            Transform {
                handler: "workbench.tasks.v2".into(),
                input_type: "morrow.workbench.tasks.request.v2".into(),
                output_type: "morrow.workbench.tasks.response.v2".into(),
                input,
            },
        )
        .unwrap();
        let capture = pool
            .record_transform(&manager, &mut host, &session, &invocation)
            .unwrap();
        assert_eq!(
            capture.report().execution.outcome,
            Ok(0),
            "{count} tasks, {action:?}"
        );
        eprintln!(
            "{count} tasks, {action:?}: fuel remaining {:?}",
            capture.report().execution.fuel_remaining
        );
        assert_eq!(capture.report().execution.host_calls, 0);
        let output = &capture
            .report()
            .output
            .as_ref()
            .expect("successful guest output")
            .bytes;
        assert_eq!(output, &expected);
        let message =
            serialize::read_message_from_flat_slice(&mut output.as_slice(), Default::default())
                .unwrap();
        let response = message.get_root::<wire::response::Reader>().unwrap();
        assert_eq!(
            (
                response.get_complete(),
                response.get_incomplete(),
                response.get_ambiguous()
            ),
            match action {
                wire::Action::SetCompletion => (1, 0, count as u32 - 1),
                wire::Action::Remove => (0, 0, count as u32 - 1),
                wire::Action::Add => (0, 1, count as u32 - 1),
                wire::Action::CompleteAllAndSetStage | wire::Action::Project =>
                    (count as u32, 0, 0),
                _ => (0, 0, count as u32),
            }
        );
        assert_eq!(
            response.get_stage().unwrap().to_str().unwrap(),
            if index == 0 { "计划中" } else { "推进中" }
        );
        current = response.get_properties().unwrap().to_vec();
        assert!(replay::replay(capture.evidence(), limits).unwrap().matches);
        assert_eq!(
            host.store_local().card("card").unwrap().unwrap().encode(),
            source.encode()
        );
    }
    let invocation = Invocation::new_transform(
        "stale-source",
        Transform {
            handler: "workbench.tasks.v2".into(),
            input_type: "morrow.workbench.tasks.request.v2".into(),
            output_type: "morrow.workbench.tasks.response.v2".into(),
            input: request(wire::Action::Migrate, &old, 2),
        },
    )
    .unwrap();
    let capture = pool
        .record_transform(&manager, &mut host, &session, &invocation)
        .unwrap();
    assert!(capture.report().failure.is_some());
    assert!(capture.report().output.is_none());
    assert_eq!(
        host.store_local().card("card").unwrap().unwrap().encode(),
        source.encode()
    );
    pool.close_all(&mut host).unwrap();
}
