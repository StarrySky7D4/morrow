#![cfg(not(target_arch = "wasm32"))]
use morrow_core::{
    Error,
    content::CardRecord,
    content_change::ContentChange,
    dispatch::{Connection, HostRuntime},
    lifecycle::GrantKind,
    store::{EventBudget, Store},
    transaction::Lookup,
};
fn setup(granted: bool) -> (tempfile::TempDir, HostRuntime, Connection) {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("content.db"), EventBudget::default()).unwrap();
    store
        .create_local(
            "seed",
            &CardRecord::new("card", "note", 1, "original", b"original bytes".to_vec()).unwrap(),
        )
        .unwrap();
    let mut host = HostRuntime::new(store).unwrap();
    let mut connection = host.connect().unwrap();
    if granted {
        host.grant(&mut connection, GrantKind::EditContent, "card", 1000, 0)
            .unwrap();
    }
    (dir, host, connection)
}
fn change() -> ContentChange {
    ContentChange {
        operation_id: "edit".into(),
        card_id: "card".into(),
        expected_revision: 1,
        title: "changed".into(),
        body: b"proposal bytes".to_vec(),
        preview_text: "preview".into(),
        attachments: None,
    }
}
fn original(host: &HostRuntime) {
    let card = host.store_local().card("card").unwrap().unwrap();
    assert_eq!(card.summary().revision, 1);
    assert_eq!(card.body(), b"original bytes");
    assert!(matches!(
        host.store_local().lookup("edit").unwrap(),
        Lookup::Absent
    ));
    assert_eq!(host.store_local().pending(0, 10).unwrap().len(), 1);
    host.store_local().integrity_check().unwrap();
}
#[test]
fn final_guard_denial_rolls_back_content_receipt_and_event() {
    let (_dir, mut host, connection) = setup(true);
    let mut clock = vec![10, 30].into_iter();
    let mut observed = Vec::new();
    let result = host.edit_content_guarded(
        &connection,
        &change(),
        || clock.next().expect("only two clock reads"),
        |now| {
            observed.push(now);
            if now == 30 {
                Err(Error::Invalid("provider revoked"))
            } else {
                Ok(())
            }
        },
    );
    assert_eq!(result, Err(Error::Invalid("provider revoked")));
    assert_eq!(observed, [10, 30]);
    original(&host);
}
#[test]
fn duplicate_receipt_requires_both_guards_without_undoing_committed_content() {
    let (_dir, mut host, connection) = setup(true);
    let receipt = host.edit_content(&connection, &change(), || 1).unwrap();
    for rejected_at in [1, 2] {
        let mut guards = 0;
        let result = host.edit_content_guarded(
            &connection,
            &change(),
            || 2,
            |_| {
                guards += 1;
                if guards == rejected_at {
                    Err(Error::Invalid("provider revoked"))
                } else {
                    Ok(())
                }
            },
        );
        assert_eq!(result, Err(Error::Invalid("provider revoked")));
        assert_eq!(guards, rejected_at);
        assert_eq!(
            host.store_local().lookup("edit").unwrap(),
            Lookup::Committed(receipt.clone())
        );
        assert_eq!(
            host.store_local().card("card").unwrap().unwrap().body(),
            b"proposal bytes"
        );
        assert_eq!(host.store_local().pending(0, 10).unwrap().len(), 2);
    }
    let mut guards = 0;
    assert_eq!(
        host.edit_content_guarded(
            &connection,
            &change(),
            || 3,
            |_| {
                guards += 1;
                Ok(())
            }
        )
        .unwrap(),
        receipt
    );
    assert_eq!(guards, 2);
    assert_eq!(host.store_local().pending(0, 10).unwrap().len(), 2);
}
#[test]
fn accepting_guard_cannot_supply_missing_wrong_or_expired_content_authority() {
    let (_dir, mut host, mut connection) = setup(false);
    assert!(
        host.edit_content_guarded(
            &connection,
            &change(),
            || panic!("missing content grant"),
            |_| Ok(())
        )
        .is_err()
    );
    host.grant(&mut connection, GrantKind::Rename, "card", 1000, 0)
        .unwrap();
    host.grant(&mut connection, GrantKind::EditContent, "other", 1000, 0)
        .unwrap();
    assert!(
        host.edit_content_guarded(&connection, &change(), || 1, |_| Ok(()))
            .is_err()
    );
    host.grant(&mut connection, GrantKind::EditContent, "card", 5, 0)
        .unwrap();
    let mut observed = Vec::new();
    assert!(
        host.edit_content_guarded(
            &connection,
            &change(),
            || 5,
            |now| {
                observed.push(now);
                Ok(())
            }
        )
        .is_err()
    );
    assert_eq!(observed, [5]);
    original(&host);
}
#[test]
fn scope_is_checked_after_guard_triggered_revocation() {
    let (_dir, mut host, connection) = setup(true);
    let revoked = host.revocation(&connection).unwrap();
    let mut calls = 0;
    assert!(!revoked.is_revoked());
    assert!(
        host.edit_content_guarded(
            &connection,
            &change(),
            || 1,
            |_| {
                calls += 1;
                if calls == 2 {
                    revoked.revoke();
                }
                Ok(())
            }
        )
        .is_err()
    );
    assert_eq!(calls, 2);
    assert!(revoked.is_revoked());
    original(&host);
}
#[test]
fn guard_observes_exact_clock_values_and_no_extra_ticks_are_consumed() {
    let (_dir, mut host, connection) = setup(true);
    let mut ticks = [42, 900].into_iter();
    let mut observed = Vec::new();
    let receipt = host
        .edit_content_guarded(
            &connection,
            &change(),
            || ticks.next().expect("unexpected clock read"),
            |now| {
                observed.push(now);
                Ok(())
            },
        )
        .unwrap();
    assert_eq!(observed, [42, 900]);
    assert!(ticks.next().is_none());
    assert_eq!(receipt.revision, 2);
    assert_eq!(host.store_local().pending(0, 10).unwrap().len(), 2);
}
#[test]
fn revocation_observation_is_shared_read_only_and_one_way() {
    let (_dir, host, connection) = setup(false);
    let signal = host.revocation(&connection).unwrap();
    let other = signal.clone();
    assert!(!signal.is_revoked());
    std::thread::spawn(move || {
        other.revoke();
        assert!(other.is_revoked());
    })
    .join()
    .unwrap();
    assert!(signal.is_revoked());
    signal.revoke();
    assert!(signal.is_revoked());
}
