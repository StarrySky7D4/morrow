//! Actual packaged SDK code on a native worker; same durable operation identity across tasks.
use morrow_core::{
    content::CardRecord,
    dispatch::HostRuntime,
    lifecycle::GrantKind,
    plugin_package::catalog,
    store::{EventBudget, Store},
    transaction::Lookup,
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
        return Err("package paths required".into());
    }
    for path in paths {
        let p = PreparedPackage::new(
            catalog::read_file(std::path::Path::new(&path))?,
            Limits::default(),
        )
        .expect("prepare");
        let dir = tempfile::tempdir()?;
        let db = dir.path().join("db");
        let mut store = Store::open(&db, EventBudget::default())?;
        store.create_local(
            "seed",
            &CardRecord::new("legacy-123", "morrow.note", 1, "old", vec![])?,
        )?;
        let mut host = HostRuntime::new(store)?;
        let mut c = p.connect(&mut host)?;
        host.grant(&mut c, GrantKind::Rename, "legacy-123", u64::MAX, 0)?;
        let mut ticks = 0;
        let mut worker = Worker::spawn(
            p,
            host,
            c,
            move || {
                ticks += 1;
                ticks
            },
            4,
        )
        .expect("worker");
        let timeout = Duration::from_secs(10);
        let mut tasks = vec![
            worker.submit(timeout).unwrap(),
            worker.submit(timeout).unwrap(),
        ];
        worker.drain(timeout).unwrap();
        let until = Instant::now() + timeout;
        for task in &mut tasks {
            loop {
                if let Some(r) = task.try_result().unwrap() {
                    assert_eq!(r.outcome, Ok(20));
                    assert_eq!(r.host_calls, 1);
                    break;
                }
                assert!(Instant::now() < until);
                std::thread::sleep(Duration::from_millis(1));
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
        host.store_local().integrity_check()?;
        assert_eq!(host.store_local().pending(0, 10)?.len(), 2);
        drop(host);
        let store = Store::open_existing(&db, EventBudget::default())?;
        assert!(
            matches!(store.lookup_for_card("legacy-123","wasm-op")?,Lookup::Committed(r) if r.revision==2)
        );
        store.integrity_check()?;
        println!(
            "PASS: {path}: native worker ran two SDK tasks, drained and retired; reopened durable revision 2 without duplicate commit"
        );
    }
    Ok(())
}
