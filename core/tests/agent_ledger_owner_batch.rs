#![cfg(not(target_arch = "wasm32"))]
//! Additive owner-bound cleanup in the original Store; no v25 schema change.
use morrow_core::{
    Error,
    agent_ledger::Record,
    store::{AgentLedgerMutation as Mutation, EventBudget, ServiceAuthorityResource, Store},
};
const DOMAIN: [u8; 32] = [119; 32];
fn record(id: &str, revision: u64, body: &[u8]) -> Record {
    Record::new(DOMAIN, id, revision, body.to_vec()).unwrap()
}
fn fixture() -> (tempfile::TempDir, std::path::PathBuf, Store) {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("original-core.sqlite");
    let store = Store::open(&path, EventBudget::default()).unwrap();
    (temp, path, store)
}

#[test]
fn unrelated_service_revocation_preserves_ledger_owner_and_legacy_semantics() {
    let (_temp, _path, mut store) = fixture();
    let legacy = store.pin_service_authority().unwrap();
    let scoped = store
        .narrow_service_authority(
            &legacy,
            &[ServiceAuthorityResource::Configuration("kept".into())],
        )
        .unwrap();
    let owner = store.pin_agent_ledger_owner().unwrap();
    store
        .service_authority_control()
        .revoke_resource(&ServiceAuthorityResource::Configuration("unrelated".into()));
    assert!(legacy.check().is_err());
    scoped.check().unwrap();
    store.validate_agent_ledger_owner(&owner).unwrap();
    store
        .batch_agent_ledger_local(
            &owner,
            &[Mutation::Put {
                record: record("active", 1, b"fixed"),
                expected_revision: 0,
            }],
        )
        .unwrap();
    store
        .service_authority_control()
        .revoke_resource(&ServiceAuthorityResource::Configuration("kept".into()));
    assert!(scoped.check().is_err());
    owner.check().unwrap();
}

#[test]
fn ledger_epoch_rotation_invalidates_every_old_owner_but_no_service_lease() {
    let (_temp, _path, mut store) = fixture();
    let legacy = store.pin_service_authority().unwrap();
    let first = store.pin_agent_ledger_owner().unwrap();
    let clone = first.clone();
    let other = store.pin_agent_ledger_owner().unwrap();
    for owner in [&first, &clone] {
        assert!(store.validate_agent_ledger_owner(owner).is_err());
    }
    store.validate_agent_ledger_owner(&other).unwrap();
    store.invalidate_agent_ledger_owner(&other).unwrap();
    for owner in [&first, &clone, &other] {
        assert!(store.validate_agent_ledger_owner(owner).is_err());
    }
    assert!(store.invalidate_agent_ledger_owner(&first).is_err());
    legacy.check().unwrap();
    let fresh = store.pin_agent_ledger_owner().unwrap();
    store.validate_agent_ledger_owner(&fresh).unwrap();
    store
        .batch_agent_ledger_local(
            &fresh,
            &[Mutation::Put {
                record: record("new-generation", 1, b"new"),
                expected_revision: 0,
            }],
        )
        .unwrap();
}

#[test]
fn global_revoke_and_store_close_never_restore_an_old_owner() {
    let (_temp, path, mut store) = fixture();
    let owner = store.pin_agent_ledger_owner().unwrap();
    store.service_authority_control().revoke_all();
    assert!(owner.check().is_err());
    assert!(
        store
            .batch_agent_ledger_local(
                &owner,
                &[Mutation::Put {
                    record: record("forbidden", 1, b"old"),
                    expected_revision: 0
                }]
            )
            .is_err()
    );
    let fresh = store.pin_agent_ledger_owner().unwrap();
    fresh.check().unwrap();
    drop(store);
    assert!(fresh.check().is_err());
    let mut reopened = Store::open_existing(&path, EventBudget::default()).unwrap();
    assert!(reopened.validate_agent_ledger_owner(&fresh).is_err());
    reopened.pin_agent_ledger_owner().unwrap().check().unwrap();
}

#[test]
fn foreign_store_cannot_use_or_replace_an_original_owner() {
    let (_temp, path, mut original) = fixture();
    let owner = original.pin_agent_ledger_owner().unwrap();
    original
        .batch_agent_ledger_local(
            &owner,
            &[Mutation::Put {
                record: record("history", 1, b"readable"),
                expected_revision: 0,
            }],
        )
        .unwrap();
    let mut foreign = Store::open_existing(&path, EventBudget::default()).unwrap();
    assert_eq!(
        foreign.pin_agent_ledger_owner().err(),
        Some(Error::StorageBusy)
    );
    assert!(foreign.validate_agent_ledger_owner(&owner).is_err());
    assert!(
        foreign
            .batch_agent_ledger_local(
                &owner,
                &[Mutation::Delete {
                    expected: record("history", 1, b"readable")
                }]
            )
            .is_err()
    );
    assert_eq!(
        foreign.agent_ledger_ids_local(&DOMAIN).unwrap(),
        vec!["history"]
    );
    assert_eq!(
        foreign.compare_exchange_agent_ledger_local(&record("bypass", 1, b"blocked"), 0),
        Err(Error::StorageBusy)
    );
    assert_eq!(
        original.agent_ledger_ids_local(&DOMAIN).unwrap(),
        vec!["history"]
    );
}

#[test]
fn metadata_update_and_exact_physical_delete_commit_atomically_and_replay_conflicts() {
    let (_temp, path, mut store) = fixture();
    let owner = store.pin_agent_ledger_owner().unwrap();
    let body = record("private-session", 1, b"private-body");
    let meta = record("metadata", 1, b"active");
    store
        .batch_agent_ledger_local(
            &owner,
            &[
                Mutation::Put {
                    record: body.clone(),
                    expected_revision: 0,
                },
                Mutation::Put {
                    record: meta,
                    expected_revision: 0,
                },
            ],
        )
        .unwrap();
    let deleted_meta = record("metadata", 2, b"tombstone-sha-only");
    let changes = [
        Mutation::Delete { expected: body },
        Mutation::Put {
            record: deleted_meta.clone(),
            expected_revision: 1,
        },
    ];
    store.batch_agent_ledger_local(&owner, &changes).unwrap();
    assert!(
        store
            .load_agent_ledger_local(&DOMAIN, "private-session")
            .unwrap()
            .is_none()
    );
    assert_eq!(
        store
            .load_agent_ledger_local(&DOMAIN, "metadata")
            .unwrap()
            .unwrap()
            .container(),
        deleted_meta.container()
    );
    assert_eq!(
        store.batch_agent_ledger_local(&owner, &changes),
        Err(Error::RevisionConflict)
    );
    drop(store);
    let reopened = Store::open_existing(&path, EventBudget::default()).unwrap();
    assert_eq!(
        reopened.agent_ledger_ids_local(&DOMAIN).unwrap(),
        vec!["metadata"]
    );
    reopened.integrity_check().unwrap();
    let sql = rusqlite::Connection::open(&path).unwrap();
    let version: i64 = sql
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, 25);
    let columns: Vec<String> = sql
        .prepare("PRAGMA table_info(agent_ledger)")
        .unwrap()
        .query_map([], |r| r.get(1))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(columns, vec!["domain", "id", "revision", "payload"]);
}

#[test]
fn wrong_exact_container_or_late_expected_revision_rolls_back_the_entire_batch() {
    let (_temp, _path, mut store) = fixture();
    let owner = store.pin_agent_ledger_owner().unwrap();
    let original = record("history", 1, b"original");
    store
        .batch_agent_ledger_local(
            &owner,
            &[Mutation::Put {
                record: original.clone(),
                expected_revision: 0,
            }],
        )
        .unwrap();
    for bad in [
        record("history", 1, b"forged-same-revision"),
        record("history", 2, b"original"),
    ] {
        assert_eq!(
            store.batch_agent_ledger_local(
                &owner,
                &[
                    Mutation::Put {
                        record: record("not-inserted", 1, b"tombstone"),
                        expected_revision: 0
                    },
                    Mutation::Delete { expected: bad },
                ]
            ),
            Err(Error::RevisionConflict)
        );
        assert_eq!(
            store.agent_ledger_ids_local(&DOMAIN).unwrap(),
            vec!["history"]
        );
        assert_eq!(
            store
                .load_agent_ledger_local(&DOMAIN, "history")
                .unwrap()
                .unwrap()
                .container(),
            original.container()
        );
    }
}

#[test]
fn quota_failure_restores_deleted_body_and_original_metadata() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("quota.sqlite");
    let mut store = Store::open(
        &path,
        EventBudget {
            max_count: 1024,
            max_bytes: 4096,
        },
    )
    .unwrap();
    let owner = store.pin_agent_ledger_owner().unwrap();
    let body = record("history", 1, b"private");
    let metadata = record("metadata", 1, b"active");
    store
        .batch_agent_ledger_local(
            &owner,
            &[
                Mutation::Put {
                    record: body.clone(),
                    expected_revision: 0,
                },
                Mutation::Put {
                    record: metadata.clone(),
                    expected_revision: 0,
                },
            ],
        )
        .unwrap();
    let huge = Record::new(DOMAIN, "metadata", 2, vec![0; 8192]).unwrap();
    assert_eq!(
        store.batch_agent_ledger_local(
            &owner,
            &[
                Mutation::Delete {
                    expected: body.clone()
                },
                Mutation::Put {
                    record: huge,
                    expected_revision: 1
                }
            ]
        ),
        Err(Error::EventCapacity)
    );
    assert_eq!(
        store
            .load_agent_ledger_local(&DOMAIN, "history")
            .unwrap()
            .unwrap()
            .container(),
        body.container()
    );
    assert_eq!(
        store
            .load_agent_ledger_local(&DOMAIN, "metadata")
            .unwrap()
            .unwrap()
            .container(),
        metadata.container()
    );
    store.integrity_check().unwrap();
}

#[test]
fn duplicate_keys_empty_batch_and_unsupported_memory_owner_are_rejected() {
    let (_temp, _path, mut store) = fixture();
    let owner = store.pin_agent_ledger_owner().unwrap();
    assert_eq!(
        store.batch_agent_ledger_local(&owner, &[]),
        Err(Error::Limit)
    );
    let candidate = record("duplicate", 1, b"same");
    assert!(
        store
            .batch_agent_ledger_local(
                &owner,
                &[
                    Mutation::Put {
                        record: candidate.clone(),
                        expected_revision: 0
                    },
                    Mutation::Delete {
                        expected: candidate
                    }
                ]
            )
            .is_err()
    );
    assert!(store.agent_ledger_ids_local(&DOMAIN).unwrap().is_empty());
    // The public ordinary Store already rejects an in-memory adapter without
    // WAL. It cannot become a file-backed owner through a storage fallback.
    assert!(Store::open(std::path::Path::new(":memory:"), EventBudget::default()).is_err());
}

#[test]
fn lowered_quota_preserves_paid_markers_and_cleanup_but_rejects_growth() {
    let (_temp, path, mut store) = fixture();
    let owner = store.pin_agent_ledger_owner().unwrap();
    let body = record("private", 1, &[42; 4096]);
    let metadata = record("reserved", 1, &[0; 2048]);
    store
        .batch_agent_ledger_local(
            &owner,
            &[
                Mutation::Put {
                    record: body.clone(),
                    expected_revision: 0,
                },
                Mutation::Put {
                    record: metadata,
                    expected_revision: 0,
                },
            ],
        )
        .unwrap();
    drop(store);
    let mut store = Store::open_existing(
        &path,
        EventBudget {
            max_count: 1024,
            max_bytes: 1,
        },
    )
    .unwrap();
    let consumed = record("reserved", 2, &[1; 2048]);
    // Legacy local CAS keeps its old quota behavior.
    assert_eq!(
        store.compare_exchange_agent_ledger_local(&consumed, 1),
        Err(Error::EventCapacity)
    );
    let owner = store.pin_agent_ledger_owner().unwrap();
    store
        .batch_agent_ledger_local(
            &owner,
            &[Mutation::Put {
                record: consumed.clone(),
                expected_revision: 1,
            }],
        )
        .unwrap();
    assert_eq!(
        store.batch_agent_ledger_local(
            &owner,
            &[Mutation::Put {
                record: record("new", 1, b"even-small-new-row"),
                expected_revision: 0,
            }]
        ),
        Err(Error::EventCapacity)
    );
    assert_eq!(
        store.batch_agent_ledger_local(
            &owner,
            &[Mutation::Put {
                record: record("reserved", 3, &[1; 2049]),
                expected_revision: 2,
            }]
        ),
        Err(Error::EventCapacity)
    );
    let retired = record("reserved", 3, &[2; 2048]);
    let retirement = [
        Mutation::Put {
            record: retired.clone(),
            expected_revision: 2,
        },
        Mutation::Delete { expected: body },
    ];
    store.batch_agent_ledger_local(&owner, &retirement).unwrap();
    assert_eq!(
        store.batch_agent_ledger_local(&owner, &retirement),
        Err(Error::RevisionConflict)
    );
    assert_eq!(
        store.agent_ledger_ids_local(&DOMAIN).unwrap(),
        vec!["reserved"]
    );
    assert_eq!(
        store
            .load_agent_ledger_local(&DOMAIN, "reserved")
            .unwrap()
            .unwrap()
            .container(),
        retired.container()
    );
    store.integrity_check().unwrap();
}

#[test]
fn nongrowing_batch_still_rejects_corrupt_untouched_records_without_changes() {
    let (_temp, path, mut store) = fixture();
    let owner = store.pin_agent_ledger_owner().unwrap();
    let original = record("reserved", 1, &[0; 2048]);
    store
        .batch_agent_ledger_local(
            &owner,
            &[
                Mutation::Put {
                    record: original.clone(),
                    expected_revision: 0,
                },
                Mutation::Put {
                    record: record("untouched", 1, b"valid"),
                    expected_revision: 0,
                },
            ],
        )
        .unwrap();
    let sql = rusqlite::Connection::open(&path).unwrap();
    sql.execute(
        "UPDATE agent_ledger SET revision=2 WHERE id='untouched'",
        [],
    )
    .unwrap();
    assert_eq!(
        store.batch_agent_ledger_local(
            &owner,
            &[Mutation::Put {
                record: record("reserved", 2, &[1; 2048]),
                expected_revision: 1,
            }]
        ),
        Err(Error::Integrity)
    );
    assert_eq!(
        store
            .load_agent_ledger_local(&DOMAIN, "reserved")
            .unwrap()
            .unwrap()
            .container(),
        original.container()
    );
}

#[test]
fn generic_ledger_writes_cannot_bypass_an_active_owner_or_replacement_epoch() {
    let (_temp, _path, mut store) = fixture();
    let legacy_service = store.pin_service_authority().unwrap();
    let original = record("protected", 1, b"original");
    store
        .compare_exchange_agent_ledger_local(&original, 0)
        .unwrap();
    let owner = store.pin_agent_ledger_owner().unwrap();
    for candidate in [
        record("protected", 2, b"tamper"),
        record("extra", 1, b"unregistered"),
    ] {
        let expected = candidate.revision() - 1;
        assert_eq!(
            store.compare_exchange_agent_ledger_local(&candidate, expected),
            Err(Error::Invalid("active agent ledger owner"))
        );
    }
    let replacement = store.pin_agent_ledger_owner().unwrap();
    assert!(owner.check().is_err());
    assert!(
        store
            .batch_agent_ledger_local(
                &owner,
                &[Mutation::Delete {
                    expected: original.clone()
                }]
            )
            .is_err()
    );
    assert_eq!(
        store
            .load_agent_ledger_local(&DOMAIN, "protected")
            .unwrap()
            .unwrap()
            .container(),
        original.container()
    );
    store
        .batch_agent_ledger_local(&replacement, &[Mutation::Delete { expected: original }])
        .unwrap();
    assert!(
        store
            .compare_exchange_agent_ledger_local(&record("still-owned", 1, b"blocked"), 0)
            .is_err()
    );
    store.service_authority_control().revoke_all();
    assert!(replacement.check().is_err());
    store
        .compare_exchange_agent_ledger_local(&record("explicitly-unowned", 1, b"local"), 0)
        .unwrap();
    assert!(legacy_service.check().is_err());
}

#[cfg(feature = "fault-injection")]
#[test]
#[ignore = "child harness; parent supplies a dedicated Store and crash boundary"]
fn ledger_batch_crash_child() {
    let path = std::env::var_os("MORROW_LEDGER_BATCH_CRASH_STORE").expect("child Store path");
    let mut store =
        Store::open_existing(&std::path::PathBuf::from(path), EventBudget::default()).unwrap();
    let owner = store.pin_agent_ledger_owner().unwrap();
    let body = store
        .load_agent_ledger_local(&DOMAIN, "private")
        .unwrap()
        .unwrap();
    let metadata = store
        .load_agent_ledger_local(&DOMAIN, "metadata")
        .unwrap()
        .unwrap();
    store
        .batch_agent_ledger_local(
            &owner,
            &[
                Mutation::Delete { expected: body },
                Mutation::Put {
                    record: record("metadata", 2, b"tombstone"),
                    expected_revision: metadata.revision(),
                },
            ],
        )
        .unwrap();
    panic!("requested crash boundary did not run");
}

#[cfg(feature = "fault-injection")]
#[test]
fn batch_crash_boundaries_leave_delete_and_tombstone_atomic_and_never_replay_success() {
    for (boundary, committed) in [
        ("agent-ledger-batch-before-commit", false),
        ("agent-ledger-batch-after-commit", true),
    ] {
        let (_temp, path, mut store) = fixture();
        let owner = store.pin_agent_ledger_owner().unwrap();
        let body = record("private", 1, b"private-body");
        let metadata = record("metadata", 1, b"active");
        store
            .batch_agent_ledger_local(
                &owner,
                &[
                    Mutation::Put {
                        record: body.clone(),
                        expected_revision: 0,
                    },
                    Mutation::Put {
                        record: metadata,
                        expected_revision: 0,
                    },
                ],
            )
            .unwrap();
        drop(store);
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "ledger_batch_crash_child",
                "--nocapture",
            ])
            .env("MORROW_LEDGER_BATCH_CRASH_STORE", &path)
            .env("MORROW_TEST_CRASH_AT", boundary)
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(86), "{boundary}");
        let mut reopened = Store::open_existing(&path, EventBudget::default()).unwrap();
        let loaded = reopened
            .load_agent_ledger_local(&DOMAIN, "metadata")
            .unwrap()
            .unwrap();
        assert_eq!(
            loaded.revision(),
            if committed { 2 } else { 1 },
            "{boundary}"
        );
        assert_eq!(
            reopened
                .load_agent_ledger_local(&DOMAIN, "private")
                .unwrap()
                .is_none(),
            committed,
            "{boundary}"
        );
        reopened.integrity_check().unwrap();
        let owner = reopened.pin_agent_ledger_owner().unwrap();
        let changes = [
            Mutation::Delete { expected: body },
            Mutation::Put {
                record: record("metadata", 2, b"tombstone"),
                expected_revision: 1,
            },
        ];
        let result = reopened.batch_agent_ledger_local(&owner, &changes);
        if committed {
            assert_eq!(result, Err(Error::RevisionConflict));
        } else {
            result.unwrap();
        }
        assert_eq!(
            reopened.batch_agent_ledger_local(&owner, &changes),
            Err(Error::RevisionConflict)
        );
    }
}
