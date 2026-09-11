//! Actual compiled SDK guest -> fixed import -> authority-bound core -> SQLite.
use morrow_core::{
    content::CardRecord,
    dispatch::HostRuntime,
    lifecycle::GrantKind,
    store::{EventBudget, Store},
    transaction::Lookup,
};
use morrow_plugin_runtime::{Cancellation, Fault, Limits, Runner};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let paths: Vec<_> = std::env::args().skip(1).collect();
    if paths.is_empty() {
        return Err("compiled guest wasm paths required".into());
    }
    for path in paths {
        qualify(&path)?;
    }
    Ok(())
}
fn qualify(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let wasm = std::fs::read(path)?;
    let runner = Runner::new(&wasm, Limits::default()).expect("guest contract");
    let expected = morrow_core::runtime::RenameRequest {
        operation_id: "wasm-op".into(),
        card_id: "legacy-123".into(),
        expected_revision: 1,
        title: "Wasm SDK rename".into(),
    }
    .encode()?;
    let dir = tempfile::tempdir()?;
    let db = dir.path().join("qualification.db");
    let mut store = Store::open(&db, EventBudget::default())?;
    store.create_local(
        "seed",
        &CardRecord::new("legacy-123", "morrow.note", 1, "old", vec![])?,
    )?;
    let mut host = HostRuntime::new(store)?;
    let mut a = host.connect()?;
    let b = host.connect()?;
    let mut clock = 0u64;
    let report = runner.run(
        &mut |input| {
            assert_eq!(
                input, expected,
                "guest request must match independent core encoding"
            );
            host.dispatch(&a, input, || {
                clock += 1;
                clock
            })
            .map_err(|_| ())
        },
        Cancellation::default(),
    );
    assert_eq!(report.outcome, Ok(10));
    assert_eq!(report.host_calls, 1);
    host.grant(&mut a, GrantKind::Rename, "legacy-123", 100000, clock)?;
    let report = runner.run(
        &mut |input| {
            assert_eq!(
                input, expected,
                "guest request must match independent core encoding"
            );
            host.dispatch(&a, input, || {
                clock += 1;
                clock
            })
            .map_err(|_| ())
        },
        Cancellation::default(),
    );
    assert_eq!(report.outcome, Ok(20));
    assert_eq!(report.host_calls, 1);
    let report = runner.run(
        &mut |input| {
            assert_eq!(
                input, expected,
                "guest request must match independent core encoding"
            );
            host.dispatch(&a, input, || {
                clock += 1;
                clock
            })
            .map_err(|_| ())
        },
        Cancellation::default(),
    );
    assert_eq!(report.outcome, Ok(20));
    let report = runner.run(
        &mut |input| {
            assert_eq!(
                input, expected,
                "guest request must match independent core encoding"
            );
            host.dispatch(&b, input, || {
                clock += 1;
                clock
            })
            .map_err(|_| ())
        },
        Cancellation::default(),
    );
    assert_eq!(report.outcome, Ok(10));
    host.revoke(&mut a, GrantKind::Rename, "legacy-123")?;
    let report = runner.run(
        &mut |input| {
            assert_eq!(
                input, expected,
                "guest request must match independent core encoding"
            );
            host.dispatch(&a, input, || {
                clock += 1;
                clock
            })
            .map_err(|_| ())
        },
        Cancellation::default(),
    );
    assert_eq!(report.outcome, Ok(10));
    // Cancelling after a durable submission suppresses the reply, never rolls it back.
    host.grant(&mut a, GrantKind::Rename, "legacy-123", 100000, clock)?;
    let cancel = Cancellation::default();
    let signal = cancel.clone();
    let report = runner.run(
        &mut |input| {
            assert_eq!(
                input, expected,
                "guest request must match independent core encoding"
            );
            let result = host
                .dispatch(&a, input, || {
                    clock += 1;
                    clock
                })
                .map_err(|_| ());
            signal.cancel();
            result
        },
        cancel,
    );
    assert_eq!(report.outcome, Err(Fault::Cancelled));
    assert_eq!(report.host_calls, 1);
    host.disconnect(&a)?;
    let report = runner.run(
        &mut |input| {
            assert_eq!(
                input, expected,
                "guest request must match independent core encoding"
            );
            host.dispatch(&a, input, || {
                clock += 1;
                clock
            })
            .map_err(|_| ())
        },
        Cancellation::default(),
    );
    assert_eq!(report.outcome, Ok(10));
    host.disconnect(&b)?;
    host.store_local().integrity_check()?;
    assert_eq!(host.store_local().pending(0, 10)?.len(), 2);
    drop(host);
    let store = Store::open_existing(&db, EventBudget::default())?;
    let result = store.lookup_for_card("legacy-123", "wasm-op")?;
    assert!(matches!(result,Lookup::Committed(r) if r.revision==2));
    store.integrity_check()?;
    println!(
        "PASS: {path}: actual SDK Wasm guest executed with denied/granted/deduplicated/cross-connection/revoked/stopped outcomes; cancelled reply kept durable revision 2; reopened SQLite verified"
    );
    Ok(())
}
