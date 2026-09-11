#![cfg(not(target_arch = "wasm32"))]
use morrow_core::{
    Error,
    content::{CardRecord, CardSummary},
    dispatch::{Connection, HostRuntime},
    lifecycle::{GrantKind, HostPolicy},
    response::{Failure, Outcome, Response},
    runtime::{Command, RenameRequest},
    store::{EventBudget, Store},
};
fn setup() -> (tempfile::TempDir, HostRuntime, Connection) {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("db"), EventBudget::default()).unwrap();
    store
        .create_local(
            "seed",
            &CardRecord::new(
                "card",
                "private.type",
                1,
                "private title",
                b"never include this body".to_vec(),
            )
            .unwrap(),
        )
        .unwrap();
    let mut host = HostRuntime::new(store).unwrap();
    let connection = host.connect().unwrap();
    (dir, host, connection)
}
fn read(card: &str) -> Vec<u8> {
    Command::ReadSummary {
        request_id: "query".into(),
        card_id: card.into(),
    }
    .encode()
    .unwrap()
}
fn rename() -> Vec<u8> {
    RenameRequest {
        operation_id: "edit".into(),
        card_id: "card".into(),
        expected_revision: 1,
        title: "changed".into(),
    }
    .encode()
    .unwrap()
}
fn run(host: &mut HostRuntime, connection: &Connection, bytes: &[u8], now: u64) -> Response {
    Response::decode(&host.dispatch(connection, bytes, || now).unwrap()).unwrap()
}
#[test]
fn permissions_are_separate_and_denial_hides_existence() {
    let (_dir, mut host, mut connection) = setup();
    host.grant(&mut connection, GrantKind::Rename, "card", 100, 0)
        .unwrap();
    let denial = run(&mut host, &connection, &read("card"), 1);
    assert_eq!(denial.outcome, Outcome::Rejected(Failure::Denied));
    assert_eq!(denial, run(&mut host, &connection, &read("missing"), 1));
    host.grant(&mut connection, GrantKind::ReadSummary, "card", 100, 1)
        .unwrap();
    let response = run(&mut host, &connection, &read("card"), 2);
    let Outcome::Summary(summary) = &response.outcome else {
        panic!("summary required")
    };
    assert_eq!(summary.title, "private title");
    assert_eq!(summary.revision, 1);
    assert!(
        !response
            .encode()
            .unwrap()
            .windows(b"never include this body".len())
            .any(|v| v == b"never include this body")
    );
    host.revoke(&mut connection, GrantKind::ReadSummary, "card")
        .unwrap();
    assert_eq!(run(&mut host, &connection, &read("card"), 3), denial);
    assert_eq!(host.store_local().pending(0, 10).unwrap().len(), 1);
}
#[test]
fn read_grant_does_not_allow_write_and_other_connections_cannot_borrow_it() {
    let (_dir, mut host, mut a) = setup();
    let b = host.connect().unwrap();
    host.grant(&mut a, GrantKind::ReadSummary, "card", 100, 0)
        .unwrap();
    assert_eq!(
        run(&mut host, &a, &rename(), 1).outcome,
        Outcome::Rejected(Failure::Denied)
    );
    assert_eq!(
        run(&mut host, &b, &read("card"), 1).outcome,
        Outcome::Rejected(Failure::Denied)
    );
    let (_other_dir, mut other, _) = setup();
    assert_eq!(
        run(&mut other, &a, &read("card"), 2).outcome,
        Outcome::Rejected(Failure::Denied)
    );
    assert!(
        other
            .grant(&mut a, GrantKind::ReadSummary, "card", 100, 2)
            .is_err()
    );
    assert!(matches!(
        run(&mut host, &a, &read("card"), 2).outcome,
        Outcome::Summary(_)
    ));
}
#[test]
fn disconnected_endpoint_stays_invalid_after_reconnect() {
    let (_dir, mut host, mut old) = setup();
    host.grant(&mut old, GrantKind::ReadSummary, "card", 100, 0)
        .unwrap();
    host.disconnect(&old).unwrap();
    let mut new = host.connect().unwrap();
    host.grant(&mut new, GrantKind::ReadSummary, "card", 100, 1)
        .unwrap();
    assert_eq!(
        run(&mut host, &old, &read("card"), 2).outcome,
        Outcome::Rejected(Failure::Denied)
    );
    assert!(matches!(
        run(&mut host, &new, &read("card"), 2).outcome,
        Outcome::Summary(_)
    ));
}
#[test]
fn expiry_is_rechecked_before_data_delivery() {
    let (_dir, mut host, mut connection) = setup();
    host.grant(&mut connection, GrantKind::ReadSummary, "card", 10, 0)
        .unwrap();
    let mut ticks = [9, 10].into_iter();
    let response = Response::decode(
        &host
            .dispatch(&connection, &read("card"), || ticks.next().unwrap())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(response.outcome, Outcome::Rejected(Failure::Denied));
    assert_eq!(run(&mut host, &connection, &read("card"), 11), response);
}
#[test]
fn read_policy_rejects_write_grants_and_draining_instances() {
    let (_dir, host, _) = setup();
    let mut policy = HostPolicy::new().unwrap();
    let instance = policy.activate().unwrap();
    policy.ready(instance).unwrap();
    let rename = policy.grant_rename(instance, "card", 100, 0).unwrap();
    assert!(
        policy
            .read_summary(instance, rename, host.store_local(), "card", || 1)
            .is_err()
    );
    let read = policy
        .grant(instance, GrantKind::ReadSummary, "card", 100, 1)
        .unwrap();
    policy.drain(instance, 20, 2).unwrap();
    assert!(
        policy
            .read_summary(instance, read, host.store_local(), "card", || 3)
            .is_err()
    );
}
#[test]
fn binary_commit_receipt_deduplicates_and_revocation_blocks_retry() {
    let (_dir, mut host, mut connection) = setup();
    host.grant(&mut connection, GrantKind::Rename, "card", 100, 0)
        .unwrap();
    let first = run(&mut host, &connection, &rename(), 1);
    let Outcome::Renamed(receipt) = &first.outcome else {
        panic!("receipt required")
    };
    assert_eq!(receipt.revision, 2);
    assert_eq!(receipt.operation_id, "edit");
    assert_eq!(run(&mut host, &connection, &rename(), 2), first);
    host.revoke(&mut connection, GrantKind::Rename, "card")
        .unwrap();
    assert_eq!(
        run(&mut host, &connection, &rename(), 3).outcome,
        Outcome::Rejected(Failure::Denied)
    );
    assert_eq!(host.store_local().pending(0, 10).unwrap().len(), 2);
}
#[test]
fn malformed_or_old_protocol_cannot_reach_storage() {
    let (_dir, mut host, mut connection) = setup();
    host.grant(&mut connection, GrantKind::Rename, "card", 100, 0)
        .unwrap();
    assert!(host.dispatch(&connection, &[0, 1, 2], || 1).is_err());
    let mut trailing = rename();
    trailing.extend([0; 8]);
    assert!(host.dispatch(&connection, &trailing, || 1).is_err());
    let mut builder = capnp::message::Builder::new_default();
    builder
        .init_root::<morrow_core::runtime_capnp::request::Builder>()
        .set_protocol_version(2);
    assert_eq!(
        host.dispatch(
            &connection,
            &capnp::serialize::write_message_to_words(&builder),
            || 1
        ),
        Err(Error::UnsupportedVersion)
    );
    assert_eq!(
        host.store_local()
            .card("card")
            .unwrap()
            .unwrap()
            .summary()
            .revision,
        1
    );
}
#[test]
fn response_roundtrip_is_bounded_and_keeps_full_uint64() {
    let response = Response {
        request_id: "query".into(),
        outcome: Outcome::Summary(CardSummary {
            id: "card".into(),
            type_id: "type".into(),
            format_version: 1,
            revision: u64::MAX,
            title: "中文 🧭".into(),
            preview_text: "preview".into(),
        }),
    };
    let bytes = response.encode().unwrap();
    assert_eq!(Response::decode(&bytes).unwrap(), response);
    let mut trailing = bytes.clone();
    trailing.extend([0; 8]);
    assert!(Response::decode(&trailing).is_err());
    assert!(Response::decode(&vec![0; 65537]).is_err());
    let mut changed = response;
    if let Outcome::Summary(v) = &mut changed.outcome {
        v.preview_text = "x".repeat(16385);
    }
    assert_eq!(changed.encode(), Err(Error::Limit));
}
