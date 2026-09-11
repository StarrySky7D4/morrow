//! Real pure computation in C/C++/Rust Wasm; no content grants or content calls.
use morrow_core::{
    content::CardRecord,
    dispatch::HostRuntime,
    plugin_package::catalog,
    store::{EventBudget, Store},
    task::{Invocation, MAX_VALUE_BYTES, Transform},
};
use morrow_plugin_runtime::{
    Limits,
    package::PreparedPackage,
    worker::{Phase, Worker},
};
use std::time::{Duration, Instant};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let paths: Vec<_> = std::env::args().skip(1).collect();
    if paths.is_empty() {
        return Err("transform package paths required".into());
    }
    for path in paths {
        let p = PreparedPackage::new(
            catalog::read_file(std::path::Path::new(&path))?,
            Limits::default(),
        )
        .unwrap();
        assert!(p.package().capabilities().is_empty());
        let dir = tempfile::tempdir()?;
        let db = dir.path().join("db");
        let mut store = Store::open(&db, EventBudget::default())?;
        store.create_local(
            "seed",
            &CardRecord::new("card", "note", 1, "untouched", vec![])?,
        )?;
        let mut host = HostRuntime::new(store)?;
        let c = p.connect(&mut host)?;
        let mut worker = Worker::spawn(
            p,
            host,
            c,
            || panic!("pure transform attempted host clock"),
            16,
        )
        .unwrap();
        let timeout = Duration::from_secs(30);
        let until = Instant::now() + timeout;
        let mut tasks = vec![];
        let samples = [
            vec![],
            "Hello 世界🌈".as_bytes().to_vec(),
            (0..=255).collect::<Vec<u8>>(),
            (0..MAX_VALUE_BYTES).map(|n| (n % 256) as u8).collect(),
        ];
        for handler in ["bytes.reverse", "bytes.ascii-uppercase"] {
            for input in &samples {
                let mut expected = input.clone();
                if handler == "bytes.reverse" {
                    expected.reverse();
                } else {
                    expected.make_ascii_uppercase();
                }
                let invocation = Invocation::new_transform(
                    &format!("pure-{}", tasks.len()),
                    Transform {
                        handler: handler.into(),
                        input_type: "bytes".into(),
                        output_type: "bytes".into(),
                        input: input.clone(),
                    },
                )?;
                tasks.push((worker.submit_task(invocation, timeout).unwrap(), expected));
            }
        }
        worker.drain(timeout).unwrap();
        for (task, expected) in &mut tasks {
            let result = loop {
                if let Some(r) = task.try_result().unwrap() {
                    break r;
                }
                assert!(Instant::now() < until);
                std::thread::sleep(Duration::from_millis(1));
            };
            assert_eq!(result.execution.outcome, Ok(0));
            assert_eq!(result.execution.host_calls, 0);
            assert!(result.response.is_none());
            let output = result.output.expect("correlated produced data");
            assert_eq!(output.type_id, "bytes");
            assert_eq!(&output.bytes, expected);
        }
        let host = loop {
            if let Some(h) = worker.try_finish().unwrap() {
                break h;
            }
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(1));
        };
        assert_eq!(worker.phase(), Phase::Stopped);
        assert_eq!(host.store_local().pending(0, 10)?.len(), 1);
        host.store_local().integrity_check()?;
        drop(host);
        let store = Store::open_existing(&db, EventBudget::default())?;
        assert_eq!(store.card("card")?.unwrap().summary().title, "untouched");
        store.integrity_check()?;
        println!(
            "PASS: {path}: 8 pure transformations, empty/Unicode bytes/all byte values/64KiB; independently verified output, zero core calls, no content or event changes"
        );
    }
    Ok(())
}
