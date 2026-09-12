use morrow_core::plugin_package::{
    Package,
    proto::{Capability, TransformHandler},
};
use morrow_workbench_host::Workbench;
use morrow_workbench_plugin::{Action, Idea};
fn package() -> Package {
    let path = std::env::var("MORROW_WORKBENCH_WASM")
        .expect("set MORROW_WORKBENCH_WASM to the actual compiled Rust guest");
    let module = std::fs::read(path).unwrap();
    let mut manifest = Package::manifest_for_transform(
        "org.morrow.workbench",
        "0.1.9-test.10",
        &module,
        vec![
            TransformHandler {
                handler: "studio.command".into(),
                input_type: "morrow.studio.request.v1".into(),
                output_type: "morrow.studio.response.v1".into(),
                max_input_bytes: 65536,
                max_output_bytes: 65536,
            },
            TransformHandler {
                handler: "workbench.command".into(),
                input_type: "morrow.workbench.request.v1".into(),
                output_type: "morrow.workbench.response.v1".into(),
                max_input_bytes: 65536,
                max_output_bytes: 65536,
            },
        ],
    );
    manifest.requested_capabilities = vec![
        Capability::CreateContent as i32,
        Capability::EditContent as i32,
        Capability::ReadContent as i32,
    ];
    Package::build(manifest, &module).unwrap()
}
fn idea(id: &str) -> Idea {
    Idea {
        id: id.into(),
        title: format!("记录 {id}"),
        description: "原样 **Markdown**".into(),
        category: "灵感".into(),
        stage: "待整理".into(),
        ..Default::default()
    }
}
#[test]
fn persistent_guest_host_attachment_and_missing_plugin_readback() {
    let d = tempfile::tempdir().unwrap();
    let db = d.path().join("workbench.db");
    let mut h = Workbench::open(&db, Some(package())).unwrap();
    let bytes = b"original office attachment \0\xff";
    let asset = h
        .import(
            "a",
            "原稿.docx",
            "file",
            &mut &bytes[..],
            bytes.len() as u64,
        )
        .unwrap();
    let mut draft = idea("a");
    draft.assets = vec![asset.clone()];
    let saved = h.create("create-a", draft).unwrap();
    assert_eq!(saved.revision, 1);
    let mut invalid = idea("b");
    invalid.assets = vec![asset.clone()];
    assert!(h.create("cross-card", invalid).is_err());
    assert!(h.read("b").is_err());
    let fav = h
        .apply(morrow_workbench_host::Mutation {
            operation: "fav",
            id: "a",
            revision: 1,
            action: Action::Favorite,
            proposed: None,
            text: "",
            flag: true,
        })
        .unwrap();
    assert!(fav.idea.favorite);
    assert!(
        h.apply(morrow_workbench_host::Mutation {
            operation: "stale",
            id: "a",
            revision: 1,
            action: Action::Favorite,
            proposed: None,
            text: "",
            flag: false
        })
        .is_err()
    );
    let mut exported = Vec::new();
    h.export("a", &asset.id, &mut exported).unwrap();
    assert_eq!(exported, bytes);
    let deleted = h
        .apply(morrow_workbench_host::Mutation {
            operation: "delete",
            id: "a",
            revision: 2,
            action: Action::Delete,
            proposed: None,
            text: "",
            flag: false,
        })
        .unwrap();
    assert!(deleted.idea.deleted);
    assert!(h.query("概览", "全部", "", "最近添加").unwrap().is_empty());
    let restored = h
        .apply(morrow_workbench_host::Mutation {
            operation: "undo",
            id: "a",
            revision: 3,
            action: Action::Restore,
            proposed: None,
            text: "",
            flag: false,
        })
        .unwrap();
    assert!(!restored.idea.deleted);
    h.apply(morrow_workbench_host::Mutation {
        operation: "delete-again",
        id: "a",
        revision: 4,
        action: Action::Delete,
        proposed: None,
        text: "",
        flag: false,
    })
    .unwrap();
    drop(h);
    let mut reopened = Workbench::open(&db, Some(package())).unwrap();
    assert!(
        reopened
            .apply(morrow_workbench_host::Mutation {
                operation: "late-undo",
                id: "a",
                revision: 5,
                action: Action::Restore,
                proposed: None,
                text: "",
                flag: false
            })
            .is_err()
    );
    drop(reopened);
    let readonly = Workbench::open(&db, None).unwrap();
    assert!(!readonly.writable());
    let saved = readonly.read("a").unwrap();
    assert_eq!(saved.revision, 5);
    assert!(saved.idea.favorite);
    assert!(saved.idea.deleted);
    let mut exported = Vec::new();
    readonly.export("a", &asset.id, &mut exported).unwrap();
    assert_eq!(exported, bytes);
}
#[test]
fn query_pages_beyond_one_message_and_sorts_globally() {
    let d = tempfile::tempdir().unwrap();
    let mut h = Workbench::open(&d.path().join("db"), Some(package())).unwrap();
    for i in 0..140 {
        let id = format!("idea-{i:03}");
        h.create(&format!("create-{i}"), idea(&id)).unwrap();
    }
    let all = h.query("概览", "全部", "", "最近添加").unwrap();
    assert_eq!(all.len(), 140);
    assert_eq!(all.first().unwrap(), "idea-139");
    let title = h.query("概览", "全部", "", "标题排序").unwrap();
    assert_eq!(title.first().unwrap(), "idea-000");
    assert_eq!(title.last().unwrap(), "idea-139");
    h.apply(morrow_workbench_host::Mutation {
        operation: "favorite",
        id: "idea-010",
        revision: 1,
        action: Action::Favorite,
        proposed: None,
        text: "",
        flag: true,
    })
    .unwrap();
    let fav = h.query("概览", "全部", "", "收藏优先").unwrap();
    assert_eq!(fav[0], "idea-010");
    assert_eq!(fav[1], "idea-139");
}
