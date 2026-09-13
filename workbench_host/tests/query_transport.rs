#![cfg(target_os = "windows")]
mod common;
use capnp::{message::Builder, serialize};
use morrow_core::{
    content::CardRecord,
    store::{EventBudget, Store},
};
use morrow_workbench_host::{host_capnp as wire, protocol};
use morrow_workbench_plugin::{Action, Request, codec, persistence};
use std::{
    io::{Read, Write},
    os::windows::process::CommandExt,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

fn request(action: wire::Action, id: &str, text: &str) -> Vec<u8> {
    let mut message = Builder::new_default();
    let mut r = message.init_root::<wire::request::Builder>();
    r.set_version(1);
    r.set_digest(&protocol::digest());
    r.set_action(action);
    r.set_id(id);
    r.set_limit(2);
    if action == wire::Action::Query {
        r.set_payload(
            &codec::encode_request(&Request {
                action: Action::Query,
                current: Default::default(),
                proposed: Default::default(),
                text: text.into(),
                flag: false,
                now_ms: 1,
                ideas: vec![],
                section: "概览".into(),
                filter: "全部".into(),
                sort: "最近添加".into(),
            })
            .unwrap(),
        );
    }
    let bytes = serialize::write_message_to_words(&message);
    let mut frame = (bytes.len() as u32).to_le_bytes().to_vec();
    frame.extend_from_slice(&bytes);
    frame
}

#[test]
fn oversized_actual_query_returns_bounded_error_and_same_process_keeps_serving() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("workbench.db");
    // 600 valid IDs fit the 4096 result-count contract but exceed the byte budget.
    // Seed ordinary content locally; the query itself executes the actual Rust SDK guest.
    let ids: Vec<String> = (0..600)
        .map(|i| format!("card-{i:04}-{}", "x".repeat(230)))
        .collect();
    assert!(ids.iter().map(String::len).sum::<usize>() > 128 * 1024);
    let mut store = Store::open(&db, EventBudget::default()).unwrap();
    for (i, id) in ids.iter().enumerate() {
        let mut idea = common::idea(id);
        idea.title = if i == 0 { "needle" } else { "record" }.into();
        let card = CardRecord::new(
            id,
            "org.morrow.idea",
            1,
            &idea.title,
            persistence::encode(&idea, None).unwrap(),
        )
        .unwrap();
        store.create_local(&format!("seed-{i}"), &card).unwrap();
    }
    drop(store);
    let package = dir.path().join("workbench.morrowplugin");
    std::fs::write(&package, common::package().archive()).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_morrow-workbench-host"))
        .arg(&db)
        .arg(&package)
        .creation_flags(0x08000000)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut requests = request(wire::Action::Query, "", "");
    requests.extend(request(wire::Action::Page, "", ""));
    requests.extend(request(wire::Action::Query, "", "needle"));
    requests.extend(request(wire::Action::Read, &ids[0], ""));
    child.stdin.take().unwrap().write_all(&requests).unwrap();
    // Expected bounded responses fit the pipe. Concurrent readers also ensure a
    // regression returning a large response cannot deadlock the child at stdout.
    let mut stdout = child.stdout.take().unwrap();
    let output = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).unwrap();
        bytes
    });
    let mut stderr = child.stderr.take().unwrap();
    let errors = std::thread::spawn(move || {
        let mut text = String::new();
        stderr.read_to_string(&mut text).unwrap();
        text
    });
    let deadline = Instant::now() + Duration::from_secs(90);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("query transport process exceeded deadline");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let stderr = errors.join().unwrap();
    assert!(
        status.success(),
        "host exited after oversized query: {stderr}"
    );
    let bytes = output.join().unwrap();
    let mut remaining = bytes.as_slice();
    for index in 0..4 {
        let mut header = [0; 4];
        remaining.read_exact(&mut header).unwrap();
        let size = u32::from_le_bytes(header) as usize;
        assert!((1..=128 * 1024).contains(&size));
        assert!(remaining.len() >= size);
        let message = serialize::read_message(&mut &remaining[..size], Default::default()).unwrap();
        remaining = &remaining[size..];
        let reply = message.get_root::<wire::response::Reader>().unwrap();
        assert_eq!(reply.get_version(), 1);
        assert_eq!(reply.get_digest().unwrap(), protocol::digest());
        assert!(!reply.get_read_only());
        let error = reply.get_error().unwrap().to_str().unwrap();
        if index == 0 {
            assert!(error.contains("128 KiB frame budget"), "{error}");
            assert!(error.contains("no result delivered"));
            assert_eq!(reply.get_ids().unwrap().len(), 0);
            assert!(reply.get_payload().unwrap().is_empty());
            assert!(size < 1024);
        } else {
            assert!(error.is_empty(), "request {index}: {error}");
            match index {
                1 => assert_eq!(reply.get_ids().unwrap().len(), 2),
                2 => {
                    let got = reply.get_ids().unwrap();
                    assert_eq!(got.len(), 1);
                    assert_eq!(got.get(0).unwrap().to_str().unwrap(), ids[0]);
                }
                3 => {
                    let record = codec::decode_response(reply.get_payload().unwrap()).unwrap();
                    assert_eq!(record.idea.id, ids[0]);
                    assert_eq!(reply.get_revision(), 1);
                }
                _ => unreachable!(),
            }
        }
    }
    assert!(remaining.is_empty());
}
