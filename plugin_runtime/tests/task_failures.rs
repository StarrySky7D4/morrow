use morrow_core::task::{
    FailureCode, Invocation, PluginFailure, Transform, TransformResult, VERSION, schema_digest,
};
use morrow_core::task_capnp as wire;
use morrow_plugin_sdk::task::{FailureCode as GuestCode, Invocation as Guest};
fn task(data: Vec<u8>) -> Invocation {
    Invocation::new_transform(
        "task",
        Transform {
            handler: "validate".into(),
            input_type: "bytes".into(),
            output_type: "bytes".into(),
            input: data,
        },
    )
    .unwrap()
}
#[test]
fn independent_failure_codecs_bind_all_codes_and_unicode_messages_to_input() {
    let input = task(vec![1]);
    let guest = Guest::decode(input.bytes()).unwrap();
    for (code, guest_code) in [
        (FailureCode::InvalidInput, GuestCode::InvalidInput),
        (FailureCode::UnsupportedInput, GuestCode::UnsupportedInput),
        (FailureCode::ResourceLimit, GuestCode::ResourceLimit),
        (FailureCode::Failed, GuestCode::Failed),
    ] {
        for message in ["输入不受支持🌈".to_owned(), "x".repeat(1024)] {
            let failure = PluginFailure { code, message };
            let bytes = guest.failure(guest_code, &failure.message).unwrap();
            assert_eq!(bytes, input.failure_completion(&failure).unwrap());
            assert_eq!(
                input.verify_transform_result(&bytes).unwrap(),
                TransformResult::Failure(failure)
            );
            assert!(input.verify_output(&bytes).is_err());
            assert!(input.verify_completion(&bytes, &[]).is_err());
            assert!(task(vec![2]).verify_transform_result(&bytes).is_err());
            assert!(
                input
                    .verify_transform_result(&bytes[..bytes.len() - 1])
                    .is_err()
            );
        }
    }
    for message in [
        String::new(),
        "x".repeat(1025),
        "bad\nmessage".into(),
        "bad\0message".into(),
    ] {
        assert!(guest.failure(GuestCode::Failed, &message).is_err());
    }
}
#[test]
fn malformed_or_mixed_failures_do_not_become_results() {
    let input = task(vec![]);
    for case in 0..9 {
        let mut m = capnp::message::Builder::new_default();
        let mut r = m.init_root::<wire::completion::Builder>();
        r.set_version(if case == 0 { VERSION - 1 } else { VERSION });
        r.set_schema_digest(&schema_digest());
        r.set_task_id(if case == 1 { "other" } else { input.task_id() });
        r.set_input_digest(&input.digest());
        r.set_kind(if case == 2 {
            wire::Kind::ContentCommand
        } else {
            wire::Kind::Transform
        });
        if case == 3 {
            r.set_response(&[1]);
        }
        if case == 4 {
            let mut o = r.reborrow().init_output();
            o.set_type_id("bytes");
            o.set_bytes(&[]);
        }
        if case != 5 {
            let mut f = r.init_failure();
            f.set_code(FailureCode::Failed);
            f.set_message(if case == 6 {
                ""
            } else if case == 7 {
                "bad\ntext"
            } else {
                "failure"
            });
        }
        if case == 8 {
            let mut root = m.get_root::<wire::completion::Builder>().unwrap();
            root.reborrow()
                .get_failure()
                .unwrap()
                .set_message("x".repeat(1025));
        }
        assert!(
            input
                .verify_transform_result(&capnp::serialize::write_message_to_words(&m))
                .is_err(),
            "case {case}"
        );
    }
}
#[test]
fn plugin_failure_cannot_replace_or_accompany_core_response() {
    use morrow_core::{
        response::{Failure, Outcome, Response},
        runtime::Command,
    };
    let input = Invocation::new(
        "content",
        &Command::ReadSummary {
            request_id: "read".into(),
            card_id: "card".into(),
        },
    )
    .unwrap();
    let actual = Response {
        request_id: "read".into(),
        outcome: Outcome::Rejected(Failure::Denied),
    }
    .encode()
    .unwrap();
    assert!(
        input
            .failure_completion(&PluginFailure {
                code: FailureCode::Failed,
                message: "failure".into()
            })
            .is_err()
    );
    assert!(
        Guest::decode(input.bytes())
            .unwrap()
            .failure(GuestCode::Failed, "failure")
            .is_err()
    );
    let mut m = capnp::message::Builder::new_default();
    let mut r = m.init_root::<wire::completion::Builder>();
    r.set_version(VERSION);
    r.set_schema_digest(&schema_digest());
    r.set_task_id(input.task_id());
    r.set_input_digest(&input.digest());
    r.set_kind(wire::Kind::ContentCommand);
    r.set_response(&actual);
    let mut f = r.init_failure();
    f.set_code(FailureCode::Failed);
    f.set_message("failure");
    assert!(
        input
            .verify_completion(&capnp::serialize::write_message_to_words(&m), &actual)
            .is_err()
    );
}

#[cfg(all(feature = "packages", not(target_arch = "wasm32")))]
#[test]
fn guest_business_failure_is_distinct_from_trap_protocol_fault_and_cancel() {
    use morrow_core::{
        dispatch::HostRuntime,
        plugin_package::{Package, proto::TransformHandler},
        store::{EventBudget, Store},
    };
    use morrow_plugin_runtime::{Cancellation, Fault, Limits, package::PreparedPackage};
    let input = task(vec![255]);
    let expected = PluginFailure {
        code: FailureCode::UnsupportedInput,
        message: "Input rejected".into(),
    };
    let done = input.failure_completion(&expected).unwrap();
    let data: String = done
        .iter()
        .map(|b| format!("{}{:02x}", char::from(92), b))
        .collect();
    for (after, expected_fault) in [
        ("i32.const 0", None),
        ("unreachable", Some(Fault::Trap)),
        ("i32.const 1", Some(Fault::TaskProtocol)),
    ] {
        let wasm = wat::parse_str(format!(
            r#"(module
          (import "morrow_task_v1" "read_input" (func $read(param i32 i32)(result i32)))
          (import "morrow_task_v1" "complete" (func $done(param i32 i32)(result i32)))
          (memory(export "memory") 3)(data(i32.const 140000) "{data}")
          (func(export "morrow_run")(result i32) i32.const 0 i32.const 131072 call $read drop
          i32.const 140000 i32.const {} call $done drop {after}))"#,
            done.len()
        ))
        .unwrap();
        let p = PreparedPackage::new(
            Package::build(
                Package::manifest_for_transform(
                    "errors",
                    "1.0.0",
                    &wasm,
                    vec![TransformHandler {
                        handler: "validate".into(),
                        input_type: "bytes".into(),
                        output_type: "bytes".into(),
                        max_input_bytes: 1,
                        max_output_bytes: 0,
                    }],
                ),
                &wasm,
            )
            .unwrap(),
            Limits::default(),
        )
        .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let mut host =
            HostRuntime::new(Store::open(&dir.path().join("db"), EventBudget::default()).unwrap())
                .unwrap();
        let c = p.connect(&mut host).unwrap();
        let result = p.run_task(
            &mut host,
            &c,
            &input,
            || panic!("pure failure entered core"),
            Cancellation::default(),
        );
        assert!(result.output.is_none() && result.response.is_none());
        assert_eq!(result.execution.host_calls, 0);
        match expected_fault {
            Some(f) => {
                assert_eq!(result.execution.outcome, Err(f));
                assert!(result.failure.is_none());
            }
            None => {
                assert_eq!(result.execution.outcome, Ok(0));
                assert_eq!(result.failure, Some(expected.clone()));
            }
        }
        let cancel = Cancellation::default();
        cancel.cancel();
        let r = p.run_task(&mut host, &c, &input, || panic!(), cancel);
        assert_eq!(r.execution.outcome, Err(Fault::Cancelled));
        assert!(r.failure.is_none() && r.output.is_none() && r.response.is_none());
        assert!(host.store_local().pending(0, 10).unwrap().is_empty());
    }
}
