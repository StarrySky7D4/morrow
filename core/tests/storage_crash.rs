#![cfg(all(not(target_arch = "wasm32"), feature = "fault-injection"))]
use morrow_core::{
    store::{EventBudget, Store},
    transaction::Lookup,
};
use std::{path::Path, process::Command};
const EXE: &str = env!("CARGO_BIN_EXE_morrow-core-store");
fn command(path: &Path, action: &str, tail: &[&str], crash: Option<&str>) -> std::process::Output {
    let mut command = Command::new(EXE);
    command.arg(action).arg(path).args(tail);
    if let Some(point) = crash {
        command.env("MORROW_TEST_CRASH_AT", point);
    }
    command.output().unwrap()
}
fn scenario(rename: bool) {
    for boundary in [
        "after-begin",
        "after-card",
        "after-operation",
        "after-event",
        "before-commit",
        "after-commit",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("crash.db");
        assert!(command(&db, "init", &[], None).status.success());
        if rename {
            assert!(
                command(&db, "create-local", &["seed", "card", "old"], None)
                    .status
                    .success()
            );
        }
        let (action, args) = if rename {
            ("rename-local", vec!["op", "card", "1", "new"])
        } else {
            ("create-local", vec!["op", "card", "new"])
        };
        let crashed = command(&db, action, &args, Some(boundary));
        assert_eq!(
            crashed.status.code(),
            Some(86),
            "boundary {boundary}: {:?}",
            crashed
        );
        // Recovery/query runs in a NEW child process after the writer terminated without cleanup.
        let query = command(&db, "query", &["op"], None);
        assert!(query.status.success(), "{:?}", query);
        let committed = boundary == "after-commit";
        assert!(
            String::from_utf8(query.stdout)
                .unwrap()
                .starts_with(if committed {
                    "LocallyCommitted"
                } else {
                    "Absent"
                })
        );
        let check = command(&db, "check", &[], None);
        assert!(check.status.success(), "{:?}", check);
        let store = Store::open_existing(&db, EventBudget::default()).unwrap();
        assert_eq!(
            matches!(store.lookup("op").unwrap(), Lookup::Committed(_)),
            committed
        );
        assert_eq!(
            store.pending(0, 10).unwrap().len(),
            usize::from(rename) + usize::from(committed)
        );
        let card = store.card("card").unwrap();
        if committed {
            assert_eq!(card.unwrap().summary().title, "new");
        } else if rename {
            assert_eq!(card.unwrap().summary().title, "old");
        } else {
            assert!(card.is_none());
        }
        drop(store);
        // Same stable operation ID after ambiguous caller outcome: no duplicate event/revision.
        assert!(command(&db, action, &args, None).status.success());
        assert!(command(&db, action, &args, None).status.success());
        let store = Store::open_existing(&db, EventBudget::default()).unwrap();
        assert_eq!(store.pending(0, 10).unwrap().len(), usize::from(rename) + 1);
        assert_eq!(
            store.card("card").unwrap().unwrap().summary().revision,
            if rename { 2 } else { 1 }
        );
    }
}
#[test]
fn create_recovers_at_six_process_exit_boundaries() {
    scenario(false);
}
#[test]
fn rename_recovers_at_six_process_exit_boundaries() {
    scenario(true);
}
