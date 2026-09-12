//! Actual Rust guest + scoped host proposal + single-database persistence proof.
use morrow_core::{
    content::CardRecord,
    content_change::ContentChange,
    dispatch::HostRuntime,
    lifecycle::GrantKind,
    plugin_package::{
        Package,
        proto::{Capability, TransformHandler},
    },
    store::{EventBudget, Store},
    task::{Invocation, Transform},
};
use morrow_plugin_runtime::{Limits, package::PreparedPackage};
use morrow_workbench_plugin::{Action, Idea, Request, Response, codec, persistence};
fn request(action: Action, current: Idea, proposed: Idea) -> Request {
    Request {
        action,
        current,
        proposed,
        text: String::new(),
        flag: false,
        now_ms: 1,
        ideas: vec![],
        section: "概览".into(),
        filter: "全部".into(),
        sort: "最近添加".into(),
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let module = std::fs::read(std::env::args().nth(1).ok_or("module")?)?;
    let mut manifest = Package::manifest_for_transform(
        "org.morrow.workbench",
        "0.1.9-test.10",
        &module,
        vec![TransformHandler {
            handler: "workbench.command".into(),
            input_type: "morrow.workbench.request.v1".into(),
            output_type: "morrow.workbench.response.v1".into(),
            max_input_bytes: 65536,
            max_output_bytes: 65536,
        }],
    );
    manifest.requested_capabilities = vec![
        Capability::CreateContent as i32,
        Capability::EditContent as i32,
        Capability::ReadContent as i32,
    ];
    let prepared = PreparedPackage::new(Package::build(manifest, &module)?, Limits::default())
        .map_err(|e| format!("prepare: {e:?}"))?;
    let temporary = tempfile::tempdir()?;
    let path = temporary.path().join("db");
    let mut host = HostRuntime::new(Store::open(&path, EventBudget::default())?)?;
    let mut connection = prepared.connect(&mut host)?;
    for kind in [
        GrantKind::CreateContent,
        GrantKind::EditContent,
        GrantKind::ReadContent,
    ] {
        host.grant(&mut connection, kind, "idea", 10000, 0)?;
    }
    let mut count = 0;
    let mut run =
        |host: &mut HostRuntime, input: Request| -> Result<Response, Box<dyn std::error::Error>> {
            count += 1;
            let input = codec::encode_request(&input)?;
            let task = Invocation::new_transform(
                &format!("task-{count}"),
                Transform {
                    handler: "workbench.command".into(),
                    input_type: "morrow.workbench.request.v1".into(),
                    output_type: "morrow.workbench.response.v1".into(),
                    input,
                },
            )?;
            let report = prepared.run_task(host, &connection, &task, || 1, Default::default());
            if report.execution.outcome != Ok(0)
                || report.execution.host_calls != 0
                || report.failure.is_some()
            {
                return Err(format!(
                    "run failed: {:?} {:?}",
                    report.execution.outcome, report.failure
                )
                .into());
            }
            Ok(codec::decode_response(
                &report.output.ok_or("output")?.bytes,
            )?)
        };
    let draft = Idea {
        id: "idea".into(),
        title: "完整工作台".into(),
        description: "正文".into(),
        category: "灵感".into(),
        todos: vec!["阅读".into(), "记录".into()],
        ..Default::default()
    };
    let created = run(&mut host, request(Action::Create, Idea::default(), draft))?.idea;
    let card = CardRecord::new(
        "idea",
        "org.morrow.idea",
        1,
        &created.title,
        persistence::encode(&created, None)?,
    )?;
    host.create_content(&connection, "create", &card, || 1)?;
    let mut state = created;
    let commit = |host: &mut HostRuntime,
                  idea: &Idea,
                  operation: &str|
     -> Result<(), Box<dyn std::error::Error>> {
        let prior = host.read_content(&connection, "idea", || 1)?;
        let change = ContentChange {
            operation_id: operation.into(),
            card_id: "idea".into(),
            expected_revision: prior.summary().revision,
            title: idea.title.clone(),
            body: persistence::encode(idea, Some(&prior.body()))?,
            preview_text: idea.description.clone(),
            attachments: None,
        };
        host.edit_content(&connection, &change, || 1)?;
        Ok(())
    };
    let mut favorite = request(Action::Favorite, state, Idea::default());
    favorite.flag = true;
    state = run(&mut host, favorite)?.idea;
    commit(&mut host, &state, "favorite")?;
    state = run(
        &mut host,
        request(Action::ToProject, state, Idea::default()),
    )?
    .idea;
    assert_eq!(state.stage, "计划中");
    commit(&mut host, &state, "project")?;
    let mut stage = request(Action::Stage, state, Idea::default());
    stage.text = "已完成".into();
    state = run(&mut host, stage)?.idea;
    assert_eq!(state.completed.len(), 2);
    commit(&mut host, &state, "stage")?;
    let mut edited = state.clone();
    edited.title = "保留收藏与勾选".into();
    edited.todos = vec!["记录".into(), "下一步".into()];
    edited.favorite = false;
    edited.completed.clear();
    state = run(&mut host, request(Action::Edit, state, edited))?.idea;
    assert!(state.favorite);
    assert_eq!(state.completed, vec!["记录"]);
    commit(&mut host, &state, "edit")?;
    let mut query = request(Action::Query, Idea::default(), Idea::default());
    query.ideas = vec![state.clone()];
    query.text = "正文".into();
    assert_eq!(run(&mut host, query)?.ids, vec!["idea"]);
    let mut deletion = request(Action::Delete, state, Idea::default());
    deletion.now_ms = 100;
    state = run(&mut host, deletion)?.idea;
    assert!(state.deleted);
    commit(&mut host, &state, "delete")?;
    let mut undo = request(Action::Restore, state, Idea::default());
    undo.now_ms = 8099;
    state = run(&mut host, undo)?.idea;
    assert!(!state.deleted);
    commit(&mut host, &state, "undo")?;
    host.disconnect(&connection)?;
    drop(host);
    let store = Store::open_existing(&path, EventBudget::default())?;
    store.integrity_check()?;
    let restored = store.card("idea")?.ok_or("card")?;
    let decoded = persistence::decode("idea", &restored.summary().title, &restored.body())?;
    assert_eq!(decoded, state);
    assert_eq!(restored.summary().revision, 7);
    assert_eq!(store.pending(0, 128)?.len(), 7);
    println!(
        "PASS: actual Rust Wasm workbench; create, favorite, project, checklist stage, edit preservation, query, delete/undo; 7 atomic commits and reopen parity"
    );
    Ok(())
}
