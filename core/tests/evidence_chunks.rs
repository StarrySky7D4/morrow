#![cfg(not(target_arch = "wasm32"))]
use morrow_core::{
    audit::{self, SigningKey, TrustedLog},
    content::CardRecord,
    dispatch::{Connection, HostRuntime},
    lifecycle::GrantKind,
    plugin_package::{Package, proto::TransformHandler},
    store::Store,
    task::{Invocation, Transform},
    task_evidence::{
        self, Evidence,
        proto::{ExecutionBudget, TaskEvidence},
    },
    transaction::Lookup,
};
use prost::Message;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};
const BLOCK: usize = 32768;
// Independent wire fixture for the persisted version-1 recipe and chunk contracts.
#[derive(Clone, PartialEq, Message)]
struct Recipe {
    #[prost(uint32, tag = "1")]
    version: u32,
    #[prost(bytes = "vec", tag = "2")]
    evidence: Vec<u8>,
    #[prost(bytes = "vec", tag = "3")]
    container: Vec<u8>,
    #[prost(uint64, tag = "4")]
    length: u64,
    #[prost(bytes = "vec", repeated, tag = "5")]
    chunks: Vec<Vec<u8>>,
}
#[derive(Clone, PartialEq, Message)]
struct Chunk {
    #[prost(uint32, tag = "1")]
    version: u32,
    #[prost(bytes = "vec", tag = "2")]
    digest: Vec<u8>,
    #[prost(bytes = "vec", tag = "3")]
    payload: Vec<u8>,
}
fn pack(magic: &[u8; 8], raw: &[u8]) -> Vec<u8> {
    let compressed = lz4_flex::block::compress(raw);
    let mut bytes = magic.to_vec();
    bytes.extend(1u16.to_le_bytes());
    bytes.extend((raw.len() as u32).to_le_bytes());
    bytes.extend((compressed.len() as u32).to_le_bytes());
    bytes.extend(Sha256::digest(raw));
    bytes.extend(compressed);
    bytes
}
fn unpack(bytes: &[u8], magic: &[u8; 8]) -> Vec<u8> {
    assert_eq!(&bytes[..8], magic);
    assert_eq!(u16::from_le_bytes(bytes[8..10].try_into().unwrap()), 1);
    let raw = lz4_flex::block::decompress(
        &bytes[50..],
        u32::from_le_bytes(bytes[10..14].try_into().unwrap()) as usize,
    )
    .unwrap();
    assert_eq!(Sha256::digest(&raw).as_slice(), &bytes[18..50]);
    raw
}
// Synthetic observations exercise persistence only. These bytes are not evidence of actual guest execution.
fn evidence(label: &str, module_bytes: usize) -> Evidence {
    let mut module = b"\0asm\x01\0\0\0".to_vec();
    let mut random = 0x81234567u32;
    for _ in module.len()..module_bytes {
        random ^= random << 13;
        random ^= random >> 17;
        random ^= random << 5;
        module.push(random as u8);
    }
    let p = Package::build(
        Package::manifest_for_transform(
            "test.evidence",
            "1.0.0",
            &module,
            vec![TransformHandler {
                handler: "convert".into(),
                input_type: "bytes".into(),
                output_type: "bytes".into(),
                max_input_bytes: 65536,
                max_output_bytes: 65536,
            }],
        ),
        &module,
    )
    .unwrap();
    let input = Invocation::new_transform(
        label,
        Transform {
            handler: "convert".into(),
            input_type: "bytes".into(),
            output_type: "bytes".into(),
            input: b"synthetic".to_vec(),
        },
    )
    .unwrap();
    task_evidence::encode(TaskEvidence {
        schema_version: 1,
        package_archive: p.archive().to_vec(),
        invocation: input.bytes().to_vec(),
        budget: Some(ExecutionBudget {
            fuel: 100000,
            memory_bytes: 65536,
            host_calls: 4,
        }),
        backend: task_evidence::BACKEND.into(),
        completion: input.output_completion(b"synthetic output").unwrap(),
        fault: 0,
        exit_code: Some(0),
        observed_host_calls: 0,
        fuel_remaining: 90000,
        batch: None,
    })
    .unwrap()
}

fn card(id: &str) -> CardRecord {
    CardRecord::new(id, "test.card", 1, "original", b"original".to_vec()).unwrap()
}
fn connection(h: &mut HostRuntime, id: &str) -> Connection {
    let mut c = h.connect().unwrap();
    h.grant(&mut c, GrantKind::CreateContent, id, 100, 0)
        .unwrap();
    c
}
fn initialized(path: &Path) -> HostRuntime {
    HostRuntime::new(Store::open(path, Default::default()).unwrap()).unwrap()
}
fn commit(h: &mut HostRuntime, operation: &str, id: &str, evidence: &[Evidence]) {
    let c = connection(h, id);
    h.create_content_with_evidence(&c, operation, &card(id), evidence, || 1)
        .unwrap();
    h.disconnect(&c).unwrap();
}
fn fixture() -> (tempfile::TempDir, PathBuf, Evidence, Evidence) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let a = evidence("task-a", 128 * 1024);
    let b = evidence("task-b", 128 * 1024);
    let mut h = initialized(&path);
    commit(&mut h, "create", "card", &[a.clone(), b.clone()]);
    drop(h);
    (dir, path, a, b)
}
fn read_recipe(sql: &rusqlite::Connection, digest: [u8; 32]) -> Recipe {
    let bytes: Vec<u8> = sql
        .query_row(
            "SELECT payload FROM task_evidence WHERE digest=?1",
            [digest.as_slice()],
            |r| r.get(0),
        )
        .unwrap();
    Recipe::decode(unpack(&bytes, b"MORROWQ1").as_slice()).unwrap()
}
fn replace_recipe(sql: &rusqlite::Connection, digest: [u8; 32], recipe: &Recipe) {
    sql.execute(
        "UPDATE task_evidence SET payload=?1 WHERE digest=?2",
        rusqlite::params![
            pack(b"MORROWQ1", &recipe.encode_to_vec()),
            digest.as_slice()
        ],
    )
    .unwrap();
}
fn version(sql: &rusqlite::Connection) -> i64 {
    sql.query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap()
}
fn counts(sql: &rusqlite::Connection) -> (i64, i64) {
    (
        sql.query_row("SELECT count(*) FROM evidence_chunks", [], |r| r.get(0))
            .unwrap(),
        sql.query_row("SELECT count(*) FROM task_evidence_chunks", [], |r| {
            r.get(0)
        })
        .unwrap(),
    )
}
fn legacy_seven(path: &Path, originals: &[Evidence]) {
    let sql = rusqlite::Connection::open(path).unwrap();
    sql.execute_batch("PRAGMA foreign_keys=OFF; BEGIN IMMEDIATE;")
        .unwrap();
    for e in originals {
        sql.execute(
            "UPDATE task_evidence SET payload=?1 WHERE digest=?2",
            rusqlite::params![e.container(), e.digest().as_slice()],
        )
        .unwrap();
    }
    sql.execute_batch("DROP TABLE task_evidence_chunks; DROP TABLE evidence_chunks; PRAGMA user_version=7; COMMIT;").unwrap();
}
#[test]
fn same_package_observations_share_real_blocks_and_rebuild_exact_original_containers() {
    let (_dir, path, a, b) = fixture();
    let sql = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(version(&sql), 9);
    let mut all = BTreeSet::new();
    let mut per = Vec::new();
    let mut logical = 0;
    for e in [&a, &b] {
        let recipe = read_recipe(&sql, e.digest());
        assert_eq!(recipe.version, 1);
        assert_eq!(recipe.evidence, e.digest());
        assert_eq!(recipe.length, e.container().len() as u64);
        assert_eq!(recipe.container, Sha256::digest(e.container()).as_slice());
        let expected = e
            .container()
            .chunks(BLOCK)
            .map(|chunk| Sha256::digest(chunk).to_vec())
            .collect::<Vec<_>>();
        assert_eq!(recipe.chunks, expected);
        logical += expected.len();
        let mut reconstructed = Vec::new();
        for digest in &recipe.chunks {
            let bytes: Vec<u8> = sql
                .query_row(
                    "SELECT payload FROM evidence_chunks WHERE digest=?1",
                    [digest],
                    |r| r.get(0),
                )
                .unwrap();
            let chunk = Chunk::decode(unpack(&bytes, b"MORROWH1").as_slice()).unwrap();
            assert_eq!(chunk.version, 1);
            assert_eq!(&chunk.digest, digest);
            assert_eq!(Sha256::digest(&chunk.payload).as_slice(), digest);
            assert!(!chunk.payload.is_empty() && chunk.payload.len() <= BLOCK);
            reconstructed.extend(chunk.payload);
        }
        assert_eq!(reconstructed, e.container());
        all.extend(expected.clone());
        per.push(expected.into_iter().collect::<BTreeSet<_>>());
    }
    let shared = per[0].intersection(&per[1]).count();
    assert!(shared >= 2, "same-package prefix failed to share");
    assert_eq!(counts(&sql), (all.len() as i64, logical as i64));
    eprintln!(
        "chunk-sharing logical={logical} unique={} shared={shared}",
        all.len()
    );
    drop(sql);
    let s = Store::open_existing(&path, Default::default()).unwrap();
    s.integrity_check().unwrap();
    let originals = s.operation_evidence("card", "create").unwrap();
    for (stored, original) in originals.iter().zip([&a, &b]) {
        assert_eq!(stored.container(), original.container());
        assert_eq!(stored.raw(), original.raw());
        assert_eq!(stored.digest(), original.digest());
    }
}
#[test]
fn chunk_recipe_link_and_size_corruption_fail_closed_without_repair() {
    for case in 0..14 {
        let (_dir, path, a, _b) = fixture();
        let sql = rusqlite::Connection::open(&path).unwrap();
        sql.execute_batch("PRAGMA foreign_keys=OFF").unwrap();
        let mut recipe = read_recipe(&sql, a.digest());
        match case {
            0 => {
                sql.execute(
                    "UPDATE evidence_chunks SET payload=x'00' WHERE digest=?1",
                    [&recipe.chunks[0]],
                )
                .unwrap();
            }
            1 => {
                sql.execute(
                    "DELETE FROM evidence_chunks WHERE digest=?1",
                    [&recipe.chunks[0]],
                )
                .unwrap();
            }
            2 => {
                sql.execute(
                    "DELETE FROM task_evidence_chunks WHERE evidence_digest=?1 AND ordinal=0",
                    [a.digest().as_slice()],
                )
                .unwrap();
            }
            3 => {
                sql.execute("INSERT INTO task_evidence_chunks(evidence_digest,ordinal,digest) VALUES(?1,?2,?3)",rusqlite::params![a.digest().as_slice(),recipe.chunks.len() as i64,&recipe.chunks[0]]).unwrap();
            }
            4 => {
                recipe.chunks.swap(0, 1);
                replace_recipe(&sql, a.digest(), &recipe);
            }
            5 => {
                recipe.length = task_evidence::MAX_CONTAINER_BYTES as u64 + 1;
                replace_recipe(&sql, a.digest(), &recipe);
            }
            6 => {
                recipe.evidence = vec![99; 32];
                replace_recipe(&sql, a.digest(), &recipe);
            }
            7 => {
                recipe.chunks = vec![
                    recipe.chunks[0].clone();
                    task_evidence::MAX_CONTAINER_BYTES.div_ceil(BLOCK) + 1
                ];
                replace_recipe(&sql, a.digest(), &recipe);
            }
            8 => {
                let bad = Chunk {
                    version: 1,
                    digest: recipe.chunks[0].clone(),
                    payload: vec![0; BLOCK + 1],
                };
                sql.execute(
                    "UPDATE evidence_chunks SET payload=?1 WHERE digest=?2",
                    rusqlite::params![pack(b"MORROWH1", &bad.encode_to_vec()), &recipe.chunks[0]],
                )
                .unwrap();
            }
            9 => {
                sql.execute(
                    "UPDATE task_evidence SET payload=zeroblob(17000) WHERE digest=?1",
                    [a.digest().as_slice()],
                )
                .unwrap();
            }
            11 | 13 => {
                let bad = Chunk {
                    version: 1,
                    digest: recipe.chunks[0].clone(),
                    payload: if case == 11 {
                        b"wrong digest".to_vec()
                    } else {
                        vec![]
                    },
                };
                sql.execute(
                    "UPDATE evidence_chunks SET payload=?1 WHERE digest=?2",
                    rusqlite::params![pack(b"MORROWH1", &bad.encode_to_vec()), &recipe.chunks[0]],
                )
                .unwrap();
            }
            12 => {
                recipe.container = vec![99; 32];
                replace_recipe(&sql, a.digest(), &recipe);
            }
            _ => {
                recipe.chunks.swap(0, 1);
                replace_recipe(&sql, a.digest(), &recipe);
                for ordinal in [0, 1] {
                    sql.execute("UPDATE task_evidence_chunks SET digest=?1 WHERE evidence_digest=?2 AND ordinal=?3",rusqlite::params![&recipe.chunks[ordinal],a.digest().as_slice(),ordinal as i64]).unwrap();
                }
            }
        }
        drop(sql);
        assert!(
            Store::open_existing(&path, Default::default()).is_err(),
            "accepted corruption {case}"
        );
        let sql = rusqlite::Connection::open(&path).unwrap();
        assert_eq!(version(&sql), 9);
    }
}
#[test]
fn orphan_chunk_is_not_accepted_as_valid_store() {
    let (_dir, path, _, _) = fixture();
    let sql = rusqlite::Connection::open(&path).unwrap();
    let payload = b"orphan chunk".to_vec();
    let digest = Sha256::digest(&payload).to_vec();
    let chunk = Chunk {
        version: 1,
        digest: digest.clone(),
        payload,
    };
    sql.execute(
        "INSERT INTO evidence_chunks(digest,payload) VALUES(?1,?2)",
        rusqlite::params![&digest, pack(b"MORROWH1", &chunk.encode_to_vec())],
    )
    .unwrap();
    drop(sql);
    assert!(Store::open_existing(&path, Default::default()).is_err());
    let sql = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(
        sql.query_row(
            "SELECT count(*) FROM evidence_chunks WHERE digest=?1",
            [&digest],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
}
#[test]
fn legacy_migration_preserves_raw_unknown_fields_exact_container_and_commit_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let base = evidence("future", 128 * 1024);
    let mut raw = base.raw().to_vec();
    raw.extend([0xa0, 0x06, 0x2a]); // unknown optional field 100
    let container = pack(b"MORROWE1", &raw);
    let e = task_evidence::decode(&container, Sha256::digest(&raw).into()).unwrap();
    let mut h = initialized(&path);
    commit(&mut h, "create", "card", std::slice::from_ref(&e));
    drop(h);
    legacy_seven(&path, std::slice::from_ref(&e));
    let sql = rusqlite::Connection::open(&path).unwrap();
    let before: Vec<u8> = sql
        .query_row(
            "SELECT payload FROM operations WHERE id='create'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    drop(sql);
    let s = Store::open_existing(&path, Default::default()).unwrap();
    let restored = s.operation_evidence("card", "create").unwrap().remove(0);
    assert_eq!(restored.container(), container);
    assert_eq!(restored.raw(), raw);
    assert_eq!(restored.digest(), e.digest());
    drop(s);
    let sql = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(version(&sql), 9);
    let after: Vec<u8> = sql
        .query_row(
            "SELECT payload FROM operations WHERE id='create'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(before, after);
}
#[test]
fn audited_readonly_v7_does_not_migrate_and_v8_preserves_existing_signature() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let key = SigningKey::from_bytes(&[17; 32]);
    let trust = TrustedLog {
        id: "chunk-review".into(),
        key: key.verifying_key(),
    };
    let store = Store::open_audited(&path, Default::default(), true, trust.clone()).unwrap();
    let mut h = HostRuntime::new(store).unwrap();
    let e = evidence("signed", 128 * 1024);
    commit(&mut h, "create", "card", std::slice::from_ref(&e));
    let segment = audit::sign(
        &audit::from_pending(&trust, 1, [0; 32], &h.store_local().pending(0, 10).unwrap()).unwrap(),
        &trust,
        &key,
    )
    .unwrap();
    h.store_local_mut().seal_pending(&segment).unwrap();
    drop(h);
    legacy_seven(&path, std::slice::from_ref(&e));
    let s = Store::open_read_only_audited(&path, trust.clone()).unwrap();
    assert_eq!(
        s.operation_evidence("card", "create").unwrap()[0].container(),
        e.container()
    );
    assert_eq!(s.sealed_segment(1).unwrap().unwrap(), segment);
    drop(s);
    let sql = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(version(&sql), 7);
    drop(sql);
    let store = Store::open_audited(&path, Default::default(), false, trust).unwrap();
    assert_eq!(store.sealed_segment(1).unwrap().unwrap(), segment);
    let mut h = HostRuntime::new(store).unwrap();
    commit(
        &mut h,
        "new-write",
        "second",
        &[evidence("second", 128 * 1024)],
    );
    h.store_local().integrity_check().unwrap();
    drop(h);
    let sql = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(version(&sql), 9);
    assert!(counts(&sql).0 > 0);
}
#[cfg(feature = "fault-injection")]
#[test]
fn evidence_chunks_crash_child() {
    let Ok(path) = std::env::var("MORROW_CHUNKS_TEST_DB") else {
        return;
    };
    let mode = std::env::var("MORROW_CHUNKS_TEST_MODE").unwrap();
    let store = Store::open_existing(Path::new(&path), Default::default()).unwrap();
    if mode == "migrate" {
        return;
    }
    let mut h = HostRuntime::new(store).unwrap();
    commit(&mut h, "create", "card", &[evidence("crash", 128 * 1024)]);
}
#[cfg(feature = "fault-injection")]
fn child(path: &Path, mode: &str, point: &str) -> std::process::ExitStatus {
    use std::{
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "evidence_chunks_crash_child", "--nocapture"])
        .env("MORROW_CHUNKS_TEST_DB", path)
        .env("MORROW_CHUNKS_TEST_MODE", mode)
        .env("MORROW_TEST_CRASH_AT", point)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command.spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status,
            Ok(None) => {}
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("observe child: {error}");
            }
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("owned child timed out");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}
#[cfg(feature = "fault-injection")]
#[test]
fn write_crashes_never_leave_chunks_without_committed_content() {
    for point in ["after-evidence-chunks", "before-commit", "after-commit"] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("db");
        drop(initialized(&path));
        assert_eq!(child(&path, "write", point).code(), Some(86));
        let s = Store::open_existing(&path, Default::default()).unwrap();
        let committed = point == "after-commit";
        assert_eq!(s.card("card").unwrap().is_some(), committed);
        assert_eq!(
            matches!(s.lookup("create").unwrap(), Lookup::Committed(_)),
            committed
        );
        assert_eq!(s.pending(0, 10).unwrap().len(), usize::from(committed));
        s.integrity_check().unwrap();
        if committed {
            assert_eq!(
                s.operation_evidence("card", "create").unwrap()[0].container(),
                evidence("crash", 128 * 1024).container()
            );
        }
        drop(s);
        let sql = rusqlite::Connection::open(&path).unwrap();
        if committed {
            assert!(counts(&sql).0 > 0 && counts(&sql).1 > 0);
        } else {
            assert_eq!(counts(&sql), (0, 0));
        }
    }
}
#[cfg(feature = "fault-injection")]
#[test]
fn migration_crashes_keep_whole_v7_or_whole_v8_and_original_evidence() {
    for point in [
        "evidence-chunks-migration-before-commit",
        "evidence-chunks-migration-after-commit",
    ] {
        let (_dir, path, a, b) = fixture();
        legacy_seven(&path, &[a.clone(), b.clone()]);
        assert_eq!(child(&path, "migrate", point).code(), Some(86), "{point}");
        let sql = rusqlite::Connection::open(&path).unwrap();
        let committed = point.ends_with("after-commit");
        assert_eq!(version(&sql), if committed { 8 } else { 7 });
        let tables:i64=sql.query_row("SELECT count(*) FROM sqlite_schema WHERE name IN ('evidence_chunks','task_evidence_chunks')",[],|r|r.get(0)).unwrap();
        assert_eq!(tables, if committed { 2 } else { 0 });
        drop(sql);
        let s = Store::open_existing(&path, Default::default()).unwrap();
        s.integrity_check().unwrap();
        let loaded = s.operation_evidence("card", "create").unwrap();
        assert_eq!(loaded[0].container(), a.container());
        assert_eq!(loaded[1].container(), b.container());
        assert_eq!(s.card("card").unwrap().unwrap().summary().revision, 1);
    }
}

#[test]
fn signed_snapshot_contains_complete_shared_chunks_after_original_database_is_removed() {
    let dir = tempfile::tempdir().unwrap();
    let original = dir.path().join("original.db");
    let snapshot = dir.path().join("snapshot.db");
    let key = SigningKey::from_bytes(&[23; 32]);
    let trust = TrustedLog {
        id: "snapshot-chunk-review".into(),
        key: key.verifying_key(),
    };
    let store = Store::open_audited(&original, Default::default(), true, trust.clone()).unwrap();
    let mut host = HostRuntime::new(store).unwrap();
    let a = evidence("snapshot-a", 128 * 1024);
    let b = evidence("snapshot-b", 128 * 1024);
    commit(&mut host, "create", "card", &[a.clone(), b.clone()]);
    let segment = audit::sign(
        &audit::from_pending(
            &trust,
            1,
            [0; 32],
            &host.store_local().pending(0, 10).unwrap(),
        )
        .unwrap(),
        &trust,
        &key,
    )
    .unwrap();
    host.store_local_mut().seal_pending(&segment).unwrap();
    host.store_local()
        .snapshot_to(&snapshot, 64 * 1024 * 1024)
        .unwrap();
    drop(host);
    // This file was created exclusively by this test in its own temporary directory.
    std::fs::remove_file(&original).unwrap();
    assert!(!original.exists());
    let sql = rusqlite::Connection::open_with_flags(
        &snapshot,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    assert_eq!(version(&sql), 9);
    let (physical, links) = counts(&sql);
    assert!(
        physical > 0 && physical < links,
        "snapshot must really include shared chunks"
    );
    drop(sql);
    // No original DB, plugin directory, or credential lookup is supplied here.
    let store = Store::open_read_only_audited(&snapshot, trust).unwrap();
    store.integrity_check().unwrap();
    let restored = store.operation_evidence("card", "create").unwrap();
    assert_eq!(restored.len(), 2);
    for (actual, expected) in restored.iter().zip([&a, &b]) {
        assert_eq!(actual.container(), expected.container());
        assert_eq!(actual.raw(), expected.raw());
        assert_eq!(actual.digest(), expected.digest());
    }
    assert_eq!(store.sealed_segment(1).unwrap().unwrap(), segment);
    assert_eq!(store.card("card").unwrap().unwrap().summary().revision, 1);
    assert!(store.pending(0, 10).unwrap().is_empty());
    assert!(!original.exists());
}

// Version 8 and 9 share physical tables, but only 9 may contain batch observations.
fn schema_eight(path: &Path) {
    let sql = rusqlite::Connection::open(path).unwrap();
    assert_eq!(version(&sql), 9);
    sql.execute_batch("PRAGMA user_version=8;").unwrap();
}
fn operation_payload(path: &Path) -> Vec<u8> {
    rusqlite::Connection::open(path)
        .unwrap()
        .query_row(
            "SELECT payload FROM operations WHERE id='create'",
            [],
            |r| r.get(0),
        )
        .unwrap()
}
fn stored_chunks(path: &Path) -> Vec<(Vec<u8>, Vec<u8>)> {
    let sql = rusqlite::Connection::open(path).unwrap();
    let mut statement = sql
        .prepare("SELECT digest,payload FROM evidence_chunks ORDER BY digest")
        .unwrap();
    statement
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}
fn signed_fixture(items: &[Evidence]) -> (tempfile::TempDir, PathBuf, TrustedLog, Vec<u8>) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("signed.db");
    let key = SigningKey::from_bytes(&[38; 32]);
    let trust = TrustedLog {
        id: "batch-migration-review".into(),
        key: key.verifying_key(),
    };
    let store = Store::open_audited(&path, Default::default(), true, trust.clone()).unwrap();
    let mut h = HostRuntime::new(store).unwrap();
    commit(&mut h, "create", "card", items);
    let segment = audit::sign(
        &audit::from_pending(&trust, 1, [0; 32], &h.store_local().pending(0, 10).unwrap()).unwrap(),
        &trust,
        &key,
    )
    .unwrap();
    h.store_local_mut().seal_pending(&segment).unwrap();
    drop(h);
    (dir, path, trust, segment)
}
#[test]
fn format_eight_readonly_then_migration_preserve_unknown_v1_bytes_and_signed_history() {
    let base = evidence("old-single", 128 * 1024);
    let mut raw = base.raw().to_vec();
    raw.extend([0xa0, 0x06, 42]);
    let e = task_evidence::decode(&pack(b"MORROWE1", &raw), Sha256::digest(&raw).into()).unwrap();
    let (_dir, path, trust, segment) = signed_fixture(std::slice::from_ref(&e));
    schema_eight(&path);
    let commit_before = operation_payload(&path);
    let chunks_before = stored_chunks(&path);
    let read = Store::open_read_only_audited(&path, trust.clone()).unwrap();
    assert_eq!(
        read.operation_evidence("card", "create").unwrap()[0].raw(),
        raw
    );
    assert_eq!(read.sealed_segment(1).unwrap().unwrap(), segment);
    drop(read);
    assert_eq!(version(&rusqlite::Connection::open(&path).unwrap()), 8);
    let migrated = Store::open_audited(&path, Default::default(), false, trust).unwrap();
    migrated.integrity_check().unwrap();
    let original = migrated
        .operation_evidence("card", "create")
        .unwrap()
        .remove(0);
    assert_eq!(original.raw(), e.raw());
    assert_eq!(original.container(), e.container());
    assert_eq!(original.digest(), e.digest());
    assert_eq!(migrated.sealed_segment(1).unwrap().unwrap(), segment);
    assert_eq!(
        migrated.card("card").unwrap().unwrap().summary().revision,
        1
    );
    drop(migrated);
    assert_eq!(version(&rusqlite::Connection::open(&path).unwrap()), 9);
    assert_eq!(operation_payload(&path), commit_before);
    assert_eq!(stored_chunks(&path), chunks_before);
}
#[cfg(feature = "fault-injection")]
#[test]
fn format_eight_batch_migration_crashes_publish_only_whole_version_transition() {
    for point in [
        "batch-evidence-migration-before-commit",
        "batch-evidence-migration-after-commit",
    ] {
        let (_dir, path, a, b) = fixture();
        schema_eight(&path);
        let commit_before = operation_payload(&path);
        let chunks_before = stored_chunks(&path);
        assert_eq!(child(&path, "migrate", point).code(), Some(86));
        assert_eq!(
            version(&rusqlite::Connection::open(&path).unwrap()),
            if point.ends_with("after-commit") {
                9
            } else {
                8
            }
        );
        assert_eq!(operation_payload(&path), commit_before);
        assert_eq!(stored_chunks(&path), chunks_before);
        let reopened = Store::open_existing(&path, Default::default()).unwrap();
        reopened.integrity_check().unwrap();
        let originals = reopened.operation_evidence("card", "create").unwrap();
        for (actual, expected) in originals.iter().zip([&a, &b]) {
            assert_eq!(actual.container(), expected.container());
            assert_eq!(actual.raw(), expected.raw());
            assert_eq!(actual.digest(), expected.digest());
        }
        assert_eq!(originals.len(), 2);
        assert_eq!(
            reopened.card("card").unwrap().unwrap().summary().revision,
            1
        );
        drop(reopened);
        assert_eq!(version(&rusqlite::Connection::open(&path).unwrap()), 9);
        assert_eq!(operation_payload(&path), commit_before);
        assert_eq!(stored_chunks(&path), chunks_before);
    }
}
#[test]
fn schema_two_hidden_in_format_eight_is_rejected_without_version_repair() {
    let single = evidence("batch-page", 128 * 1024);
    let o = single.data();
    let batch = task_evidence::encode(TaskEvidence {
        schema_version: 2,
        package_archive: o.package_archive.clone(),
        batch: Some(task_evidence::proto::Batch {
            intent_type: "test.intent".into(),
            intent: b"synthetic intent".to_vec(),
            total_fuel: 100000,
            observations: vec![task_evidence::proto::Observation {
                invocation: o.invocation.clone(),
                budget: o.budget,
                backend: o.backend.clone(),
                completion: o.completion.clone(),
                fault: o.fault,
                exit_code: o.exit_code,
                observed_host_calls: o.observed_host_calls,
                fuel_remaining: o.fuel_remaining,
            }],
        }),
        ..Default::default()
    })
    .unwrap();
    let (_dir, path, trust, segment) = signed_fixture(std::slice::from_ref(&batch));
    let valid = Store::open_read_only_audited(&path, trust.clone()).unwrap();
    assert_eq!(
        valid.operation_evidence("card", "create").unwrap()[0].container(),
        batch.container()
    );
    assert_eq!(valid.sealed_segment(1).unwrap().unwrap(), segment);
    drop(valid);
    schema_eight(&path);
    let commit_before = operation_payload(&path);
    let chunks_before = stored_chunks(&path);
    assert!(matches!(
        Store::open_read_only_audited(&path, trust.clone()),
        Err(morrow_core::Error::UnsupportedVersion)
    ));
    assert!(matches!(
        Store::open_audited(&path, Default::default(), false, trust),
        Err(morrow_core::Error::UnsupportedVersion)
    ));
    assert_eq!(version(&rusqlite::Connection::open(&path).unwrap()), 8);
    assert_eq!(operation_payload(&path), commit_before);
    assert_eq!(stored_chunks(&path), chunks_before);
}
