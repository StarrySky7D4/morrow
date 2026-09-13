#![cfg(target_os = "windows")]
mod common;
use morrow_audit::{
    session::{OpenMode, Session},
    snapshot, verify,
};
use morrow_core::{
    content::CardRecord,
    lifecycle::GrantKind,
    plugin_package::{catalog::Catalog, registry::Registry},
    read_journal::Input,
    task::{Invocation, Transform},
};
use morrow_plugin_runtime::{Limits, instance_pool::Pool, manager::Manager, replay};
use morrow_workbench_plugin::{Action, Idea, Request, codec, persistence};
use std::collections::BTreeSet;
#[test]
fn actual_rust_query_observation_survives_signing_snapshot_and_offline_replay() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    std::fs::create_dir(&source).unwrap();
    let mut session = Session::open(
        &source.join("workbench.db"),
        Default::default(),
        OpenMode::Initialize,
    )
    .unwrap();
    for (id, title) in [("a", "Zebra"), ("b", "Apple")] {
        let mut idea = common::idea(id);
        idea.title = title.into();
        session
            .runtime()
            .store_local_mut()
            .create_local(
                &format!("seed-{id}"),
                &CardRecord::new(
                    id,
                    "org.morrow.idea",
                    1,
                    title,
                    persistence::encode(&idea, None).unwrap(),
                )
                .unwrap(),
            )
            .unwrap();
    }
    let before = ["a", "b"].map(|id| session.store().card(id).unwrap().unwrap().encode());
    let package = common::package();
    let catalog = Catalog::open(&source.join("packages")).unwrap();
    catalog.install(&package).unwrap();
    let mut manager = Manager::new(
        Registry::open(&source.join("registry"), catalog).unwrap(),
        Limits::default(),
    );
    let id = &package.manifest().package_id;
    manager.select(&package, manager.revision()).unwrap();
    manager
        .approve(
            id,
            package.digest(),
            BTreeSet::from([GrantKind::ReadContent]),
            manager.revision(),
        )
        .unwrap();
    manager
        .set_enabled(id, package.digest(), true, manager.revision())
        .unwrap();
    let mut pool = Pool::new(session.runtime_ref(), Default::default()).unwrap();
    let revision = manager.revision();
    let instance = pool
        .start(&mut manager, session.runtime(), id, &[], revision)
        .unwrap();
    // Explicit trusted local input collection for this SDK qualification. This is not the
    // default Workbench::query enumeration, authorization proof, or a complete query manifest.
    let ideas = ["a", "b"]
        .iter()
        .map(|id| {
            let card = session.store().card(id).unwrap().unwrap();
            persistence::decode(id, &card.summary().title, &card.body()).unwrap()
        })
        .collect();
    let request = codec::encode_request(&Request {
        action: Action::Query,
        current: Idea::default(),
        proposed: Idea::default(),
        text: String::new(),
        flag: false,
        now_ms: 0,
        ideas,
        section: "概览".into(),
        filter: "全部".into(),
        sort: "标题排序".into(),
    })
    .unwrap();
    let invocation = Invocation::new_transform(
        "actual-read-task",
        Transform {
            handler: "workbench.command".into(),
            input_type: "morrow.workbench.request.v1".into(),
            output_type: "morrow.workbench.response.v1".into(),
            input: request.clone(),
        },
    )
    .unwrap();
    let captured = pool
        .record_transform(&manager, session.runtime(), &instance, &invocation)
        .unwrap();
    let (report, evidence) = captured.into_parts();
    let response = report.output.unwrap().bytes;
    assert_eq!(codec::decode_response(&response).unwrap().ids, ["b", "a"]);
    let input = Input {
        operation_id: "observed-query".into(),
        subject: "qualification.query".into(),
        request_type: "morrow.workbench.request.v1".into(),
        request,
        response_type: "morrow.workbench.response.v1".into(),
        response,
    };
    session
        .runtime()
        .store_local_mut()
        .record_read_local_authorized(&input, std::slice::from_ref(&evidence), || Ok(()))
        .unwrap();
    let observation = session
        .store()
        .lookup_read(&input.subject, &input.operation_id)
        .unwrap()
        .unwrap();
    let original = observation.container().to_vec();
    let original_evidence = evidence.container().to_vec();
    pool.close_all(session.runtime()).unwrap();
    session.flush(16).unwrap();
    let trust = session.trust();
    let signed = session.store().sealed_segment(1).unwrap().unwrap();
    let checked = verify(&signed, &trust).unwrap();
    assert_eq!(
        checked.segment().events.last().unwrap().original_commit,
        original
    );
    for (index, id) in ["a", "b"].iter().enumerate() {
        assert_eq!(
            session.store().card(id).unwrap().unwrap().encode(),
            before[index]
        );
    }
    let backup = dir.path().join("query.morrowbackup");
    session.backup_snapshot(&backup).unwrap();
    drop(pool);
    drop(manager);
    drop(session);
    assert!(source.starts_with(dir.path()) && source.file_name().unwrap() == "source");
    std::fs::remove_dir_all(&source).unwrap();
    let restored = dir.path().join("restored");
    snapshot::restore(&backup, &restored).unwrap();
    let session = Session::open(
        &restored.join("workbench.db"),
        Default::default(),
        OpenMode::Existing,
    )
    .unwrap();
    session.store().integrity_check().unwrap();
    assert_eq!(session.store().sealed_segment(1).unwrap().unwrap(), signed);
    assert_eq!(
        session
            .store()
            .lookup_read(&input.subject, &input.operation_id)
            .unwrap()
            .unwrap()
            .container(),
        original
    );
    let stored = session
        .store()
        .read_evidence(&input.subject, &input.operation_id)
        .unwrap()
        .unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].container(), original_evidence);
    assert_eq!(stored[0].data().invocation, invocation.bytes());
    assert!(
        replay::replay(&stored[0], Limits::default())
            .unwrap()
            .matches
    );
}
