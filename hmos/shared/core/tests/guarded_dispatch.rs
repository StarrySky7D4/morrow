#![cfg(not(target_arch = "wasm32"))]
use morrow_core::{
    Error,
    content::{Attachment, CardRecord},
    content_change::ContentChange,
    dispatch::{Connection, HostRuntime},
    lifecycle::{ContentAuthorization, GrantKind},
    response::{Failure, Outcome, Response},
    runtime::{Command, CreateContent, ReadAttachment, ReadContent, RenameRequest},
    store::{EventBudget, Store},
    transaction::Lookup,
};
use std::io::Cursor;

fn setup() -> (tempfile::TempDir, HostRuntime, Connection) {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("db"), EventBudget::default()).unwrap();
    let blob = store
        .stage_blob(&mut Cursor::new(b"secret file"), 11, None, 0)
        .unwrap();
    let card = CardRecord::new_with_attachments(
        "card",
        "note",
        1,
        "original",
        b"secret body".to_vec(),
        &[Attachment {
            id: "file".into(),
            display_name: "file.bin".into(),
            media_type: "application/octet-stream".into(),
            byte_length: blob.byte_length,
            sha256: blob.sha256,
        }],
    )
    .unwrap();
    store.create_local("seed", &card).unwrap();
    let mut host = HostRuntime::new(store).unwrap();
    let mut connection = host.connect().unwrap();
    for kind in [
        GrantKind::Rename,
        GrantKind::EditContent,
        GrantKind::ReadContent,
        GrantKind::ReadSummary,
        GrantKind::QueryOperation,
    ] {
        host.grant(&mut connection, kind, "card", 1000, 0).unwrap();
    }
    host.grant(&mut connection, GrantKind::CreateContent, "new", 1000, 0)
        .unwrap();
    host.grant_attachment(&mut connection, "card", "file", 1000, 0)
        .unwrap();
    (dir, host, connection)
}
fn commands() -> Vec<Command> {
    vec![
        Command::CreateContent(CreateContent {
            operation_id: "create".into(),
            card_id: "new".into(),
            type_id: "note".into(),
            format_version: 1,
            title: "new".into(),
            body: b"new body".to_vec(),
        }),
        Command::EditContent(ContentChange {
            operation_id: "edit".into(),
            card_id: "card".into(),
            expected_revision: 1,
            title: "edited".into(),
            body: b"edited body".to_vec(),
            preview_text: String::new(),
            attachments: None,
        }),
        Command::Rename(RenameRequest {
            operation_id: "rename".into(),
            card_id: "card".into(),
            expected_revision: 1,
            title: "renamed".into(),
        }),
        Command::ReadContent(ReadContent {
            request_id: "content".into(),
            card_id: "card".into(),
            expected_revision: 1,
            offset: 0,
            length: 32,
        }),
        Command::ReadSummary {
            request_id: "summary".into(),
            card_id: "card".into(),
        },
        Command::ReadAttachment(ReadAttachment {
            request_id: "attachment".into(),
            card_id: "card".into(),
            attachment_id: "file".into(),
            expected_revision: 1,
            offset: 0,
            length: 32,
        }),
        Command::QueryOperation {
            request_id: "lookup".into(),
            card_id: "card".into(),
            operation_id: "seed".into(),
        },
    ]
}
fn decode(bytes: Vec<u8>) -> Outcome {
    Response::decode(&bytes).unwrap().outcome
}
fn unchanged(host: &HostRuntime) {
    let card = host.store_local().card("card").unwrap().unwrap();
    assert_eq!(card.summary().revision, 1);
    assert_eq!(card.body(), b"secret body");
    assert!(host.store_local().card("new").unwrap().is_none());
    assert_eq!(host.store_local().pending(0, 10).unwrap().len(), 1);
}
#[test]
fn every_command_checks_guard_before_first_object_access() {
    for command in commands() {
        let (_dir, mut host, connection) = setup();
        let mut calls = 0;
        let outcome = decode(
            host.dispatch_guarded(
                &connection,
                &command.encode().unwrap(),
                || 7,
                |actual, now| {
                    assert_eq!(actual, &command);
                    assert_eq!(now, 7);
                    calls += 1;
                    Err(Error::Invalid("host policy denied"))
                },
            )
            .unwrap(),
        );
        assert_eq!(outcome, Outcome::Rejected(Failure::Denied));
        assert_eq!(calls, 1);
        unchanged(&host);
    }
}
#[test]
fn all_writes_reject_final_transaction_guard_without_mutation() {
    for (command, rejected_at) in commands().into_iter().take(3).zip([2, 2, 3]) {
        let (_dir, mut host, connection) = setup();
        let mut calls = 0;
        let outcome = decode(
            host.dispatch_guarded(
                &connection,
                &command.encode().unwrap(),
                || 8,
                |_, _| {
                    calls += 1;
                    if calls == rejected_at {
                        Err(Error::Invalid("precommit revoked"))
                    } else {
                        Ok(())
                    }
                },
            )
            .unwrap(),
        );
        assert_eq!(outcome, Outcome::Rejected(Failure::Denied));
        assert_eq!(calls, rejected_at);
        unchanged(&host);
        assert!(matches!(
            host.store_local().lookup(command.request_id()).unwrap(),
            Lookup::Absent
        ));
    }
}
#[test]
fn read_second_check_and_final_encoded_delivery_never_release_revoked_data() {
    for command in commands().into_iter().skip(3) {
        // Content does two two-sided reads; all others one. The last guard is after encoding.
        let final_call = if matches!(command, Command::ReadContent(_)) {
            5
        } else {
            3
        };
        for rejected_at in [2, final_call] {
            let (_dir, mut host, connection) = setup();
            let revoked = host.revocation(&connection).unwrap();
            let mut calls = 0;
            let bytes = host
                .dispatch_guarded(
                    &connection,
                    &command.encode().unwrap(),
                    || 9,
                    |_, _| {
                        calls += 1;
                        if calls == rejected_at {
                            revoked.revoke();
                        }
                        Ok(()) // original permission must still reject this otherwise-successful guard
                    },
                )
                .unwrap();
            assert_eq!(decode(bytes.clone()), Outcome::Rejected(Failure::Denied));
            assert_eq!(calls, rejected_at);
            assert!(!bytes.windows(6).any(|part| part == b"secret"));
        }
    }
}
#[test]
fn late_write_delivery_denial_keeps_commit_and_retry_still_requires_guard() {
    for (command, final_call) in commands().into_iter().take(3).zip([3, 3, 4]) {
        let (_dir, mut host, connection) = setup();
        let mut calls = 0;
        let bytes = command.encode().unwrap();
        assert_eq!(
            decode(
                host.dispatch_guarded(
                    &connection,
                    &bytes,
                    || 10,
                    |_, _| {
                        calls += 1;
                        if calls == final_call {
                            Err(Error::Invalid("late denial"))
                        } else {
                            Ok(())
                        }
                    }
                )
                .unwrap()
            ),
            Outcome::Rejected(Failure::Denied)
        );
        assert_eq!(calls, final_call);
        assert!(matches!(
            host.store_local().lookup(command.request_id()).unwrap(),
            Lookup::Committed(_)
        ));
        let pending = host.store_local().pending(0, 10).unwrap().len();
        assert_eq!(
            decode(
                host.dispatch_guarded(
                    &connection,
                    &bytes,
                    || 11,
                    |_, _| Err(Error::Invalid("retry denied"))
                )
                .unwrap()
            ),
            Outcome::Rejected(Failure::Denied)
        );
        assert_eq!(host.store_local().pending(0, 10).unwrap().len(), pending);
        assert!(!matches!(
            decode(host.dispatch(&connection, &bytes, || 12).unwrap()),
            Outcome::Rejected(_)
        ));
    }
}
#[test]
fn successful_guard_cannot_replace_object_or_connection_permission() {
    let (_dir, mut host, connection) = setup();
    let foreign = host.connect().unwrap();
    for command in commands() {
        assert_eq!(
            decode(
                host.dispatch_guarded(&foreign, &command.encode().unwrap(), || 999, |_, _| Ok(()))
                    .unwrap()
            ),
            Outcome::Rejected(Failure::Denied)
        );
    }
    let (_other_dir, mut other, _) = setup();
    let read = commands().remove(4).encode().unwrap();
    assert_eq!(
        decode(
            other
                .dispatch_guarded(&connection, &read, || 999, |_, _| Ok(()))
                .unwrap()
        ),
        Outcome::Rejected(Failure::Denied)
    );
    // Foreign rejections did not advance either genuine host clock.
    assert!(matches!(
        decode(host.dispatch(&connection, &read, || 1).unwrap()),
        Outcome::Summary(_)
    ));
    unchanged(&host);
}
#[test]
fn probe_replacement_revocation_and_expiry_never_reauthorize_old_grant() {
    fn send_sync<T: Send + Sync>() {}
    send_sync::<ContentAuthorization>();
    let (_dir, mut host, mut connection) = setup();
    let old = host
        .content_authorization(&connection, GrantKind::ReadContent, "card", None, 1)
        .unwrap();
    host.grant(&mut connection, GrantKind::ReadContent, "card", 100, 2)
        .unwrap();
    assert!(old.check(3).is_err());
    let fresh = host
        .content_authorization(&connection, GrantKind::ReadContent, "card", None, 3)
        .unwrap();
    let cloned = fresh.clone();
    std::thread::spawn(move || cloned.check(4).unwrap())
        .join()
        .unwrap();
    assert!(fresh.check(3).is_err());
    host.revoke(&mut connection, GrantKind::ReadContent, "card")
        .unwrap();
    assert!(fresh.check(5).is_err());
    host.grant(&mut connection, GrantKind::ReadContent, "card", 10, 5)
        .unwrap();
    let short = host
        .content_authorization(&connection, GrantKind::ReadContent, "card", None, 6)
        .unwrap();
    short.check(9).unwrap();
    assert!(short.check(10).is_err());
}
#[test]
fn probe_is_exact_scoped_and_host_stop_drop_are_terminal() {
    let (_dir, mut host, connection) = setup();
    let probe = host
        .content_authorization(
            &connection,
            GrantKind::ReadAttachment,
            "card",
            Some("file"),
            1,
        )
        .unwrap();
    assert!(
        host.content_authorization(
            &connection,
            GrantKind::ReadAttachment,
            "card",
            Some("missing"),
            999
        )
        .is_err()
    );
    assert!(
        host.content_authorization(
            &connection,
            GrantKind::ReadContent,
            "card",
            Some("file"),
            999
        )
        .is_err()
    );
    let (_foreign_dir, foreign, _) = setup();
    assert!(
        foreign
            .content_authorization(
                &connection,
                GrantKind::ReadAttachment,
                "card",
                Some("file"),
                999
            )
            .is_err()
    );
    probe.check(2).unwrap();
    host.disconnect(&connection).unwrap();
    assert!(probe.check(3).is_err());
    let mut next = host.connect().unwrap();
    host.grant(&mut next, GrantKind::ReadContent, "card", 100, 3)
        .unwrap();
    let probe = host
        .content_authorization(&next, GrantKind::ReadContent, "card", None, 4)
        .unwrap();
    drop(host);
    assert!(probe.check(5).is_err());
}
#[test]
fn probe_external_revocation_and_shared_host_clock_are_checked() {
    let (_dir, mut host, connection) = setup();
    let probe = host
        .content_authorization(&connection, GrantKind::ReadContent, "card", None, 1)
        .unwrap();
    let read = commands().remove(4).encode().unwrap();
    host.dispatch(&connection, &read, || 8).unwrap();
    assert!(probe.check(7).is_err());
    probe.check(9).unwrap();
    assert_eq!(
        decode(host.dispatch(&connection, &read, || 8).unwrap()),
        Outcome::Rejected(Failure::Denied)
    );
    host.revocation(&connection).unwrap().revoke();
    assert!(probe.check(10).is_err());
}

#[test]
fn legacy_dispatch_retains_original_clock_boundaries() {
    // Existing workers observe commits on later host calls. An additional sample in
    // legacy dispatch would move their cancellation boundary inside the first call.
    for (command, original_count) in commands().into_iter().zip([2, 2, 4, 4, 2, 2, 2]) {
        let (_dir, mut host, connection) = setup();
        let mut calls = 0;
        let outcome = decode(
            host.dispatch(&connection, &command.encode().unwrap(), || {
                calls += 1;
                calls
            })
            .unwrap(),
        );
        assert!(!matches!(outcome, Outcome::Rejected(_)));
        assert_eq!(calls, original_count);
    }
}
