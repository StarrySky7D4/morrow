#![cfg(not(target_arch = "wasm32"))]
//! Durable claims in the original library. No recovered history is live authority.
use morrow_core::{
    Error,
    agent_ledger::{MAX_CONTAINER_BYTES, Record},
    content::CardRecord,
    store::{EventBudget, MAX_AGENT_LEDGER_RECORDS, Store},
};
use rusqlite::{Connection, params};
use std::path::PathBuf;

const DOMAIN: [u8; 32] = [31; 32];
fn fixture() -> (tempfile::TempDir, PathBuf, Store) {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("original-library.db");
    let store = Store::open(&path, EventBudget::default()).unwrap();
    (temp, path, store)
}
fn record(id: &str, revision: u64, payload: &[u8]) -> Record {
    Record::new(DOMAIN, id, revision, payload.to_vec()).unwrap()
}
fn noise(length: usize) -> Vec<u8> {
    let mut state = 0x7b25b42eu32;
    (0..length)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state as u8
        })
        .collect()
}
fn version(path: &std::path::Path) -> i64 {
    Connection::open(path)
        .unwrap()
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap()
}

#[test]
fn exact_ledger_container_and_namespace_survive_reopen_and_original_snapshot() {
    let (temp, path, mut store) = fixture();
    let first = record("session", 1, b"opaque\0private\xff");
    store
        .compare_exchange_agent_ledger_local(&first, 0)
        .unwrap();
    let other = Record::new([32; 32], "session", 1, b"other-domain".to_vec()).unwrap();
    store
        .compare_exchange_agent_ledger_local(&other, 0)
        .unwrap();
    let snapshot = temp.path().join("same-library-snapshot.db");
    store.snapshot_to(&snapshot, 16 * 1024 * 1024).unwrap();
    let second = record("session", 2, b"updated");
    store
        .compare_exchange_agent_ledger_local(&second, 1)
        .unwrap();
    drop(store);
    let reopened = Store::open_existing(&path, Default::default()).unwrap();
    assert_eq!(
        reopened
            .load_agent_ledger_local(&DOMAIN, "session")
            .unwrap()
            .unwrap()
            .container(),
        second.container()
    );
    assert_eq!(
        reopened
            .load_agent_ledger_local(&[32; 32], "session")
            .unwrap()
            .unwrap()
            .payload(),
        b"other-domain"
    );
    assert!(
        reopened
            .load_agent_ledger_local(&DOMAIN, "absent")
            .unwrap()
            .is_none()
    );
    let prior = Store::open_existing(&snapshot, Default::default()).unwrap();
    assert_eq!(
        prior
            .load_agent_ledger_local(&DOMAIN, "session")
            .unwrap()
            .unwrap()
            .container(),
        first.container()
    );
    reopened.integrity_check().unwrap();
    prior.integrity_check().unwrap();
}

#[test]
fn already_committed_identical_claim_never_returns_success_again() {
    let (_temp, _path, mut store) = fixture();
    let first = record("execute-once", 1, b"claimed");
    store
        .compare_exchange_agent_ledger_local(&first, 0)
        .unwrap();
    for _ in 0..2 {
        assert_eq!(
            store.compare_exchange_agent_ledger_local(&first, 0),
            Err(Error::RevisionConflict)
        );
    }
    let second = record("execute-once", 2, b"done");
    assert_eq!(
        store.compare_exchange_agent_ledger_local(&second, 0),
        Err(Error::RevisionConflict)
    );
    store
        .compare_exchange_agent_ledger_local(&second, 1)
        .unwrap();
    assert_eq!(
        store.compare_exchange_agent_ledger_local(&second, 1),
        Err(Error::RevisionConflict)
    );
    assert_eq!(
        store.compare_exchange_agent_ledger_local(&record("missing", 2, b"bad"), 1),
        Err(Error::RevisionConflict)
    );
    assert_eq!(
        store.compare_exchange_agent_ledger_local(&first, u64::MAX),
        Err(Error::RevisionConflict)
    );
    store.integrity_check().unwrap();
}

#[test]
fn independent_stores_and_parallel_claims_have_only_one_successful_winner() {
    let (_temp, path, first_store) = fixture();
    let second_store = Store::open_existing(&path, Default::default()).unwrap();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let workers: Vec<_> = [first_store, second_store]
        .into_iter()
        .enumerate()
        .map(|(index, mut store)| {
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let candidate = record("parallel", 1, &[index as u8]);
                barrier.wait();
                let result = store.compare_exchange_agent_ledger_local(&candidate, 0);
                (candidate, result)
            })
        })
        .collect();
    let outcomes: Vec<_> = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .collect();
    assert_eq!(
        outcomes.iter().filter(|(_, result)| result.is_ok()).count(),
        1
    );
    for (_, result) in &outcomes {
        assert!(matches!(
            result,
            Ok(()) | Err(Error::StorageBusy) | Err(Error::RevisionConflict)
        ));
    }
    let mut store = Store::open_existing(&path, Default::default()).unwrap();
    let winner = outcomes.iter().find(|(_, result)| result.is_ok()).unwrap();
    assert_eq!(
        store
            .load_agent_ledger_local(&DOMAIN, "parallel")
            .unwrap()
            .unwrap()
            .container(),
        winner.0.container()
    );
    for (candidate, _) in outcomes {
        assert_eq!(
            store.compare_exchange_agent_ledger_local(&candidate, 0),
            Err(Error::RevisionConflict)
        );
    }
    store.integrity_check().unwrap();
}

#[test]
fn shared_authority_writer_pin_coordinates_foreign_store_without_granting_ledger_authority() {
    let (_temp, path, mut owner) = fixture();
    let lease = owner.pin_service_authority().unwrap();
    let mut other = Store::open_existing(&path, Default::default()).unwrap();
    let first = record("pinned", 1, b"history");
    assert_eq!(
        other.compare_exchange_agent_ledger_local(&first, 0),
        Err(Error::StorageBusy)
    );
    owner
        .compare_exchange_agent_ledger_local(&first, 0)
        .unwrap();
    lease.check().unwrap(); // A ledger write neither grants nor revokes resources.
    assert_eq!(
        other
            .load_agent_ledger_local(&DOMAIN, "pinned")
            .unwrap()
            .unwrap()
            .container(),
        first.container()
    );
    drop(owner);
    assert!(lease.check().is_err());
    other
        .compare_exchange_agent_ledger_local(&record("pinned", 2, b"next"), 1)
        .unwrap();
}

#[test]
fn row_and_envelope_identity_corruption_fail_load_integrity_and_reopen() {
    for case in 0..7 {
        let (_temp, path, mut store) = fixture();
        let first = record("stored", 1, b"original");
        store
            .compare_exchange_agent_ledger_local(&first, 0)
            .unwrap();
        let raw = Connection::open(&path).unwrap();
        let (domain, id) = match case {
            0 => {
                raw.execute("UPDATE agent_ledger SET domain=?1", [[41u8; 32].as_slice()])
                    .unwrap();
                ([41; 32], "stored")
            }
            1 => {
                raw.execute("UPDATE agent_ledger SET id='other'", [])
                    .unwrap();
                (DOMAIN, "other")
            }
            2 => {
                raw.execute("UPDATE agent_ledger SET revision=2", [])
                    .unwrap();
                (DOMAIN, "stored")
            }
            3 => {
                raw.execute("UPDATE agent_ledger SET payload=x'01'", [])
                    .unwrap();
                (DOMAIN, "stored")
            }
            4 => {
                raw.execute(
                    "UPDATE agent_ledger SET payload=zeroblob(?1)",
                    [MAX_CONTAINER_BYTES as i64 + 1],
                )
                .unwrap();
                (DOMAIN, "stored")
            }
            5 => {
                raw.execute(
                    "UPDATE agent_ledger SET payload=?1",
                    [record("other", 1, b"same-size").container()],
                )
                .unwrap();
                (DOMAIN, "stored")
            }
            6 => {
                raw.execute_batch("ALTER TABLE agent_ledger ADD COLUMN unchecked INTEGER")
                    .unwrap();
                (DOMAIN, "stored")
            }
            _ => unreachable!(),
        };
        drop(raw);
        assert!(
            store.load_agent_ledger_local(&domain, id).is_err(),
            "load case {case}"
        );
        assert!(store.integrity_check().is_err(), "integrity case {case}");
        assert!(
            Store::open_existing(&path, Default::default()).is_err(),
            "reopen case {case}"
        );
    }
}

#[test]
fn row_limit_is_global_across_domains_and_replacement_remains_possible() {
    let (_temp, path, mut store) = fixture();
    for index in 0..MAX_AGENT_LEDGER_RECORDS {
        let candidate = Record::new(
            [index as u8 + 1; 32],
            "one-per-domain",
            1,
            vec![index as u8],
        )
        .unwrap();
        store
            .compare_exchange_agent_ledger_local(&candidate, 0)
            .unwrap();
    }
    assert_eq!(
        store.compare_exchange_agent_ledger_local(&record("overflow", 1, b"extra"), 0),
        Err(Error::Limit)
    );
    let replacement = Record::new([1; 32], "one-per-domain", 2, b"changed".to_vec()).unwrap();
    store
        .compare_exchange_agent_ledger_local(&replacement, 1)
        .unwrap();
    store.integrity_check().unwrap();
    // Out-of-process excess rows cannot be accepted as a valid reopen.
    let raw = Connection::open(&path).unwrap();
    let extra = record("injected", 1, b"extra");
    raw.execute(
        "INSERT INTO agent_ledger(domain,id,revision,payload) VALUES(?1,?2,?3,?4)",
        params![DOMAIN.as_slice(), extra.id(), 1i64, extra.container()],
    )
    .unwrap();
    drop(raw);
    let over_limit_replacement =
        Record::new([1; 32], "one-per-domain", 3, b"must not claim".to_vec()).unwrap();
    assert_eq!(
        store.compare_exchange_agent_ledger_local(&over_limit_replacement, 2),
        Err(Error::Limit)
    );
    assert_eq!(
        store
            .load_agent_ledger_local(&[1; 32], "one-per-domain")
            .unwrap()
            .unwrap()
            .container(),
        replacement.container()
    );
    assert_eq!(store.integrity_check(), Err(Error::Limit));
    assert!(matches!(
        Store::open_existing(&path, Default::default()),
        Err(Error::Limit)
    ));
}

#[test]
fn replacement_releases_old_quota_and_failed_growth_rolls_back_original_row() {
    let (_temp, path, store) = fixture();
    drop(store);
    let first = record("bounded", 1, &noise(4096));
    let budget = EventBudget {
        max_count: 8,
        max_bytes: first.retained_bytes(),
    };
    let mut store = Store::open_existing(&path, budget).unwrap();
    store
        .compare_exchange_agent_ledger_local(&first, 0)
        .unwrap();
    let smaller = record("bounded", 2, b"tiny");
    store
        .compare_exchange_agent_ledger_local(&smaller, 1)
        .unwrap();
    let too_large = record("bounded", 3, &noise(8192));
    assert_eq!(
        store.compare_exchange_agent_ledger_local(&too_large, 2),
        Err(Error::EventCapacity)
    );
    assert_eq!(
        store
            .load_agent_ledger_local(&DOMAIN, "bounded")
            .unwrap()
            .unwrap()
            .container(),
        smaller.container()
    );
    store.integrity_check().unwrap();
}

#[test]
fn compressible_padding_pre_reserves_incompressible_terminal_state_space() {
    let (_temp, path, store) = fixture();
    drop(store);
    let planned = record("terminal-reservation", 1, &vec![0; 64 * 1024]);
    let budget = EventBudget {
        max_count: 8,
        max_bytes: planned.retained_bytes(),
    };
    let mut store = Store::open_existing(&path, budget).unwrap();
    store
        .compare_exchange_agent_ledger_local(&planned, 0)
        .unwrap();
    assert_eq!(
        store.compare_exchange_agent_ledger_local(&record("other", 1, b"crowd out"), 0),
        Err(Error::EventCapacity)
    );
    let terminal = record("terminal-reservation", 2, &noise(64 * 1024));
    assert_eq!(planned.retained_bytes(), terminal.retained_bytes());
    assert!(terminal.container().len() > planned.container().len());
    store
        .compare_exchange_agent_ledger_local(&terminal, 1)
        .unwrap();
    assert_eq!(
        store
            .load_agent_ledger_local(&DOMAIN, "terminal-reservation")
            .unwrap()
            .unwrap()
            .container(),
        terminal.container()
    );
    store.integrity_check().unwrap();
}

#[test]
fn ledger_and_original_content_outbox_share_one_logical_byte_budget() {
    let (_temp, path, store) = fixture();
    drop(store);
    let ledger = record("quota", 1, b"history");
    let mut store = Store::open_existing(
        &path,
        EventBudget {
            max_count: 8,
            max_bytes: ledger.retained_bytes(),
        },
    )
    .unwrap();
    store
        .compare_exchange_agent_ledger_local(&ledger, 0)
        .unwrap();
    let card = CardRecord::new("card", "example.note", 1, "title", vec![]).unwrap();
    assert_eq!(
        store.create_local("create", &card),
        Err(Error::EventCapacity)
    );
    assert!(store.card("card").unwrap().is_none());
    assert_eq!(store.pending_usage().unwrap(), (0, 0));
    store.integrity_check().unwrap();

    let (_other_temp, other_path, mut other) = fixture();
    other.create_local("create", &card).unwrap();
    let event_bytes = other.pending_usage().unwrap().1;
    drop(other);
    let mut other = Store::open_existing(
        &other_path,
        EventBudget {
            max_count: 8,
            max_bytes: event_bytes + ledger.retained_bytes() - 1,
        },
    )
    .unwrap();
    assert_eq!(
        other.compare_exchange_agent_ledger_local(&ledger, 0),
        Err(Error::EventCapacity)
    );
    assert!(
        other
            .load_agent_ledger_local(&DOMAIN, "quota")
            .unwrap()
            .is_none()
    );
    assert_eq!(other.pending_usage().unwrap().1, event_bytes);
    other.integrity_check().unwrap();
}

#[test]
fn genuine_v24_readonly_and_additive_migration_preserve_original_business_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("version24.db");
    let key = ed25519_dalek::SigningKey::from_bytes(&[77; 32]);
    let trust = morrow_core::audit::TrustedLog {
        id: "ledger-migration".into(),
        key: key.verifying_key(),
    };
    let mut store = Store::open_audited(&path, Default::default(), true, trust.clone()).unwrap();
    let card = CardRecord::new(
        "original",
        "example.note",
        1,
        "unchanged",
        b"original bytes".to_vec(),
    )
    .unwrap();
    store.create_local("create-original", &card).unwrap();
    let original_events = store.pending(0, 128).unwrap();
    let original_card = store.card("original").unwrap().unwrap().encode();
    let original_store = store.tls_store_identity().unwrap();
    drop(store);
    Connection::open(&path)
        .unwrap()
        .execute_batch("DROP TABLE agent_ledger; PRAGMA user_version=24;")
        .unwrap();
    let readonly = Store::open_read_only_audited(&path, trust.clone()).unwrap();
    assert!(
        readonly
            .load_agent_ledger_local(&DOMAIN, "session")
            .unwrap()
            .is_none()
    );
    assert_eq!(version(&path), 24);
    drop(readonly);
    let mut migrated = Store::open_audited(&path, Default::default(), false, trust).unwrap();
    assert_eq!(version(&path), 25);
    assert_eq!(migrated.pending(0, 128).unwrap(), original_events);
    assert_eq!(
        migrated.card("original").unwrap().unwrap().encode(),
        original_card
    );
    assert_eq!(migrated.tls_store_identity().unwrap(), original_store);
    migrated
        .compare_exchange_agent_ledger_local(&record("session", 1, b"new"), 0)
        .unwrap();
    migrated.integrity_check().unwrap();
}

#[test]
fn premature_or_missing_ledger_schema_is_rejected_without_repairing_history() {
    let (_temp, path, store) = fixture();
    drop(store);
    Connection::open(&path)
        .unwrap()
        .execute_batch("PRAGMA user_version=24;")
        .unwrap();
    assert!(matches!(
        Store::open_existing(&path, Default::default()),
        Err(Error::Integrity)
    ));
    assert_eq!(version(&path), 24);
    let raw = Connection::open(&path).unwrap();
    raw.execute_batch("PRAGMA user_version=25; DROP TABLE agent_ledger;")
        .unwrap();
    drop(raw);
    assert!(matches!(
        Store::open_existing(&path, Default::default()),
        Err(Error::Integrity)
    ));
    assert_eq!(version(&path), 25);
}

#[cfg(feature = "fault-injection")]
#[test]
#[ignore = "subprocess worker for explicit crash test"]
fn agent_ledger_subprocess_worker() {
    let path = PathBuf::from(std::env::var_os("MORROW_AGENT_LEDGER_PATH").unwrap());
    let mode = std::env::var("MORROW_AGENT_LEDGER_MODE").unwrap();
    let mut store = Store::open_existing(&path, Default::default()).unwrap();
    if mode == "write" {
        store
            .compare_exchange_agent_ledger_local(&record("crash", 2, b"durable claim"), 1)
            .unwrap();
    }
}

#[cfg(feature = "fault-injection")]
fn crash(path: &std::path::Path, mode: &str, boundary: &str) {
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "agent_ledger_subprocess_worker",
            "--ignored",
            "--nocapture",
        ])
        .env("MORROW_AGENT_LEDGER_PATH", path)
        .env("MORROW_AGENT_LEDGER_MODE", mode)
        .env("MORROW_TEST_CRASH_AT", boundary)
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(86), "{boundary}");
}

#[cfg(feature = "fault-injection")]
#[test]
fn crash_before_and_after_commit_never_duplicate_a_durable_claim() {
    for (boundary, committed) in [
        ("agent-ledger-before-commit", false),
        ("agent-ledger-after-commit", true),
    ] {
        let (_temp, path, mut store) = fixture();
        store
            .compare_exchange_agent_ledger_local(&record("crash", 1, b"before"), 0)
            .unwrap();
        drop(store);
        crash(&path, "write", boundary);
        let mut reopened = Store::open_existing(&path, Default::default()).unwrap();
        let saved = reopened
            .load_agent_ledger_local(&DOMAIN, "crash")
            .unwrap()
            .unwrap();
        assert_eq!(saved.revision(), if committed { 2 } else { 1 });
        let candidate = record("crash", 2, b"durable claim");
        if committed {
            assert_eq!(
                reopened.compare_exchange_agent_ledger_local(&candidate, 1),
                Err(Error::RevisionConflict)
            );
        } else {
            assert_eq!(saved.payload(), b"before");
        }
        reopened.integrity_check().unwrap();
    }
}

#[cfg(feature = "fault-injection")]
#[test]
fn additive_migration_crash_keeps_v24_or_v25_without_changing_existing_events() {
    for (boundary, expected) in [
        ("agent-ledger-migration-before-commit", 24),
        ("agent-ledger-migration-after-commit", 25),
    ] {
        let (_temp, path, mut store) = fixture();
        let card =
            CardRecord::new("card", "example.note", 1, "original", b"keep".to_vec()).unwrap();
        store.create_local("seed", &card).unwrap();
        let original = store.pending(0, 128).unwrap();
        drop(store);
        Connection::open(&path)
            .unwrap()
            .execute_batch("DROP TABLE agent_ledger; PRAGMA user_version=24;")
            .unwrap();
        crash(&path, "migration", boundary);
        assert_eq!(version(&path), expected);
        let reopened = Store::open_existing(&path, Default::default()).unwrap();
        assert_eq!(reopened.pending(0, 128).unwrap(), original);
        assert_eq!(
            reopened.card("card").unwrap().unwrap().encode(),
            card.encode()
        );
        reopened.integrity_check().unwrap();
    }
}
