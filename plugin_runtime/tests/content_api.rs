use morrow_core::{
    dispatch::HostRuntime,
    lifecycle::GrantKind,
    store::{EventBudget, Store},
};
use morrow_plugin_sdk::protocol::{Action, Failure, Reply, Request};
fn request(id: &str, action: Action) -> Request {
    Request {
        request_id: id.into(),
        card_id: "sdk-card".into(),
        action,
    }
}
fn call(
    host: &mut HostRuntime,
    conn: &morrow_core::dispatch::Connection,
    request: &Request,
) -> Reply {
    request
        .decode_reply(
            &host
                .dispatch(conn, &request.encode().unwrap(), || 3)
                .unwrap(),
        )
        .unwrap()
}
#[test]
fn guest_content_commands_commit_once_preserve_revision_and_respect_authority() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let mut host = HostRuntime::new(Store::open(&path, EventBudget::default()).unwrap()).unwrap();
    let mut conn = host.connect().unwrap();
    let create = request(
        "create",
        Action::CreateContent {
            type_id: "org.test.note".into(),
            format_version: 1,
            title: "title".into(),
            body: vec![0, 255, 42],
        },
    );
    assert_eq!(
        call(&mut host, &conn, &create),
        Reply::Rejected(Failure::Denied)
    );
    for kind in [
        GrantKind::CreateContent,
        GrantKind::EditContent,
        GrantKind::ReadContent,
    ] {
        host.grant(&mut conn, kind, "sdk-card", 20, 1).unwrap();
    }
    let first = call(&mut host, &conn, &create);
    assert!(matches!(first, Reply::ContentCommitted(_)));
    assert_eq!(first, call(&mut host, &conn, &create));
    let read = request(
        "read",
        Action::ReadContent {
            revision: 1,
            offset: 1,
            length: 2,
        },
    );
    let Reply::Content(part) = call(&mut host, &conn, &read) else {
        panic!()
    };
    assert_eq!(part.bytes, [255, 42]);
    assert_eq!(part.total_length, 3);
    let edit = request(
        "edit",
        Action::EditContent {
            revision: 1,
            title: "new".into(),
            body: vec![5; 32768],
            preview: "readable".into(),
        },
    );
    let r = call(&mut host, &conn, &edit);
    assert!(matches!(r, Reply::ContentCommitted(_)));
    assert_eq!(r, call(&mut host, &conn, &edit));
    assert_eq!(
        call(&mut host, &conn, &read),
        Reply::Rejected(Failure::RevisionConflict)
    );
    let read = request(
        "read",
        Action::ReadContent {
            revision: 2,
            offset: 32760,
            length: 32,
        },
    );
    let Reply::Content(part) = call(&mut host, &conn, &read) else {
        panic!()
    };
    assert_eq!(part.bytes.len(), 8);
    let mut count = 0;
    let denied = host
        .dispatch(&conn, &read.encode().unwrap(), || {
            count += 1;
            if count < 3 { 3 } else { 21 }
        })
        .unwrap();
    assert_eq!(
        read.decode_reply(&denied).unwrap(),
        Reply::Rejected(Failure::Denied)
    );
    host.revoke(&mut conn, GrantKind::EditContent, "sdk-card")
        .unwrap();
    assert_eq!(
        call(&mut host, &conn, &edit),
        Reply::Rejected(Failure::Denied)
    );
    drop(host);
    let store = Store::open_existing(&path, EventBudget::default()).unwrap();
    store.integrity_check().unwrap();
    assert_eq!(store.pending(0, 10).unwrap().len(), 2);
    let saved = store.card("sdk-card").unwrap().unwrap();
    assert_eq!(saved.summary().preview_text, "readable");
}
