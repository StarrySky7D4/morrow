#![cfg(all(feature = "packages", not(target_arch = "wasm32")))]
#![deny(unsafe_code)]
//! Frozen compiled SDK compatibility; no guest builds or repackaging.
//! Core-contract UI events do not establish Flutter rendering compatibility.
#[path = "support/frozen_sdk.rs"]
mod frozen_sdk;
use morrow_core::{
    content::{Attachment, CardRecord},
    content_change::ContentChange,
    dispatch::{Connection, HostRuntime},
    lifecycle::GrantKind,
    response::{Failure, Outcome, Response},
    runtime::{Command, CreateContent, ReadAttachment, ReadContent, RenameRequest},
    store::Store,
    task::{FailureCode, Invocation, MAX_VALUE_BYTES, Transform},
    transaction::Lookup,
    ui::{Document, Event, EventKind, Kind, Node, Session, Tone},
};
use morrow_plugin_runtime::{
    Cancellation, Fault, Limits,
    package::{PreparedPackage, TaskReport},
};
fn prepared(language: &str, kind: &str) -> PreparedPackage {
    PreparedPackage::new(
        frozen_sdk::package(&format!("{language}-{kind}")),
        Limits::default(),
    )
    .unwrap()
}
fn invoke(p: &PreparedPackage, h: &mut HostRuntime, c: &Connection, command: &Command) -> Response {
    let input = Invocation::new("frozen-content", command).unwrap();
    let r = p.run_task(h, c, &input, || 1, Cancellation::default());
    assert_eq!(r.execution.outcome, Ok(0));
    assert_eq!(r.execution.host_calls, 1);
    assert!(r.output.is_none() && r.failure.is_none());
    let response = r.response.expect("completion tied to actual core response");
    assert_eq!(response.request_id, command.request_id());
    response
}
fn task(language: &str) {
    let p = prepared(language, "task");
    assert_eq!(p.package().capabilities().len(), 7);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let mut store = Store::open(&path, Default::default()).unwrap();
    let payload = b"frozen SDK attachment: \0\xff / bytes";
    let blob = store
        .stage_blob(
            &mut std::io::Cursor::new(payload),
            payload.len() as u64,
            None,
            0,
        )
        .unwrap();
    let attachment = Attachment {
        id: "file".into(),
        display_name: "附件🌈".into(),
        media_type: "application/octet-stream".into(),
        byte_length: blob.byte_length,
        sha256: blob.sha256,
    };
    store
        .create_local(
            "seed",
            &CardRecord::new_with_attachments(
                "attached",
                "note",
                1,
                "before",
                vec![],
                &[attachment],
            )
            .unwrap(),
        )
        .unwrap();
    let mut host = HostRuntime::new(store).unwrap();
    let mut c = p.connect(&mut host).unwrap();
    for kind in [
        GrantKind::CreateContent,
        GrantKind::EditContent,
        GrantKind::ReadContent,
        GrantKind::Rename,
        GrantKind::ReadSummary,
        GrantKind::QueryOperation,
    ] {
        host.grant(&mut c, kind, "card", 100, 0).unwrap();
    }
    host.grant_attachment(&mut c, "attached", "file", 100, 0)
        .unwrap();
    let create = Command::CreateContent(CreateContent {
        operation_id: "create".into(),
        card_id: "card".into(),
        type_id: "org.test.note".into(),
        format_version: 1,
        title: "创建🌈".into(),
        body: vec![0, 255, 42],
    });
    let created = invoke(&p, &mut host, &c, &create);
    assert!(
        matches!(&created.outcome,Outcome::ContentCommitted(r) if r.revision==1 && r.card_id=="card")
    );
    assert_eq!(
        host.store_local().card("card").unwrap().unwrap().body(),
        vec![0, 255, 42]
    );
    assert_eq!(invoke(&p, &mut host, &c, &create), created);
    let edit = Command::EditContent(ContentChange {
        operation_id: "edit".into(),
        card_id: "card".into(),
        expected_revision: 1,
        title: "编辑世界".into(),
        body: vec![42; 32768],
        preview_text: "exact preview".into(),
        attachments: None,
    });
    let edited = invoke(&p, &mut host, &c, &edit);
    assert!(matches!(&edited.outcome,Outcome::ContentCommitted(r) if r.revision==2));
    assert_eq!(invoke(&p, &mut host, &c, &edit), edited);
    let rename = Command::Rename(RenameRequest {
        operation_id: "rename".into(),
        card_id: "card".into(),
        expected_revision: 2,
        title: "重命名🌈".into(),
    });
    let renamed = invoke(&p, &mut host, &c, &rename);
    assert!(matches!(&renamed.outcome,Outcome::Renamed(r) if r.revision==3));
    assert_eq!(invoke(&p, &mut host, &c, &rename), renamed);
    assert_eq!(
        invoke(&p, &mut host, &c, &create),
        created,
        "historical retry keeps original revision"
    );
    let read = Command::ReadContent(ReadContent {
        request_id: "body".into(),
        card_id: "card".into(),
        expected_revision: 3,
        offset: 32760,
        length: 32,
    });
    assert!(
        matches!(invoke(&p,&mut host,&c,&read).outcome,Outcome::ContentChunk(r) if r.bytes==vec![42;8] && r.revision==3 && r.offset==32760 && r.total_length==32768)
    );
    let summary = Command::ReadSummary {
        request_id: "summary".into(),
        card_id: "card".into(),
    };
    assert!(
        matches!(invoke(&p,&mut host,&c,&summary).outcome,Outcome::Summary(r) if r.title=="重命名🌈" && r.preview_text=="exact preview" && r.revision==3)
    );
    let query = Command::QueryOperation {
        request_id: "query".into(),
        card_id: "card".into(),
        operation_id: "create".into(),
    };
    assert!(
        matches!(invoke(&p,&mut host,&c,&query).outcome,Outcome::OperationResult {result:Lookup::Committed(r),..} if r.revision==1)
    );
    let missing = Command::QueryOperation {
        request_id: "query-missing".into(),
        card_id: "card".into(),
        operation_id: "absent".into(),
    };
    assert!(matches!(
        invoke(&p, &mut host, &c, &missing).outcome,
        Outcome::OperationResult {
            result: Lookup::Absent,
            ..
        }
    ));
    let attachment = Command::ReadAttachment(ReadAttachment {
        request_id: "attachment".into(),
        card_id: "attached".into(),
        attachment_id: "file".into(),
        expected_revision: 1,
        offset: 8,
        length: 64,
    });
    assert!(
        matches!(invoke(&p,&mut host,&c,&attachment).outcome,Outcome::AttachmentChunk(r) if r.bytes==payload[8..] && r.revision==1)
    );
    let committed = host.store_local().pending(0, 100).unwrap().len();
    assert_eq!(committed, 4, "seed/create/edit/rename only");
    for (kind, command) in [
        (GrantKind::CreateContent, &create),
        (GrantKind::EditContent, &edit),
        (GrantKind::ReadContent, &read),
        (GrantKind::Rename, &rename),
        (GrantKind::ReadSummary, &summary),
        (GrantKind::QueryOperation, &query),
    ] {
        host.revoke(&mut c, kind, "card").unwrap();
        assert_eq!(
            invoke(&p, &mut host, &c, command).outcome,
            Outcome::Rejected(Failure::Denied),
            "{language} {kind:?}"
        );
    }
    host.revoke_attachment(&mut c, "attached", "file").unwrap();
    assert_eq!(
        invoke(&p, &mut host, &c, &attachment).outcome,
        Outcome::Rejected(Failure::Denied)
    );
    host.grant(&mut c, GrantKind::Rename, "card", 100, 1)
        .unwrap();
    let cancel_command = Command::Rename(RenameRequest {
        operation_id: "cancelled-before".into(),
        card_id: "card".into(),
        expected_revision: 3,
        title: "must not execute".into(),
    });
    let input = Invocation::new("cancel-before", &cancel_command).unwrap();
    let cancel = Cancellation::default();
    cancel.cancel();
    let r = p.run_task(
        &mut host,
        &c,
        &input,
        || panic!("pre-cancel reached core"),
        cancel,
    );
    assert_eq!(r.execution.outcome, Err(Fault::Cancelled));
    assert_eq!(r.execution.host_calls, 0);
    assert!(r.response.is_none());
    assert_eq!(
        host.store_local()
            .lookup_for_card("card", "cancelled-before")
            .unwrap(),
        Lookup::Absent
    );
    // A cancellation raised inside an actual core call is not a transaction rollback.
    let command = Command::Rename(RenameRequest {
        operation_id: "cancelled-during".into(),
        card_id: "card".into(),
        expected_revision: 3,
        title: "committed before cancellation observed".into(),
    });
    let input = Invocation::new("cancel-during", &command).unwrap();
    let cancel = Cancellation::default();
    let signal = cancel.clone();
    let mut clocks = 0;
    let r = p.run_task(
        &mut host,
        &c,
        &input,
        || {
            clocks += 1;
            signal.cancel();
            2
        },
        cancel,
    );
    assert!(clocks > 0);
    assert_eq!(r.execution.outcome, Err(Fault::Cancelled));
    assert_eq!(r.execution.host_calls, 1);
    assert!(r.response.is_none());
    assert!(
        matches!(host.store_local().lookup_for_card("card","cancelled-during").unwrap(),Lookup::Committed(r) if r.revision==4)
    );
    assert_eq!(
        host.store_local().pending(0, 100).unwrap().len(),
        committed + 1
    );
    host.store_local().integrity_check().unwrap();
    drop(host);
    let reopened = Store::open_existing(&path, Default::default()).unwrap();
    assert_eq!(
        reopened.card("card").unwrap().unwrap().summary().revision,
        4
    );
    assert_eq!(
        reopened
            .card("attached")
            .unwrap()
            .unwrap()
            .summary()
            .revision,
        1
    );
}
#[allow(clippy::too_many_arguments)]
fn pure(
    p: &PreparedPackage,
    h: &mut HostRuntime,
    c: &Connection,
    handler: &str,
    input_type: &str,
    output_type: &str,
    input: Vec<u8>,
) -> TaskReport {
    let task = Invocation::new_transform(
        "frozen-pure",
        Transform {
            handler: handler.into(),
            input_type: input_type.into(),
            output_type: output_type.into(),
            input,
        },
    )
    .unwrap();
    let r = p.run_task(
        h,
        c,
        &task,
        || panic!("pure guest called core"),
        Cancellation::default(),
    );
    assert_eq!(r.execution.host_calls, 0);
    assert!(r.response.is_none());
    r
}
fn transform(language: &str) {
    let p = prepared(language, "transform");
    assert!(p.package().capabilities().is_empty());
    let dir = tempfile::tempdir().unwrap();
    let mut host =
        HostRuntime::new(Store::open(&dir.path().join("db"), Default::default()).unwrap()).unwrap();
    let c = p.connect(&mut host).unwrap();
    let samples = [
        vec![],
        b"Abc 123".to_vec(),
        "hello 世界🌈".as_bytes().to_vec(),
        (0..=255).collect(),
        (0..MAX_VALUE_BYTES).map(|n| (n % 256) as u8).collect(),
    ];
    for handler in [
        "bytes.reverse",
        "bytes.ascii-uppercase",
        "bytes.require-ascii",
    ] {
        for input in &samples {
            let r = pure(&p, &mut host, &c, handler, "bytes", "bytes", input.clone());
            assert_eq!(r.execution.outcome, Ok(0));
            if handler == "bytes.require-ascii" && !input.is_ascii() {
                assert!(r.output.is_none());
                let failure = r.failure.unwrap();
                assert_eq!(failure.code, FailureCode::UnsupportedInput);
                assert_eq!(failure.message, "Input contains non-ASCII bytes");
            } else {
                assert!(r.failure.is_none());
                let output = r.output.unwrap();
                let mut expected = input.clone();
                if handler == "bytes.reverse" {
                    expected.reverse();
                }
                if handler == "bytes.ascii-uppercase" {
                    expected.make_ascii_uppercase();
                }
                assert_eq!(output.type_id, "bytes");
                assert_eq!(output.bytes, expected);
            }
        }
    }
    for (handler, it, ot) in [
        ("absent", "bytes", "bytes"),
        ("bytes.reverse", "wrong", "bytes"),
        ("bytes.reverse", "bytes", "wrong"),
    ] {
        let r = pure(&p, &mut host, &c, handler, it, ot, vec![1]);
        assert_eq!(r.execution.outcome, Err(Fault::TaskProtocol));
        assert_eq!(r.execution.fuel_remaining, p.limits().fuel);
        assert!(r.output.is_none() && r.failure.is_none());
    }
    assert!(host.store_local().pending(0, 100).unwrap().is_empty());
    host.store_local().integrity_check().unwrap();
}
fn expected_form(title: &str) -> Document {
    let mut nodes = vec![
        Node::new("root", "", Kind::Column),
        Node::new("heading", "root", Kind::Text),
        Node::new("title", "root", Kind::TextInput),
        Node::new("pinned", "root", Kind::Toggle),
        Node::new("apply", "root", Kind::Button),
    ];
    nodes[1].text = "插件表单".into();
    nodes[1].tone = Tone::Emphasis;
    nodes[2].label = "标题".into();
    nodes[2].text = title.into();
    nodes[2].action = "title.edit".into();
    nodes[2].max_bytes = 32;
    nodes[3].label = "置顶".into();
    nodes[3].action = "pin.toggle".into();
    nodes[4].label = "应用".into();
    nodes[4].action = "apply".into();
    Document::new(nodes).unwrap()
}
fn ui(language: &str) {
    let p = prepared(language, "ui");
    assert!(p.package().capabilities().is_empty());
    let dir = tempfile::tempdir().unwrap();
    let mut host =
        HostRuntime::new(Store::open(&dir.path().join("db"), Default::default()).unwrap()).unwrap();
    let c = p.connect(&mut host).unwrap();
    let mut session = Session::new("view", 7).unwrap();
    for title in ["灵感🌈", "", "abcdefghabcdefghabcdefghabcdefgh"] {
        let r = pure(
            &p,
            &mut host,
            &c,
            "ui.form",
            "text.utf8",
            "morrow.ui.document.v1",
            title.as_bytes().to_vec(),
        );
        assert_eq!(r.execution.outcome, Ok(0));
        assert!(r.failure.is_none());
        let output = r.output.unwrap();
        assert_eq!(output.type_id, "morrow.ui.document.v1");
        let doc = Document::decode(&output.bytes).unwrap();
        assert_eq!(doc, expected_form(title));
        if session.revision() == 0 {
            session.replace(0, doc).unwrap();
        }
    }
    let event = Event {
        view: "view".into(),
        generation: 7,
        revision: 1,
        serial: 1,
        node: "title".into(),
        action: "title.edit".into(),
        kind: EventKind::EditText,
        text: "实际事件🌈".into(),
        checked: false,
    }
    .encode()
    .unwrap();
    let accepted = session.accept(&event).unwrap();
    assert!(session.accept(&event).is_err());
    let r = pure(
        &p,
        &mut host,
        &c,
        "ui.edit",
        "morrow.ui.event.v1",
        "morrow.ui.document.v1",
        event.clone(),
    );
    assert_eq!(r.execution.outcome, Ok(0));
    assert!(r.failure.is_none());
    let doc = Document::decode(&r.output.unwrap().bytes).unwrap();
    assert_eq!(doc, expected_form(&accepted.text));
    session.replace(1, doc).unwrap();
    assert!(session.accept(&event).is_err());
    for (handler, it, input) in [
        ("ui.form", "text.utf8", vec![255]),
        ("ui.form", "text.utf8", vec![0]),
        ("ui.edit", "morrow.ui.event.v1", vec![0]),
    ] {
        let r = pure(
            &p,
            &mut host,
            &c,
            handler,
            it,
            "morrow.ui.document.v1",
            input,
        );
        assert_eq!(r.execution.outcome, Ok(0));
        assert!(r.output.is_none());
        let failure = r.failure.unwrap();
        assert_eq!(failure.code, FailureCode::InvalidInput);
        assert_eq!(failure.message, "Invalid form input");
    }
    session.close();
    assert!(session.accept(&event).is_err());
    assert!(host.store_local().pending(0, 100).unwrap().is_empty());
    host.store_local().integrity_check().unwrap();
}
macro_rules! cases {
    ($task:ident,$transform:ident,$ui:ident,$language:literal) => {
        #[test]
        fn $task() {
            task($language);
        }
        #[test]
        fn $transform() {
            transform($language);
        }
        #[test]
        fn $ui() {
            ui($language);
        }
    };
}
cases!(rust_task, rust_transform, rust_ui, "rust");
cases!(c_task, c_transform, c_ui, "c");
cases!(cpp_task, cpp_transform, cpp_ui, "cpp");
