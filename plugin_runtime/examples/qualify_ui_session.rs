//! Live in-memory Wasm UI event loop. No disk event handoff or Flutter execution is claimed.
use morrow_core::{
    content::CardRecord,
    dispatch::HostRuntime,
    plugin_package::{Package, proto::TransformHandler},
    store::{EventBudget, Store},
    ui::{Event, EventKind},
};
use morrow_plugin_runtime::{
    Limits,
    package::PreparedPackage,
    ui_session::{Error, Failure, UiSession, Update},
};
use std::time::{Duration, Instant};
fn poll(s: &mut UiSession) -> Update {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(v) = s.poll() {
            return v;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn admit(s: &mut UiSession, e: &Event) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        match s.event(&e.encode().unwrap()) {
            Ok(_) => break,
            Err(Error::Busy) => {
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(1));
            }
            Err(e) => panic!("{e:?}"),
        }
    }
}
fn event(s: &UiSession, title: &str) -> Event {
    Event {
        view: s.view().into(),
        generation: s.generation(),
        revision: s.revision(),
        serial: s.serial() + 1,
        node: "title".into(),
        action: "title.edit".into(),
        kind: EventKind::EditText,
        text: title.into(),
        checked: false,
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let paths: Vec<_> = std::env::args().skip(1).collect();
    if paths.is_empty() {
        return Err("provide at least one real UI Wasm module".into());
    }
    for (index, path) in paths.iter().enumerate() {
        let module = std::fs::read(path)?;
        let handlers = vec![
            TransformHandler {
                handler: "ui.form".into(),
                input_type: "text.utf8".into(),
                output_type: "morrow.ui.document.v1".into(),
                max_input_bytes: 32,
                max_output_bytes: 65536,
            },
            TransformHandler {
                handler: "ui.edit".into(),
                input_type: "morrow.ui.event.v1".into(),
                output_type: "morrow.ui.document.v1".into(),
                max_input_bytes: 65536,
                max_output_bytes: 65536,
            },
        ];
        let package = Package::build(
            Package::manifest_for_transform(
                &format!("org.morrow.online-ui-{index}"),
                "0.1.9-test.24",
                &module,
                handlers,
            ),
            &module,
        )?;
        let digest = package.digest();
        let dir = tempfile::tempdir()?;
        let db = dir.path().join("content.db");
        let mut store = Store::open(&db, EventBudget::default())?;
        store.create_local(
            "seed",
            &CardRecord::new("card", "note", 1, "untouched", vec![])?,
        )?;
        let host = HostRuntime::new(store)?;
        let package = PreparedPackage::new(package, Limits::default()).unwrap();
        let mut s =
            UiSession::new(package, host, "online-view", 17, Duration::from_secs(10)).unwrap();
        assert_eq!(s.package_digest(), digest);
        s.open_form("初始").unwrap();
        assert!(matches!(s.open_form("duplicate"), Err(Error::Busy)));
        assert!(matches!(poll(&mut s), Update::Document { revision: 1, .. }));
        for i in 0..24 {
            let title = format!("第{i}次🌈");
            let e = event(&s, &title);
            admit(&mut s, &e);
            assert!(matches!(s.event(&e.encode()?), Err(Error::Busy)));
            match poll(&mut s) {
                Update::Document {
                    ticket,
                    revision,
                    document,
                } => {
                    assert_eq!(revision, i + 2);
                    assert_eq!(ticket.serial, i + 1);
                    assert_eq!(document.nodes()[2].text, title)
                }
                other => panic!("{other:?}"),
            }
        }
        // Caller-created wrong generation/node/action/revision cannot get routed to guest.
        for case in 0..4 {
            let mut e = event(&s, "rejected");
            match case {
                0 => e.generation += 1,
                1 => e.node = "absent".into(),
                2 => e.action = "apply".into(),
                _ => e.revision -= 1,
            }
            let deadline = Instant::now() + Duration::from_secs(30);
            loop {
                assert!(Instant::now() < deadline);
                match s.event(&e.encode()?) {
                    Err(Error::Busy) => std::thread::sleep(Duration::from_millis(1)),
                    Err(Error::Core(_)) => break,
                    other => panic!("{other:?}"),
                }
            }
            assert_eq!(s.serial(), 24);
        }
        // Valid button admission produces correlated business failure, never saved UI.
        let mut e = event(&s, "");
        e.node = "apply".into();
        e.action = "apply".into();
        e.kind = EventKind::Activate;
        admit(&mut s, &e);
        assert!(matches!(
            poll(&mut s),
            Update::Failed {
                failure: Failure::Plugin(_),
                ..
            }
        ));
        assert_eq!(s.serial(), 25);
        assert_eq!(s.revision(), 25);
        let e = event(&s, "恢复后");
        admit(&mut s, &e);
        assert!(matches!(
            poll(&mut s),
            Update::Document { revision: 26, .. }
        ));
        // Discard both queued and already-completed results; neither may update the view.
        for completed in [false, true] {
            let revision = s.revision();
            let e = event(&s, "discarded");
            admit(&mut s, &e);
            if completed {
                let deadline = Instant::now() + Duration::from_secs(30);
                while s.execution_pending() {
                    assert!(Instant::now() < deadline);
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
            assert!(s.cancel_pending().is_some());
            std::thread::sleep(Duration::from_millis(80));
            assert!(s.poll().is_none());
            assert_eq!(s.revision(), revision);
        }
        let e = event(&s, "final");
        admit(&mut s, &e);
        assert!(matches!(
            poll(&mut s),
            Update::Document { revision: 27, .. }
        ));
        let e = event(&s, "closed");
        admit(&mut s, &e);
        s.close();
        assert!(matches!(s.event(&e.encode()?), Err(Error::Closed)));
        assert!(s.poll().is_none());
        let deadline = Instant::now() + Duration::from_secs(30);
        let host = loop {
            if let Some(host) = s.try_finish().unwrap() {
                break host;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        };
        assert_eq!(host.store_local().pending(0, 10)?.len(), 1);
        host.store_local().integrity_check()?;
        assert_eq!(
            host.store_local().card("card")?.unwrap().summary().title,
            "untouched"
        );
        println!(
            "PASS {path}: 27 live UI snapshots; repeated edit/backpressure; 4 identity/stale rejections; admitted business failure/recovery; cancelled/completed/closed result suppression; zero content mutations"
        );
    }
    Ok(())
}
