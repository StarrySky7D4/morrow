//! Packaged SDK module -> immutable catalog -> prepared/bound execution -> SQLite.
use morrow_core::{
    content::CardRecord,
    dispatch::HostRuntime,
    lifecycle::GrantKind,
    plugin_package::{
        Package,
        catalog::{self, Catalog},
    },
    store::{EventBudget, Store},
    transaction::Lookup,
};
use morrow_plugin_runtime::{Cancellation, Fault, Limits, package::PreparedPackage};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let paths: Vec<_> = std::env::args().skip(1).collect();
    if paths.is_empty() {
        return Err("package paths required".into());
    }
    for path in paths {
        let dir = tempfile::tempdir()?;
        let catalog = Catalog::open(&dir.path().join("packages"))?;
        let package = catalog::read_file(std::path::Path::new(&path))?;
        let digest = package.digest();
        let installed = catalog.install(&package)?;
        assert_eq!(catalog.install(&package)?, installed);
        let prepared = PreparedPackage::new(catalog.load(digest)?, Limits::default())
            .expect("prepare package");
        let db = dir.path().join("db");
        let mut store = Store::open(&db, EventBudget::default())?;
        store.create_local(
            "seed",
            &CardRecord::new("legacy-123", "morrow.note", 1, "old", vec![])?,
        )?;
        let mut host = HostRuntime::new(store)?;
        let mut a = prepared.connect(&mut host)?;
        let b = prepared.connect(&mut host)?;
        let mut ticks = 0u64;
        let mut clock = || {
            ticks += 1;
            ticks
        };
        let report = prepared.run(&mut host, &a, &mut clock, Cancellation::default());
        assert_eq!(report.outcome, Ok(10));
        assert_eq!(report.host_calls, 1);
        assert!(
            host.grant(
                &mut a,
                GrantKind::ReadSummary,
                "legacy-123",
                100000,
                clock()
            )
            .is_err()
        );
        host.grant(&mut a, GrantKind::Rename, "legacy-123", 100000, clock())?;
        for _ in 0..2 {
            let r = prepared.run(&mut host, &a, &mut clock, Cancellation::default());
            assert_eq!(r.outcome, Ok(20));
            assert_eq!(r.host_calls, 1);
        }
        assert_eq!(
            prepared
                .run(&mut host, &b, &mut clock, Cancellation::default())
                .outcome,
            Ok(10)
        );
        // Even the same module with different manifest bytes cannot borrow this connection.
        let mut m = prepared.package().manifest().clone();
        m.package_version = "0.1.9-test.11".into();
        let changed = PreparedPackage::new(
            Package::build(m, prepared.package().module())?,
            Limits::default(),
        )
        .unwrap();
        let r = changed.run(&mut host, &a, &mut clock, Cancellation::default());
        assert_eq!(r.outcome, Err(Fault::PackageBinding));
        assert_eq!(r.host_calls, 0);
        host.revoke(&mut a, GrantKind::Rename, "legacy-123")?;
        assert_eq!(
            prepared
                .run(&mut host, &a, &mut clock, Cancellation::default())
                .outcome,
            Ok(10)
        );
        host.grant(&mut a, GrantKind::Rename, "legacy-123", 100000, clock())?;
        let cancel = Cancellation::default();
        let signal = cancel.clone();
        let r = prepared.run(
            &mut host,
            &a,
            || {
                signal.cancel();
                clock()
            },
            cancel,
        );
        assert_eq!(r.outcome, Err(Fault::Cancelled));
        assert_eq!(r.host_calls, 1);
        host.disconnect(&a)?;
        assert_eq!(
            prepared
                .run(&mut host, &a, &mut clock, Cancellation::default())
                .outcome,
            Err(Fault::InactiveConnection)
        );
        host.disconnect(&b)?;
        host.store_local().integrity_check()?;
        assert_eq!(host.store_local().pending(0, 10)?.len(), 2);
        drop(host);
        let store = Store::open_existing(&db, EventBudget::default())?;
        assert!(
            matches!(store.lookup_for_card("legacy-123","wasm-op")?,Lookup::Committed(r) if r.revision==2)
        );
        store.integrity_check()?;
        println!(
            "PASS: {path}: installed/reloaded/prepared; declared ceiling, denied/granted/dedup/instance isolation/package replacement/revoked/cancelled/stopped; reopened revision 2"
        );
    }
    Ok(())
}
