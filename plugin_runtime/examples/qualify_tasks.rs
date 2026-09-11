//! Three-language dynamic task qualification on the actual native worker.
use morrow_core::{
    content::{Attachment, CardRecord},
    dispatch::HostRuntime,
    lifecycle::GrantKind,
    plugin_package::catalog,
    response::{Failure, Outcome},
    runtime::{Command, ReadAttachment, RenameRequest},
    store::{EventBudget, Store},
    task::Invocation,
    transaction::Lookup,
};
use morrow_plugin_runtime::{
    Limits,
    package::PreparedPackage,
    worker::{Phase, Worker},
};
use std::time::{Duration, Instant};
fn rename(op: &str, card: &str, rev: u64, title: &str) -> Command {
    Command::Rename(RenameRequest {
        operation_id: op.into(),
        card_id: card.into(),
        expected_revision: rev,
        title: title.into(),
    })
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let paths: Vec<_> = std::env::args().skip(1).collect();
    if paths.is_empty() {
        return Err("ABI v2 package paths required".into());
    }
    for path in paths {
        let p = PreparedPackage::new(
            catalog::read_file(std::path::Path::new(&path))?,
            Limits::default(),
        )
        .unwrap();
        let dir = tempfile::tempdir()?;
        let db = dir.path().join("db");
        let mut store = Store::open(&db, EventBudget::default())?;
        let payload = b"dynamic attachment content";
        let blob = store.stage_blob(
            &mut std::io::Cursor::new(payload),
            payload.len() as u64,
            None,
            0,
        )?;
        let attachment = Attachment {
            id: "file".into(),
            display_name: "sample".into(),
            media_type: "application/octet-stream".into(),
            byte_length: blob.byte_length,
            sha256: blob.sha256,
        };
        store.create_local(
            "seed-a",
            &CardRecord::new_with_attachments(
                "card-A",
                "note",
                1,
                "before",
                vec![],
                &[attachment],
            )?,
        )?;
        store.create_local(
            "seed-b",
            &CardRecord::new("card-B", "note", 1, "private", vec![])?,
        )?;
        let mut host = HostRuntime::new(store)?;
        let mut c = p.connect(&mut host)?;
        for kind in [
            GrantKind::Rename,
            GrantKind::ReadSummary,
            GrantKind::QueryOperation,
        ] {
            host.grant(&mut c, kind, "card-A", u64::MAX, 0)?;
        }
        host.grant_attachment(&mut c, "card-A", "file", u64::MAX, 0)?;
        let commands = [
            rename("edit-101", "card-A", 1, "彩色🌈"),
            rename("denied-B", "card-B", 1, "must not write"),
            rename("edit-101", "card-A", 1, "彩色🌈"),
            rename("edit-202", "card-A", 2, "第二次任务"),
            Command::ReadSummary {
                request_id: "summary-3".into(),
                card_id: "card-A".into(),
            },
            Command::QueryOperation {
                request_id: "query-old".into(),
                card_id: "card-A".into(),
                operation_id: "edit-101".into(),
            },
            Command::QueryOperation {
                request_id: "query-absent".into(),
                card_id: "card-A".into(),
                operation_id: "absent".into(),
            },
            Command::ReadAttachment(ReadAttachment {
                request_id: "read-part".into(),
                card_id: "card-A".into(),
                attachment_id: "file".into(),
                expected_revision: 3,
                offset: 8,
                length: 10,
            }),
        ];
        let mut ticks = 0;
        let mut worker = Worker::spawn(
            p,
            host,
            c,
            move || {
                ticks += 1;
                ticks
            },
            16,
        )
        .unwrap();
        let timeout = Duration::from_secs(20);
        let until = Instant::now() + timeout;
        let mut tasks = commands
            .iter()
            .enumerate()
            .map(|(i, c)| {
                worker
                    .submit_task(Invocation::new(&format!("task-{i}"), c).unwrap(), timeout)
                    .unwrap()
            })
            .collect::<Vec<_>>();
        worker.drain(timeout).unwrap();
        for (i, t) in tasks.iter_mut().enumerate() {
            let r = loop {
                if let Some(r) = t.try_result().unwrap() {
                    break r;
                }
                assert!(Instant::now() < until);
                std::thread::sleep(Duration::from_millis(1));
            };
            assert_eq!(r.execution.outcome, Ok(0));
            assert_eq!(r.execution.host_calls, 1);
            let response = r.response.expect("verified authoritative response");
            assert_eq!(response.request_id, commands[i].request_id());
            match (i, response.outcome) {
                (0 | 2, Outcome::Renamed(r)) => assert_eq!(r.revision, 2),
                (1, Outcome::Rejected(Failure::Denied)) => {}
                (3, Outcome::Renamed(r)) => assert_eq!(r.revision, 3),
                (4, Outcome::Summary(r)) => {
                    assert_eq!(r.revision, 3);
                    assert_eq!(r.title, "第二次任务");
                }
                (
                    5,
                    Outcome::OperationResult {
                        result: Lookup::Committed(r),
                        ..
                    },
                ) => assert_eq!(r.revision, 2),
                (
                    6,
                    Outcome::OperationResult {
                        result: Lookup::Absent,
                        ..
                    },
                ) => {}
                (7, Outcome::AttachmentChunk(r)) => assert_eq!(r.bytes, &payload[8..18]),
                other => panic!("unexpected {other:?}"),
            }
        }
        let host = loop {
            if let Some(h) = worker.try_finish().unwrap() {
                break h;
            }
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(1));
        };
        assert_eq!(worker.phase(), Phase::Stopped);
        assert_eq!(host.store_local().pending(0, 10)?.len(), 4);
        host.store_local().integrity_check()?;
        drop(host);
        let store = Store::open_existing(&db, EventBudget::default())?;
        assert!(
            matches!(store.lookup_for_card("card-A","edit-202")?,Lookup::Committed(r) if r.revision==3)
        );
        assert_eq!(store.card("card-B")?.unwrap().summary().title, "private");
        store.integrity_check()?;
        println!(
            "PASS: {path}: 8 host-supplied tasks; dynamic Unicode rename/revision, permission denial, dedup, summary, operation query, real attachment bytes; verified completions and reopened revision 3"
        );
    }
    Ok(())
}
