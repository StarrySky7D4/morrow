//! Ordinary synthetic filesystem tests; no guest/system/owner action is invoked.
use super::Journal;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    directory: PathBuf,
    canonical: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let parent = std::env::temp_dir();
        let name = format!(
            "morrow-journal-budget-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        );
        let directory = parent.join(name);
        std::fs::create_dir(&directory).unwrap();
        let canonical = std::fs::canonicalize(&directory).unwrap();
        let parent = std::fs::canonicalize(parent).unwrap();
        assert_eq!(canonical.parent(), Some(parent.as_path()));
        Self {
            directory,
            canonical,
        }
    }

    fn journal(&self) -> Journal {
        Journal::create(&self.directory.join("new-synthetic-guest-root")).unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // Remove only the exact create-only fixture after its journal has dropped.
        if std::fs::canonicalize(&self.directory).ok().as_ref() == Some(&self.canonical) {
            let _ = std::fs::remove_dir_all(&self.directory);
        }
    }
}

const SUCCESSFUL_CHAIN: [&str; 42] = [
    "guest-identity",
    "inventory",
    "roots",
    "setup",
    "materialize",
    "open-owner",
    "production-factory",
    "sealed-pipe",
    "sealed-session-review",
    "sealed-session-install",
    "sealed-session-base-select",
    "sealed-session-base-enable",
    "sealed-session-wrapper-select",
    "sealed-session-approve",
    "sealed-session-wrapper-enable",
    "sealed-session-context",
    "sealed-session-worker",
    "sealed-session-run",
    "sealed-session-join",
    "sealed-native-review",
    "sealed-controls-review",
    "sealed-controls-install",
    "sealed-controls-base-select",
    "sealed-controls-base-enable",
    "sealed-controls-wrapper-select",
    "sealed-controls-approve",
    "sealed-controls-wrapper-enable",
    "sealed-proposal-review",
    "sealed-proposal-install",
    "sealed-proposal-base-select",
    "sealed-proposal-base-enable",
    "sealed-proposal-wrapper-select",
    "sealed-proposal-approve",
    "sealed-proposal-wrapper-enable",
    "sealed-native-context",
    "sealed-native-worker",
    "sealed-propose",
    "sealed-trusted-review",
    "sealed-trusted-approve-claim",
    "sealed-native-start",
    "sealed-controls",
    "sealed-native-join",
];

fn raw(journal: &Journal) -> Vec<u8> {
    std::fs::read(journal.directory().join("journal.jsonl")).unwrap()
}

#[test]
fn complete_successful_chain_and_explicit_cleanup_remain_reservable() {
    let fixture = Fixture::new();
    let mut journal = fixture.journal();
    let mut steps: Vec<String> = SUCCESSFUL_CHAIN.iter().map(|s| (*s).into()).collect();
    // A failed attempt is charged before any final cleanup reservation.
    steps.push("failed-business-step".into());
    steps.extend((1..=4).map(|n| format!("sealed-repair-cleanup-{n}")));
    steps.push("release-factory".into());
    steps.extend((1..=4).map(|n| format!("reap-factory-scheduler-{n}")));
    steps.extend(["finish-owner", "prepare-cleanup", "finish-cleanup"].map(String::from));
    assert_eq!(steps.len(), 55);
    for step in &steps {
        journal
            .before(
                step,
                &serde_json::json!({"synthetic_budget_test":true,"outcome":"Unknown"}),
            )
            .unwrap();
        journal
            .record("synthetic_observation", &serde_json::json!({"step":step}))
            .unwrap();
    }
    // The failed attempt remains reserved even after final cleanup steps.
    assert!(journal.before("failed-business-step", &()).is_err());
    assert_eq!(journal.attempted.len(), 55);
    assert_eq!(journal.sequence, 111);
    let records: Vec<serde_json::Value> = raw(&journal)
        .split(|b| *b == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice(line).unwrap())
        .collect();
    assert_eq!(records.len(), 111);
    for (index, step) in steps.iter().enumerate() {
        let reservation = &records[1 + index * 2];
        assert_eq!(reservation["sequence"], 1 + index * 2);
        assert_eq!(reservation["kind"], "ATTEMPT_RESERVED_OUTCOME_UNKNOWN");
        assert_eq!(reservation["payload"]["step"].as_str(), Some(step.as_str()));
    }
}

#[test]
fn duplicate_attempt_preserves_original_journal_without_replay() {
    let fixture = Fixture::new();
    let mut journal = fixture.journal();
    journal
        .before("setup", &serde_json::json!({"outcome":"Unknown"}))
        .unwrap();
    let before = raw(&journal);
    assert!(
        journal
            .before("setup", &serde_json::json!({"retry":true}))
            .is_err()
    );
    assert_eq!(raw(&journal), before);
    assert_eq!(journal.sequence, 2);
    assert_eq!(journal.attempted.len(), 1);
    assert!(journal.attempted.contains("setup"));
}

#[test]
fn fixed_attempt_ceiling_rejects_without_eviction_or_record_write() {
    let fixture = Fixture::new();
    let mut journal = fixture.journal();
    // A literal contract ceiling, not the implementation constant: 32 would fail.
    for index in 0..64 {
        journal
            .before(&format!("bounded-step-{index}"), &())
            .unwrap();
    }
    let before = raw(&journal);
    assert!(journal.before("bounded-step-64", &()).is_err());
    assert!(journal.before("bounded-step-0", &()).is_err());
    assert_eq!(raw(&journal), before);
    assert_eq!(journal.sequence, 65);
    assert_eq!(journal.attempted.len(), 64);
    assert!(!journal.attempted.contains("bounded-step-64"));
    assert!(journal.attempted.contains("bounded-step-0"));
    // The independent record budget stays 256; rejection cannot reset it.
    while journal.sequence < 256 {
        journal.record("bounded-observation", &()).unwrap();
    }
    let exhausted = raw(&journal);
    assert!(journal.record("overflow", &()).is_err());
    assert_eq!(journal.sequence, 256);
    assert_eq!(raw(&journal), exhausted);
}
