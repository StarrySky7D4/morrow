#![cfg(all(target_os = "windows", feature = "fault-injection"))]
use morrow_audit::{
    library::{Error, Registry},
    snapshot,
};
use morrow_core::content::CardRecord;
use std::path::Path;
fn prepare(root: &Path) {
    let mut registry = Registry::open(root).unwrap();
    let mut session = registry.open_session(Default::default()).unwrap();
    session
        .runtime()
        .store_local_mut()
        .create_local(
            "original-op",
            &CardRecord::new("original", "text", 1, "preserved", vec![1]).unwrap(),
        )
        .unwrap();
    session.backup_snapshot(&root.join("archive")).unwrap();
    snapshot::restore(&root.join("archive"), &root.join("copy")).unwrap();
}
#[test]
fn child() {
    let Ok(root) = std::env::var("MORROW_LIBRARY_TEST_ROOT") else {
        return;
    };
    let root = Path::new(&root);
    let mut registry = Registry::open(root).unwrap();
    let result = registry.activate(&root.join("copy"));
    if std::env::var("MORROW_AUDIT_CRASH_AT").as_deref() == Ok("library-publication-uncertain") {
        assert!(matches!(result, Err(Error::PublishUnknown)));
        assert!(matches!(
            registry.selected_database(),
            Err(Error::PublishUnknown)
        ));
        assert!(matches!(
            registry.open_session(Default::default()),
            Err(Error::PublishUnknown)
        ));
        assert!(matches!(
            registry.activate(root),
            Err(Error::PublishUnknown)
        ));
    } else {
        result.unwrap();
    }
}
#[test]
fn publication_crashes_route_to_one_verified_library_and_preserve_source() {
    for point in [
        "library-before-publish",
        "library-after-publish",
        "library-publication-uncertain",
    ] {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().canonicalize().unwrap();
        prepare(&root);
        let original = std::fs::read(root.join("workbench.db")).unwrap();
        let key = std::fs::read(root.join("workbench.db.audit-key")).unwrap();
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "child", "--nocapture"])
            .env("MORROW_LIBRARY_TEST_ROOT", &root)
            .env("MORROW_AUDIT_CRASH_AT", point)
            .output()
            .unwrap();
        assert_eq!(
            child.status.code(),
            Some(if point == "library-publication-uncertain" {
                0
            } else {
                86
            }),
            "{}",
            String::from_utf8_lossy(&child.stderr)
        );
        assert_eq!(std::fs::read(root.join("workbench.db")).unwrap(), original);
        assert_eq!(
            std::fs::read(root.join("workbench.db.audit-key")).unwrap(),
            key
        );
        let mut registry = Registry::open(&root).unwrap();
        let changed = point != "library-before-publish";
        assert_eq!(registry.generation(), if changed { 2 } else { 1 });
        assert_eq!(
            registry.selected_database().unwrap(),
            root.join(if changed {
                "copy/workbench.db"
            } else {
                "workbench.db"
            })
        );
        let session = registry.open_session(Default::default()).unwrap();
        assert_eq!(
            session
                .store()
                .card("original")
                .unwrap()
                .unwrap()
                .summary()
                .title,
            "preserved"
        );
    }
}
