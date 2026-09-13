#![cfg(target_os = "windows")]
#[path = "../../tests/common/mod.rs"]
mod common;
use crate::{Workbench, query_plan::Conditions};
use morrow_core::{
    content::CardRecord,
    content_change::ContentChange,
    lifecycle::GrantKind,
    plugin_package::Package,
    store::{EventBudget, Store},
};
use morrow_workbench_plugin::{Idea, persistence};
use std::path::Path;

fn recovery_package() -> Package {
    let original = common::package();
    let mut manifest = original.manifest().clone();
    manifest.package_version = "0.1.9-test.44".into();
    Package::build(manifest, original.module()).unwrap()
}
fn conditions() -> Conditions {
    Conditions {
        section: "概览".into(),
        filter: "全部".into(),
        text: String::new(),
        sort: "最近添加".into(),
    }
}
fn seed(host: &mut Workbench, id: &str, title: &str, body: &str) {
    let mut idea = common::idea(id);
    idea.title = title.into();
    idea.description = body.into();
    host.host
        .store_local_mut()
        .create_local(&format!("seed-{id}"), &card(&idea))
        .unwrap();
}
fn card(idea: &Idea) -> CardRecord {
    CardRecord::new(
        &idea.id,
        "org.morrow.idea",
        1,
        &idea.title,
        persistence::encode(idea, None).unwrap(),
    )
    .unwrap()
}
fn replace(host: &mut Workbench, operation: &str, idea: Idea) {
    let prior = host.host.store_local().card(&idea.id).unwrap().unwrap();
    let change = ContentChange {
        operation_id: operation.into(),
        card_id: idea.id.clone(),
        expected_revision: prior.summary().revision,
        title: idea.title.clone(),
        body: persistence::encode(&idea, Some(&prior.body())).unwrap(),
        preview_text: String::new(),
        attachments: None,
    };
    host.grant(&idea.id, GrantKind::EditContent).unwrap();
    let connection = host
        .pool
        .root(host.plugin.as_ref().unwrap())
        .unwrap()
        .connection();
    let start = host.start;
    host.host
        .edit_content(connection, &change, || crate::now(start))
        .unwrap();
    host.revoke(&idea.id, GrantKind::EditContent).unwrap();
}
fn assert_release(mut host: Workbench, db: &Path) {
    assert!(
        Workbench::open(db, None).is_err(),
        "query must retain the Windows Session lease"
    );
    host.host.flush_pending().unwrap();
    assert_eq!(host.host.store_local().pending_usage().unwrap(), (0, 0));
    host.finish().unwrap();
    drop(host);
    let mut reopened = Workbench::open(db, None).unwrap();
    assert!(reopened.host.store_local().integrity_check().is_ok());
    reopened.finish().unwrap();
}

#[test]
fn frozen_query_keeps_old_set_body_and_sort_after_formal_writes() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("workbench.db");
    let mut host = Workbench::open(&db, Some(common::package())).unwrap();
    seed(&mut host, "a", "Zebra", "needle original");
    seed(&mut host, "b", "Apple", "needle original");
    seed(&mut host, "d", "Mango", "needle original");
    let snapshot = host.host.store_local().open_card_snapshot().unwrap();
    let mut a = host.read("a").unwrap().idea;
    a.description = "changed body".into();
    replace(&mut host, "edit-a", a);
    let mut b = host.read("b").unwrap().idea;
    b.title = "Zulu".into();
    replace(&mut host, "edit-b", b);
    let mut d = host.read("d").unwrap().idea;
    d.deleted = true;
    d.deleted_at = 1;
    replace(&mut host, "delete-d", d);
    seed(&mut host, "c", "Cherry", "needle new");
    let mut select = conditions();
    select.text = "needle".into();
    select.sort = "标题排序".into();
    let (old, census) = host.query_snapshot(snapshot, &select).unwrap();
    assert_eq!(old, ["b", "d", "a"]);
    assert_eq!(census.count, 3);
    assert_eq!(
        host.query("概览", "全部", "needle", "标题排序").unwrap(),
        ["c", "b"]
    );
    assert_eq!(host.read("a").unwrap().revision, 2);
    assert_release(host, &db);
}

#[test]
fn full_non_idea_page_is_counted_and_does_not_end_query() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("workbench.db");
    let mut host = Workbench::open(&db, Some(common::package())).unwrap();
    for i in 0..128 {
        let id = format!("a-{i:03}");
        host.host
            .store_local_mut()
            .create_local(
                &format!("seed-{id}"),
                &CardRecord::new(&id, "test.other", 1, "Other", vec![i as u8]).unwrap(),
            )
            .unwrap();
    }
    seed(&mut host, "z-idea", "Visible", "body");
    let snapshot = host.host.store_local().open_card_snapshot().unwrap();
    let (ids, census) = host.query_snapshot(snapshot, &conditions()).unwrap();
    assert_eq!(ids, ["z-idea"]);
    assert_eq!(census.count, 129);
    let (_, again) = host
        .query_snapshot(
            host.host.store_local().open_card_snapshot().unwrap(),
            &conditions(),
        )
        .unwrap();
    assert_eq!(census.sha256, again.sha256);
    assert_release(host, &db);
}

#[test]
fn foreign_invalid_and_capacity_failures_release_snapshot_without_poisoning_host() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("workbench.db");
    let mut host = Workbench::open(&db, Some(common::package())).unwrap();
    seed(&mut host, "a", "A", "body");
    let foreign = Store::open(&dir.path().join("foreign.db"), EventBudget::default()).unwrap();
    let snapshot = foreign.open_card_snapshot().unwrap();
    assert!(host.query_snapshot(snapshot, &conditions()).is_err());
    let mut invalid = conditions();
    invalid.text = "x".repeat(16385);
    assert!(
        host.query_snapshot(
            host.host.store_local().open_card_snapshot().unwrap(),
            &invalid
        )
        .is_err()
    );
    // Persisted content can exceed the guest's 64 KiB invocation frame.
    seed(
        &mut host,
        "oversized",
        &"Large".repeat(200),
        &"x".repeat(65000),
    );
    assert!(host.query("概览", "全部", "", "最近添加").is_err());
    assert!(
        host.writable(),
        "trusted capacity rejection is not a guest fault"
    );
    let mut small = host.read("oversized").unwrap().idea;
    small.description = "small".into();
    replace(&mut host, "shrink", small);
    assert_eq!(
        host.query("概览", "全部", "", "最近添加").unwrap(),
        ["oversized", "a"]
    );
    host.create("after-error", common::idea("saved")).unwrap();
    assert_release(host, &db);
}

fn trap_package() -> Package {
    let mut module = vec![
        0, 97, 115, 109, 1, 0, 0, 0, 1, 5, 1, 96, 0, 1, 127, 3, 2, 1, 0, 5, 3, 1, 0, 1, 7, 23, 2,
        10, b'm', b'o', b'r', b'r', b'o', b'w', b'_', b'r', b'u', b'n', 0, 0, 6, b'm', b'e', b'm',
        b'o', b'r', b'y', 2, 0,
    ];
    module.extend([10, 5, 1, 3, 0, 0, 11]);
    let good = common::package();
    let mut manifest = Package::manifest_for_transform(
        "org.morrow.workbench",
        "0.1.9-test.43",
        &module,
        good.manifest().transform_handlers.clone(),
    );
    manifest.requested_capabilities = good.manifest().requested_capabilities.clone();
    Package::build(manifest, &module).unwrap()
}

#[test]
fn actual_guest_trap_releases_snapshot_and_explicit_restart_restores_query() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("workbench.db");
    let mut host = Workbench::open(&db, Some(trap_package())).unwrap();
    seed(&mut host, "card", "Card", "body");
    assert!(host.query("概览", "全部", "", "最近添加").is_err());
    assert!(!host.writable());
    assert_eq!(host.read("card").unwrap().revision, 1);
    seed(
        &mut host,
        "trusted",
        "Trusted",
        "host-local write after failure",
    );
    assert!(Workbench::open(&db, None).is_err());
    host.host.flush_pending().unwrap();
    host.finish().unwrap();
    drop(host);
    let mut host = Workbench::open(&db, Some(recovery_package())).unwrap();
    let status = host.plugin_status();
    host.configure_plugin(status.revision, &status.digest, true)
        .unwrap();
    assert_eq!(
        host.query("概览", "全部", "", "最近添加").unwrap(),
        ["trusted", "card"]
    );
    host.create("after-restart", common::idea("saved")).unwrap();
    assert_release(host, &db);
}

#[test]
fn empty_store_still_requires_current_read_capability() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("workbench.db");
    let original = common::package();
    let mut manifest = original.manifest().clone();
    manifest
        .requested_capabilities
        .retain(|c| *c != morrow_core::plugin_package::proto::Capability::ReadContent as i32);
    let package = Package::build(manifest, original.module()).unwrap();
    let mut host = Workbench::open(&db, Some(package)).unwrap();
    assert!(host.query("概览", "全部", "", "最近添加").is_err());
    host.host.flush_pending().unwrap();
    host.finish().unwrap();
    drop(host);
    let mut host = Workbench::open(&db, Some(recovery_package())).unwrap();
    let status = host.plugin_status();
    host.configure_plugin(status.revision, &status.digest, true)
        .unwrap();
    assert!(
        host.query("概览", "全部", "", "最近添加")
            .unwrap()
            .is_empty()
    );
    assert_release(host, &db);
}
