#![cfg(all(not(target_arch = "wasm32"), feature = "fault-injection"))]
use morrow_core::{
    content::CardRecord,
    records::{Kind, Record},
    store::{EventBudget, Store},
};
use std::{path::Path, process::Command};
fn call(path: &Path, action: &str, args: &[&str], point: Option<&str>) -> std::process::Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_morrow-core-store"));
    c.arg(action).arg(path).args(args);
    if let Some(p) = point {
        c.env("MORROW_TEST_CRASH_AT", p);
    }
    c.output().unwrap()
}
#[test]
fn record_creation_and_draft_save_recover_at_twelve_exit_boundaries() {
    for draft in [false, true] {
        for point in [
            "record-after-begin",
            "record-after-content",
            "record-after-operation",
            "record-after-event",
            "record-before-commit",
            "record-after-commit",
        ] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("db");
            let body = dir.path().join("body");
            std::fs::write(&body, [3, 2, 1]).unwrap();
            let mut s = Store::open(&path, EventBudget::default()).unwrap();
            if draft {
                s.create_local(
                    "card",
                    &CardRecord::new("card", "type", 1, "", vec![]).unwrap(),
                )
                .unwrap();
                s.create_record_local(
                    "draft",
                    &Record::draft("d", "card", 1, "type", 1, vec![0]).unwrap(),
                )
                .unwrap();
            }
            drop(s);
            let (action, args, kind, id, revision) = if draft {
                (
                    "draft-save-local",
                    vec!["op", "d", "1", body.to_str().unwrap()],
                    Kind::Draft,
                    "d",
                    2,
                )
            } else {
                (
                    "workspace-local",
                    vec!["op", "w", "0", "title"],
                    Kind::Workspace,
                    "w",
                    1,
                )
            };
            let result = call(&path, action, &args, Some(point));
            assert_eq!(result.status.code(), Some(86), "{result:?}");
            assert!(call(&path, "check", &[], None).status.success());
            let s = Store::open_existing(&path, EventBudget::default()).unwrap();
            let committed = point == "record-after-commit";
            assert_eq!(
                s.lookup_record_local(kind, id, "op").unwrap().is_some(),
                committed
            );
            assert_eq!(
                s.record_local(kind, id).unwrap().map(|v| v.revision()),
                if committed {
                    Some(revision)
                } else if draft {
                    Some(1)
                } else {
                    None
                }
            );
            drop(s);
            for _ in 0..2 {
                let result = call(&path, action, &args, None);
                assert!(result.status.success(), "{result:?}");
            }
            let s = Store::open_existing(&path, EventBudget::default()).unwrap();
            assert_eq!(
                s.record_local(kind, id).unwrap().unwrap().revision(),
                revision
            );
            assert_eq!(s.pending(0, 10).unwrap().len(), if draft { 3 } else { 1 });
            s.integrity_check().unwrap();
        }
    }
}
