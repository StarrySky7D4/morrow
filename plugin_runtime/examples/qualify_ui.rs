//! Actual three-language guest UI output, independently decoded by trusted core.
use morrow_core::{
    content::CardRecord,
    dispatch::HostRuntime,
    plugin_package::catalog,
    store::{EventBudget, Store},
    task::{FailureCode, Invocation, Transform},
    ui::{Document, Session},
};
use morrow_plugin_runtime::{
    Limits,
    package::PreparedPackage,
    worker::{Phase, Worker},
};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let output = PathBuf::from(args.next().ok_or("output directory required")?);
    let event = std::fs::read(args.next().ok_or("actual widget event required")?)?;
    std::fs::create_dir_all(&output)?;
    let paths: Vec<_> = args.collect();
    if paths.is_empty() {
        return Err("UI packages required".into());
    }
    for (index, path) in paths.iter().enumerate() {
        let p = PreparedPackage::new(
            catalog::read_file(std::path::Path::new(path))?,
            Limits::default(),
        )
        .unwrap();
        assert!(p.package().capabilities().is_empty());
        assert_eq!(p.package().manifest().transform_handlers.len(), 2);
        let dir = tempfile::tempdir()?;
        let db = dir.path().join("db");
        let mut store = Store::open(&db, EventBudget::default())?;
        store.create_local(
            "seed",
            &CardRecord::new("card", "note", 1, "untouched", vec![])?,
        )?;
        let mut host = HostRuntime::new(store)?;
        let c = p.connect(&mut host)?;
        for (handler, input_type, input) in [
            ("missing", "text.utf8", vec![]),
            ("ui.form", "wrong", vec![]),
            ("ui.form", "text.utf8", vec![b'a'; 33]),
        ] {
            let invocation = Invocation::new_transform(
                "rejected",
                Transform {
                    handler: handler.into(),
                    input_type: input_type.into(),
                    output_type: "morrow.ui.document.v1".into(),
                    input,
                },
            )?;
            let r = p.run_task(
                &mut host,
                &c,
                &invocation,
                || panic!("pure UI cannot call core"),
                Default::default(),
            );
            assert_eq!(
                r.execution.outcome,
                Err(morrow_plugin_runtime::Fault::TaskProtocol)
            );
            assert_eq!(r.execution.fuel_remaining, p.limits().fuel);
            assert_eq!(r.execution.host_calls, 0);
            assert!(r.output.is_none());
        }
        let mut worker =
            Worker::spawn(p, host, c, || panic!("pure UI cannot call core"), 16).unwrap();
        let timeout = Duration::from_secs(30);
        let samples = [
            "灵感🌈",
            "",
            "abcdefghabcdefghabcdefghabcdefgh",
            "🌈🌈🌈🌈🌈🌈🌈🌈",
        ];
        let mut cases: Vec<(&str, Vec<u8>, Option<String>)> = samples
            .iter()
            .map(|s| ("ui.form", s.as_bytes().to_vec(), Some((*s).into())))
            .collect();
        cases.extend([
            ("ui.form", vec![0xff], None),
            ("ui.form", vec![0], None),
            ("ui.edit", vec![0], None),
        ]);
        let mut session = Session::new("view", u64::MAX)?;
        // The event is produced by actual Flutter input in verify_plugin_renderer, not this qualifier.
        let mut checked = 0;
        for (case, (handler, input, expected)) in cases.into_iter().enumerate() {
            let invocation = Invocation::new_transform(
                &format!("ui-{case}"),
                Transform {
                    handler: handler.into(),
                    input_type: if handler == "ui.form" {
                        "text.utf8"
                    } else {
                        "morrow.ui.event.v1"
                    }
                    .into(),
                    output_type: "morrow.ui.document.v1".into(),
                    input,
                },
            )?;
            let mut task = worker.submit_task(invocation, timeout).unwrap();
            let deadline = Instant::now() + timeout;
            let r = loop {
                if let Some(r) = task.try_result().unwrap() {
                    break r;
                }
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(1));
            };
            assert_eq!(r.execution.outcome, Ok(0));
            assert_eq!(r.execution.host_calls, 0);
            assert!(r.response.is_none());
            if let Some(title) = expected {
                assert!(r.failure.is_none());
                let value = r.output.unwrap();
                assert_eq!(value.type_id, "morrow.ui.document.v1");
                let doc = Document::decode(&value.bytes)?;
                let mut expected =
                    Document::decode(include_bytes!("../../sdk/tests/ui_fixtures/document.capnp"))?
                        .nodes()
                        .to_vec();
                expected[2].text = title;
                assert_eq!(doc, Document::new(expected)?);
                if case == 0 {
                    session.replace(0, doc)?;
                    std::fs::write(
                        output.join(format!("guest-{index}-form.capnp")),
                        &value.bytes,
                    )?;
                }
            } else {
                assert!(r.output.is_none());
                let failure = r.failure.unwrap();
                assert_eq!(failure.code, FailureCode::InvalidInput);
                assert_eq!(failure.message, "Invalid form input");
            }
            checked += 1;
        }
        let accepted = session.accept(&event)?;
        assert_eq!(accepted.text, "从 Dart 编辑🌈");
        assert!(session.accept(&event).is_err());
        let invocation = Invocation::new_transform(
            "edit",
            Transform {
                handler: "ui.edit".into(),
                input_type: "morrow.ui.event.v1".into(),
                output_type: "morrow.ui.document.v1".into(),
                input: event.clone(),
            },
        )?;
        let mut task = worker.submit_task(invocation, timeout).unwrap();
        worker.drain(timeout).unwrap();
        let deadline = Instant::now() + timeout;
        let r = loop {
            if let Some(r) = task.try_result().unwrap() {
                break r;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        };
        assert_eq!(r.execution.outcome, Ok(0));
        assert_eq!(r.execution.host_calls, 0);
        assert!(r.failure.is_none() && r.response.is_none());
        let value = r.output.unwrap();
        assert_eq!(value.type_id, "morrow.ui.document.v1");
        let doc = Document::decode(&value.bytes)?;
        let mut expected =
            Document::decode(include_bytes!("../../sdk/tests/ui_fixtures/document.capnp"))?
                .nodes()
                .to_vec();
        expected[2].text = accepted.text;
        assert_eq!(doc, Document::new(expected)?);
        assert_eq!(session.replace(1, doc)?, 2);
        assert!(session.accept(&event).is_err());
        session.close();
        std::fs::write(
            output.join(format!("guest-{index}-updated.capnp")),
            value.bytes,
        )?;
        let host = loop {
            if let Some(h) = worker.try_finish().unwrap() {
                break h;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        };
        assert_eq!(worker.phase(), Phase::Stopped);
        assert_eq!(host.store_local().pending(0, 10)?.len(), 1);
        host.store_local().integrity_check()?;
        drop(host);
        assert_eq!(
            Store::open_existing(&db, EventBudget::default())?
                .card("card")?
                .unwrap()
                .summary()
                .title,
            "untouched"
        );
        println!(
            "PASS: {path}: 3 registration rejections; {} real tasks, 5 validated UI outputs and 3 correlated failures; admitted Flutter edit, stale event rejection, zero core calls/content changes",
            checked + 1
        );
    }
    Ok(())
}
