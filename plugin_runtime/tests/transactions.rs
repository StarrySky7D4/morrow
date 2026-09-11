use morrow_core::{
    content::CardRecord,
    dispatch::HostRuntime,
    lifecycle::GrantKind,
    runtime::RenameRequest,
    store::{EventBudget, Store},
    transaction::Lookup,
};
use morrow_plugin_runtime::{Cancellation, Fault, Limits, Runner};
#[test]
fn cancellation_trap_and_fuel_after_first_commit_never_roll_back() {
    for (after, cancelling, fault) in [
        ("i32.const 0", true, Fault::Cancelled),
        ("unreachable", false, Fault::Trap),
        ("(loop $again br $again) i32.const 0", false, Fault::Limits),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("db");
        let mut store = Store::open(&db, EventBudget::default()).unwrap();
        store
            .create_local(
                "seed",
                &CardRecord::new("card", "note", 1, "before", vec![]).unwrap(),
            )
            .unwrap();
        let mut host = HostRuntime::new(store).unwrap();
        let mut connection = host.connect().unwrap();
        host.grant(&mut connection, GrantKind::Rename, "card", 100, 0)
            .unwrap();
        let request = RenameRequest {
            operation_id: "op".into(),
            card_id: "card".into(),
            expected_revision: 1,
            title: "after".into(),
        }
        .encode()
        .unwrap();
        let data = request
            .iter()
            .map(|b| format!("{}{:02x}", char::from(92), b))
            .collect::<String>();
        let wasm=wat::parse_str(format!(r#"(module (import "morrow_v1" "exchange" (func $call (param i32 i32 i32 i32) (result i32))) (memory (export "memory") 2) (data (i32.const 0) "{data}") (func (export "morrow_run") (result i32) i32.const 0 i32.const {} i32.const 65536 i32.const 65536 call $call drop {after}))"#,request.len())).unwrap();
        let runner = Runner::new(
            &wasm,
            Limits {
                fuel: 1000,
                ..Limits::default()
            },
        )
        .unwrap();
        let cancel = Cancellation::default();
        let signal = cancel.clone();
        let report = runner.run(
            &mut |input| {
                let result = host.dispatch(&connection, input, || 1).map_err(|_| ());
                if cancelling {
                    signal.cancel();
                }
                result
            },
            cancel,
        );
        assert_eq!(report.outcome, Err(fault));
        assert_eq!(report.host_calls, 1);
        host.disconnect(&connection).unwrap();
        drop(host);
        let store = Store::open_existing(&db, EventBudget::default()).unwrap();
        assert!(
            matches!(store.lookup_for_card("card","op").unwrap(),Lookup::Committed(r) if r.revision==2)
        );
        assert_eq!(store.pending(0, 10).unwrap().len(), 2);
        store.integrity_check().unwrap();
    }
}
