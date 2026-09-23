#![cfg(target_os = "windows")]
use morrow_audit::{
    keys::Key,
    session::{OpenMode, Session},
};
use morrow_core::{
    content::CardRecord,
    lifecycle::GrantKind,
    plugin_package::{
        Package,
        proto::{Capability, TransformHandler},
    },
    store::{EventBudget, Store},
};
use morrow_workbench_plugin::{Idea, persistence};
use std::{fs, path::Path, process::Command};

fn files(root: &Path) -> std::collections::BTreeMap<String, (u64, String)> {
    use sha2::{Digest, Sha256};
    fs::read_dir(root)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            let path = entry.path();
            let raw = fs::read(&path).unwrap();
            (
                path.file_name().unwrap().to_string_lossy().into_owned(),
                (raw.len() as u64, format!("{:x}", Sha256::digest(&raw))),
            )
        })
        .collect()
}
fn cli(source: &Path, extra: &[&str]) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_morrow-migrate-library"));
    command.arg("--source").arg(source);
    command.args(extra);
    command.output().unwrap()
}
fn source() -> (tempfile::TempDir, std::path::PathBuf, Vec<u8>) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("source");
    fs::create_dir(&root).unwrap();
    let db = root.join("workbench.db");
    let mut session = Session::open(&db, EventBudget::default(), OpenMode::Initialize).unwrap();
    let body = persistence::encode(
        &Idea {
            id: "test-card".into(),
            title: "First".into(),
            category: "进行中".into(),
            stage: "计划中".into(),
            todos: vec!["same".into(), "same".into()],
            completed: vec!["same".into()],
            ..Default::default()
        },
        None,
    )
    .unwrap();
    let card = CardRecord::new("test-card", "org.morrow.idea", 1, "First", body).unwrap();
    let raw = card.encode();
    let host = session.runtime();
    let mut connection = host.connect().unwrap();
    host.grant(
        &mut connection,
        GrantKind::CreateContent,
        "test-card",
        10_000,
        0,
    )
    .unwrap();
    host.create_content(&connection, "seed", &card, || 1)
        .unwrap();
    host.disconnect(&connection).unwrap();
    session.flush(16).unwrap();
    drop(session);
    (dir, db, raw)
}
#[test]
fn preflight_is_read_only_and_rejects_active_source() {
    let (dir, db, original) = source();
    let key = Key::load(&db.with_file_name("workbench.db.audit-key")).unwrap();
    let source_bytes = files(db.parent().unwrap());
    let output = cli(&db, &[]);
    assert_eq!(files(db.parent().unwrap()), source_bytes);
    assert!(
        output.status.success(),
        "{:?}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("V1 任务卡: 1"));
    let audited = Store::open_read_only_audited(&db, key.trust()).unwrap();
    assert_eq!(
        audited.card("test-card").unwrap().unwrap().encode(),
        original
    );
    drop(audited);
    let session = Session::open(&db, EventBudget::default(), OpenMode::Existing).unwrap();
    let blocked = cli(&db, &[]);
    assert!(!blocked.status.success());
    assert!(String::from_utf8_lossy(&blocked.stderr).contains("active"));
    drop(session);
    assert!(!dir.path().join("target").exists());
}
#[test]
fn copy_migrates_v1_and_preserves_source_and_v2_history() {
    let Ok(wasm_path) = std::env::var("MORROW_WORKBENCH_WASM") else {
        eprintln!("MORROW_WORKBENCH_WASM unavailable; real guest migration case skipped");
        return;
    };
    let (dir, db, original) = source();
    let module = fs::read(wasm_path).unwrap();
    let mut manifest = Package::manifest_for_transform(
        "org.morrow.workbench",
        morrow_workbench_plugin::PACKAGE_VERSION,
        &module,
        vec![TransformHandler {
            handler: "workbench.tasks.v2".into(),
            input_type: "morrow.workbench.tasks.request.v2".into(),
            output_type: "morrow.workbench.tasks.response.v2".into(),
            max_input_bytes: 65536,
            max_output_bytes: 65536,
        }],
    );
    manifest.requested_capabilities = vec![
        Capability::CreateContent as i32,
        Capability::EditContent as i32,
        Capability::ReadContent as i32,
    ];
    let package = Package::build(manifest, &module).unwrap();
    let package_path = dir.path().join("workbench.morrowplugin");
    fs::write(&package_path, package.archive()).unwrap();
    let target = dir.path().join("target");
    let source_bytes = files(db.parent().unwrap());
    let output = Command::new(env!("CARGO_BIN_EXE_morrow-migrate-library"))
        .arg("--source")
        .arg(&db)
        .arg("--package")
        .arg(&package_path)
        .arg("--apply")
        .arg("--output")
        .arg(&target)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(files(db.parent().unwrap()), source_bytes);
    let source_key = Key::load(&db.with_file_name("workbench.db.audit-key")).unwrap();
    let untouched = Store::open_read_only_audited(&db, source_key.trust()).unwrap();
    assert_eq!(
        untouched.card("test-card").unwrap().unwrap().encode(),
        original
    );
    let migrated =
        Store::open_read_only_audited(&target.join("workbench.db"), source_key.trust()).unwrap();
    let card = migrated.card("test-card").unwrap().unwrap();
    assert_eq!(card.summary().format_version, 2);
    assert_eq!(card.summary().revision, 2);
    let body =
        morrow_workbench_plugin::tasks_v2::decode("test-card", "First", &card.body()).unwrap();
    assert_eq!(
        body.origin.unwrap().original_properties,
        CardRecord::decode(&original).unwrap().body()
    );
    assert!(
        cli(
            &db,
            &[
                "--apply",
                "--output",
                target.to_str().unwrap(),
                "--package",
                package_path.to_str().unwrap()
            ]
        )
        .status
        .success()
            == false
    );
}

#[test]
fn imports_legacy_json_cards_preferences_and_local_media() {
    let Ok(wasm_path) = std::env::var("MORROW_WORKBENCH_WASM") else {
        eprintln!("MORROW_WORKBENCH_WASM unavailable; JSON migration case skipped");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let legacy = dir.path().join("legacy");
    fs::create_dir(&legacy).unwrap();
    let photo = legacy.join("photo.png");
    fs::write(&photo, b"real image fixture bytes").unwrap();
    let snapshot = serde_json::json!({
        "version": 1,
        "ideas": [{
            "id": "json-card", "title": "Old title", "description": "Old text",
            "category": "进行中", "time": "刚刚", "icon": 0, "color": 4278190080u32,
            "favorite": true, "todos": ["same","same"], "completed": ["same"],
            "stage": "计划中", "hypothesis": "", "conclusion": "",
            "attachments": [{
                "source": {"location": photo.to_str().unwrap(), "name": "photo.png", "kind": "image", "local": true},
                "size": fs::metadata(&photo).unwrap().len()
            }]
        }],
        "theme": "white", "glass": "frosted", "background": "ambient",
        "uiLocale": "en", "uiFont": {"family": "", "asset": "", "name": ""},
        "texture": {"location": photo.to_str().unwrap(), "name": "photo.png", "kind": "image", "local": true},
        "music": {"index": 0, "tracks": []}, "completed": ["daily"]
    });
    let raw =
        serde_json::to_vec(&serde_json::json!({"flutter.daemon.studio.v1": snapshot.to_string()}))
            .unwrap();
    let source = legacy.join("export.json");
    fs::write(&source, &raw).unwrap();
    let module = fs::read(wasm_path).unwrap();
    let handlers = [
        (
            "workbench.tasks.v2",
            "morrow.workbench.tasks.request.v2",
            "morrow.workbench.tasks.response.v2",
        ),
        (
            "workbench.command",
            "morrow.workbench.request.v1",
            "morrow.workbench.response.v1",
        ),
        (
            "studio.preferences",
            "morrow.studio.preferences.v1",
            "morrow.studio.preferences.v1",
        ),
    ]
    .into_iter()
    .map(|(handler, input_type, output_type)| TransformHandler {
        handler: handler.into(),
        input_type: input_type.into(),
        output_type: output_type.into(),
        max_input_bytes: 65536,
        max_output_bytes: 65536,
    })
    .collect();
    let mut manifest = Package::manifest_for_transform(
        "org.morrow.workbench",
        morrow_workbench_plugin::PACKAGE_VERSION,
        &module,
        handlers,
    );
    manifest.requested_capabilities = vec![
        Capability::CreateContent as i32,
        Capability::EditContent as i32,
        Capability::ReadContent as i32,
    ];
    let package = Package::build(manifest, &module).unwrap();
    let package_path = dir.path().join("bundle.morrowplugin");
    fs::write(&package_path, package.archive()).unwrap();
    let inspected = cli(&source, &[]);
    assert!(
        inspected.status.success(),
        "{}",
        String::from_utf8_lossy(&inspected.stderr)
    );
    let target = dir.path().join("converted");
    let output = Command::new(env!("CARGO_BIN_EXE_morrow-migrate-library"))
        .arg("--source")
        .arg(&source)
        .arg("--package")
        .arg(&package_path)
        .arg("--apply")
        .arg("--output")
        .arg(&target)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(fs::read(&source).unwrap(), raw);
    assert_eq!(fs::read(&photo).unwrap(), b"real image fixture bytes");
    assert_eq!(
        fs::read(target.join("legacy-shared_preferences.json")).unwrap(),
        raw
    );
    let key = Key::load(&target.join("workbench.db.audit-key")).unwrap();
    let store = Store::open_read_only_audited(&target.join("workbench.db"), key.trust()).unwrap();
    let card = store.card("json-card").unwrap().unwrap();
    assert_eq!(card.summary().format_version, 2);
    assert_eq!(card.attachments().len(), 1);
    let properties =
        morrow_workbench_plugin::tasks_v2::decode("json-card", "Old title", &card.body()).unwrap();
    assert_eq!(properties.origin.unwrap().original_title, "Old title");
    assert!(target.join("migration-report.txt").is_file());
    assert!(
        target
            .join("legacy-media")
            .read_dir()
            .unwrap()
            .next()
            .is_some()
    );
}

#[test]
fn marked_incomplete_target_cannot_start_ordinary_host() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("MIGRATION_INCOMPLETE.txt"), "unfinished").unwrap();
    let db = dir.path().join("workbench.db");
    let result = Command::new(env!("CARGO_BIN_EXE_morrow-workbench-host"))
        .arg(&db)
        .arg(dir.path().join("missing.morrowplugin"))
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(!db.exists());
}
#[test]
fn unknown_legacy_field_is_rejected_during_read_only_preflight() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("snapshot.json");
    fs::write(
        &source,
        serde_json::to_vec(&serde_json::json!({
            "version": 1, "ideas": [], "futureSetting": "preserve"
        }))
        .unwrap(),
    )
    .unwrap();
    let original = fs::read(&source).unwrap();
    let result = cli(&source, &[]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("unmapped"));
    assert_eq!(fs::read(&source).unwrap(), original);
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn missing_legacy_attachment_blocks_preflight_without_creating_target() {
    let dir = tempfile::tempdir().unwrap();
    let absent = dir.path().join("missing.png");
    let snapshot = serde_json::json!({
        "version": 1,
        "ideas": [{
            "id":"broken","title":"Broken","description":"","category":"灵感",
            "time":"刚刚","icon":0,"color":4278190080u32,"favorite":false,
            "todos":[],"completed":[],"attachments":[{
                "source":{"location":absent.to_str().unwrap(),"name":"missing.png","kind":"image","local":true},
                "size":1
            }]
        }]
    });
    let source = dir.path().join("snapshot.json");
    fs::write(&source, snapshot.to_string()).unwrap();
    let result = cli(&source, &[]);
    assert!(!result.status.success());
    assert!(!result.stderr.is_empty());
    assert!(!dir.path().join("target").exists());
}
#[test]
fn failed_json_import_keeps_marked_target_for_inspection() {
    let Ok(wasm_path) = std::env::var("MORROW_WORKBENCH_WASM") else {
        eprintln!("MORROW_WORKBENCH_WASM unavailable; failure retention case skipped");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("snapshot.json");
    fs::write(
        &source,
        serde_json::json!({
            "version":1,
            "ideas":[{
                "id":"card","title":"Title","description":"","category":"灵感",
                "time":"刚刚","icon":0,"color":4278190080u32,"favorite":false,
                "todos":[],"completed":[],"attachments":[]
            }]
        })
        .to_string(),
    )
    .unwrap();
    let module = fs::read(wasm_path).unwrap();
    let mut manifest = Package::manifest_for_transform(
        "org.morrow.workbench",
        morrow_workbench_plugin::PACKAGE_VERSION,
        &module,
        vec![TransformHandler {
            handler: "workbench.tasks.v2".into(),
            input_type: "morrow.workbench.tasks.request.v2".into(),
            output_type: "morrow.workbench.tasks.response.v2".into(),
            max_input_bytes: 65536,
            max_output_bytes: 65536,
        }],
    );
    manifest.requested_capabilities = vec![
        Capability::CreateContent as i32,
        Capability::EditContent as i32,
        Capability::ReadContent as i32,
    ];
    let package = Package::build(manifest, &module).unwrap();
    let package_path = dir.path().join("incomplete-package.morrowplugin");
    fs::write(&package_path, package.archive()).unwrap();
    let target = dir.path().join("target");
    let output = Command::new(env!("CARGO_BIN_EXE_morrow-migrate-library"))
        .arg("--source")
        .arg(&source)
        .arg("--package")
        .arg(&package_path)
        .arg("--apply")
        .arg("--output")
        .arg(&target)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(target.join("MIGRATION_INCOMPLETE.txt").is_file());
    assert!(target.join("workbench.db").is_file());
    let blocked = cli(&target, &[]);
    assert!(!blocked.status.success());
    assert!(String::from_utf8_lossy(&blocked.stderr).contains("incomplete migration target"));
    assert_eq!(fs::read_to_string(&source).unwrap().contains("Title"), true);
}

fn tree_files(root: &Path) -> std::collections::BTreeMap<String, (u64, String)> {
    use sha2::{Digest, Sha256};
    fn visit(root: &Path, dir: &Path, out: &mut std::collections::BTreeMap<String, (u64, String)>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(root, &path, out);
            } else {
                let raw = fs::read(&path).unwrap();
                let relative = path
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned();
                out.insert(
                    relative,
                    (raw.len() as u64, format!("{:x}", Sha256::digest(&raw))),
                );
            }
        }
    }
    let mut out = std::collections::BTreeMap::new();
    visit(root, root, &mut out);
    out
}

fn operation_ids(db: &Path) -> std::collections::BTreeSet<String> {
    let connection =
        rusqlite::Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    let mut query = connection
        .prepare("SELECT id FROM operations ORDER BY id")
        .unwrap();
    query
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

#[test]
fn sqlite_media_font_and_operation_history_survive_two_migrations() {
    use morrow_workbench_host::{Workbench, legacy_json_preferences};
    use sha2::{Digest, Sha256};
    let wasm_path = std::env::var("MORROW_WORKBENCH_WASM")
        .expect("real Wasm required for SQLite media migration fixture");
    let (dir, db, original_card) = source();
    let original_root = db.parent().unwrap();
    let texture = original_root.join("texture.png");
    let song = original_root.join("song.mp3");
    fs::write(&texture, b"native texture bytes").unwrap();
    fs::write(&song, b"native music bytes").unwrap();
    let font_bytes = b"native fixture font bytes 0123456789";
    let font_sha = format!("{:x}", Sha256::digest(font_bytes));
    fs::create_dir(original_root.join("fonts")).unwrap();
    fs::write(
        original_root.join("fonts").join(format!("{font_sha}.sfnt")),
        font_bytes,
    )
    .unwrap();
    let module = fs::read(wasm_path).unwrap();
    let handlers = [
        (
            "workbench.tasks.v2",
            "morrow.workbench.tasks.request.v2",
            "morrow.workbench.tasks.response.v2",
        ),
        (
            "studio.preferences",
            "morrow.studio.preferences.v1",
            "morrow.studio.preferences.v1",
        ),
    ]
    .into_iter()
    .map(|(handler, input_type, output_type)| TransformHandler {
        handler: handler.into(),
        input_type: input_type.into(),
        output_type: output_type.into(),
        max_input_bytes: 65536,
        max_output_bytes: 65536,
    })
    .collect();
    let mut manifest = Package::manifest_for_transform(
        "org.morrow.workbench",
        morrow_workbench_plugin::PACKAGE_VERSION,
        &module,
        handlers,
    );
    manifest.requested_capabilities = vec![
        Capability::CreateContent as i32,
        Capability::EditContent as i32,
        Capability::ReadContent as i32,
    ];
    let package = Package::build(manifest, &module).unwrap();
    let package_path = dir.path().join("workbench.morrowplugin");
    fs::write(&package_path, package.archive()).unwrap();
    let old_settings = serde_json::json!({
        "version": 1, "ideas": [], "uiLocale": "en",
        "uiFont": {"family": "", "asset": font_sha, "name": "Fixture Font"},
        "texture": {"location": texture.to_str().unwrap(), "name": "texture.png", "kind": "image", "local": true},
        "music": {"index": 0, "tracks": [{
            "source": {"location": song.to_str().unwrap(), "name": "song.mp3", "kind": "audio", "local": true},
            "trackTitle": "Fixture song"
        }]}
    });
    let mut source_host = Workbench::open(&db, Some(package)).unwrap();
    legacy_json_preferences::apply_preferences(&mut source_host, &old_settings).unwrap();
    legacy_json_preferences::verify_preferences(&source_host, &old_settings).unwrap();
    source_host.finish().unwrap();
    drop(source_host);
    let source_operations = operation_ids(&db);
    let source_files = tree_files(original_root);
    assert!(!source_operations.is_empty());
    let first = dir.path().join("first-migration");
    let first_result = cli(
        &db,
        &[
            "--package",
            package_path.to_str().unwrap(),
            "--apply",
            "--output",
            first.to_str().unwrap(),
        ],
    );
    assert!(
        first_result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&first_result.stdout),
        String::from_utf8_lossy(&first_result.stderr)
    );
    assert_eq!(tree_files(original_root), source_files);
    let first_db = first.join("workbench.db");
    let first_key = Key::load(&first.join("workbench.db.audit-key")).unwrap();
    let first_store = Store::open_read_only_audited(&first_db, first_key.trust()).unwrap();
    let first_card = first_store.card("test-card").unwrap().unwrap();
    assert_eq!(first_card.summary().format_version, 2);
    assert_eq!(first_card.summary().revision, 2);
    assert_eq!(first_card.summary().type_id, "org.morrow.idea");
    assert_ne!(first_card.encode(), original_card);
    let first_preferences = first_store
        .card("morrow-studio-preferences")
        .unwrap()
        .unwrap();
    let first_value =
        morrow_workbench_plugin::preferences::decode_persistent(&first_preferences.body()).unwrap();
    let first_texture = first_value.texture.unwrap();
    let first_song = first_value.tracks[0].source.as_ref().unwrap();
    for (uri, expected) in [
        (&first_texture.location, b"native texture bytes".as_slice()),
        (&first_song.location, b"native music bytes".as_slice()),
    ] {
        let path = url::Url::parse(uri).unwrap().to_file_path().unwrap();
        assert!(
            path.canonicalize()
                .unwrap()
                .starts_with(first.join("legacy-media").canonicalize().unwrap())
        );
        assert_eq!(fs::read(path).unwrap(), expected);
    }
    assert_eq!(
        fs::read(first.join("fonts").join(format!("{font_sha}.sfnt"))).unwrap(),
        font_bytes
    );
    drop(first_store);
    let first_operations = operation_ids(&first_db);
    assert!(source_operations.is_subset(&first_operations));
    assert!(first_operations.len() > source_operations.len());
    let first_files = tree_files(&first);
    let second = dir.path().join("second-migration");
    let second_result = cli(
        &first_db,
        &[
            "--package",
            package_path.to_str().unwrap(),
            "--apply",
            "--output",
            second.to_str().unwrap(),
        ],
    );
    assert!(
        second_result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&second_result.stdout),
        String::from_utf8_lossy(&second_result.stderr)
    );
    assert_eq!(tree_files(&first), first_files);
    assert_eq!(tree_files(original_root), source_files);
    let second_db = second.join("workbench.db");
    let second_key = Key::load(&second.join("workbench.db.audit-key")).unwrap();
    let second_store = Store::open_read_only_audited(&second_db, second_key.trust()).unwrap();
    let second_card = second_store.card("test-card").unwrap().unwrap();
    assert_eq!(second_card.encode(), first_card.encode());
    let second_preferences = second_store
        .card("morrow-studio-preferences")
        .unwrap()
        .unwrap();
    let second_value =
        morrow_workbench_plugin::preferences::decode_persistent(&second_preferences.body())
            .unwrap();
    let second_texture = second_value.texture.unwrap();
    let second_song = second_value.tracks[0].source.as_ref().unwrap();
    for (uri, expected) in [
        (&second_texture.location, b"native texture bytes".as_slice()),
        (&second_song.location, b"native music bytes".as_slice()),
    ] {
        let path = url::Url::parse(uri).unwrap().to_file_path().unwrap();
        assert!(
            path.canonicalize()
                .unwrap()
                .starts_with(second.join("legacy-media").canonicalize().unwrap())
        );
        assert_eq!(fs::read(path).unwrap(), expected);
    }
    assert_eq!(
        fs::read(second.join("fonts").join(format!("{font_sha}.sfnt"))).unwrap(),
        font_bytes
    );
    let second_operations = operation_ids(&second_db);
    assert!(first_operations.is_subset(&second_operations));
    assert!(second_operations.len() > first_operations.len());
}
