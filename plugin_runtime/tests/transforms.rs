use morrow_core::{
    runtime::Command,
    task::{Invocation, MAX_VALUE_BYTES, Transform, VERSION, schema_digest},
    task_capnp as wire,
};
use morrow_plugin_sdk::task::Invocation as Guest;
fn input(bytes: Vec<u8>) -> Invocation {
    Invocation::new_transform(
        "transform",
        Transform {
            handler: "bytes.reverse".into(),
            input_type: "bytes".into(),
            output_type: "bytes".into(),
            input: bytes,
        },
    )
    .unwrap()
}
#[test]
fn host_guest_transforms_preserve_empty_binary_and_maximum_data() {
    for bytes in [vec![], vec![0, 255, 65, 0], vec![121; MAX_VALUE_BYTES]] {
        let host = input(bytes.clone());
        let guest = Guest::decode(host.bytes()).unwrap();
        assert!(host.command().is_none());
        assert!(guest.request().is_none());
        assert_eq!(guest.transform().unwrap().input, bytes);
        let mut output = bytes;
        output.reverse();
        let completion = guest.output(&output).unwrap();
        assert_eq!(completion, host.output_completion(&output).unwrap());
        let value = host.verify_output(&completion).unwrap();
        assert_eq!(value.bytes, output);
        assert_eq!(value.type_id, "bytes");
        assert!(host.verify_completion(&completion, &[]).is_err());
        assert!(guest.completion(&[]).is_err());
    }
}
#[test]
fn transform_sizes_types_and_message_kinds_are_not_interchangeable() {
    let host = input(vec![1]);
    let guest = Guest::decode(host.bytes()).unwrap();
    assert!(guest.output(&vec![0; MAX_VALUE_BYTES + 1]).is_err());
    let mut t = host.transform().unwrap().clone();
    t.input = vec![0; MAX_VALUE_BYTES + 1];
    assert!(Invocation::new_transform("x", t).is_err());
    for (version, type_id, payload_len, with_response) in [
        (VERSION, "other", 1, false),
        (VERSION, "bytes", MAX_VALUE_BYTES + 1, false),
        (VERSION, "bytes", 1, true),
        (VERSION - 1, "bytes", 1, false),
    ] {
        let mut m = capnp::message::Builder::new_default();
        let mut r = m.init_root::<wire::completion::Builder>();
        r.set_version(version);
        r.set_schema_digest(&schema_digest());
        r.set_task_id(host.task_id());
        r.set_input_digest(&host.digest());
        r.set_kind(wire::Kind::Transform);
        if with_response {
            r.set_response(&[1]);
        }
        let mut o = r.init_output();
        o.set_type_id(type_id);
        o.set_bytes(&vec![0; payload_len]);
        assert!(
            host.verify_output(&capnp::serialize::write_message_to_words(&m))
                .is_err()
        );
    }
    let other = Invocation::new_transform("other", host.transform().unwrap().clone()).unwrap();
    assert!(other.verify_output(&guest.output(&[2]).unwrap()).is_err());
    for kind in [wire::Kind::ContentCommand, wire::Kind::Transform] {
        let mut m = capnp::message::Builder::new_default();
        let mut r = m.init_root::<wire::invocation::Builder>();
        r.set_version(VERSION);
        r.set_schema_digest(&schema_digest());
        r.set_task_id("mixed");
        r.set_kind(kind);
        r.set_command(
            &Command::ReadSummary {
                request_id: "read".into(),
                card_id: "card".into(),
            }
            .encode()
            .unwrap(),
        );
        let mut t = r.init_transform();
        t.set_handler("bytes.reverse");
        t.set_input_type("bytes");
        t.set_output_type("bytes");
        t.set_input(&[1]);
        let bytes = capnp::serialize::write_message_to_words(&m);
        assert!(Invocation::decode(&bytes).is_err());
        assert!(Guest::decode(&bytes).is_err());
    }
}
#[cfg(all(feature = "packages", not(target_arch = "wasm32")))]
#[test]
fn pure_transform_cannot_write_even_with_instance_grants_or_expose_output_after_trap() {
    use morrow_core::{
        content::CardRecord,
        dispatch::HostRuntime,
        lifecycle::GrantKind,
        plugin_package::{Package, proto::Capability},
        runtime::RenameRequest,
        store::{EventBudget, Store},
        transaction::Lookup,
    };
    use morrow_plugin_runtime::{Cancellation, Fault, Limits, package::PreparedPackage};
    let input = input(vec![1]);
    let done = input.output_completion(&[2]).unwrap();
    let command = RenameRequest {
        operation_id: "unexpected".into(),
        card_id: "card".into(),
        expected_revision: 1,
        title: "bad".into(),
    }
    .encode()
    .unwrap();
    let data = |bytes: &[u8]| {
        bytes
            .iter()
            .map(|b| format!("{}{:02x}", char::from(92), b))
            .collect::<String>()
    };
    for (call, after, expected) in [
        (true, "i32.const 0", Some(Fault::TaskProtocol)),
        (false, "unreachable", Some(Fault::Trap)),
        (false, "i32.const 0", None),
    ] {
        let exchange = if call {
            format!(
                "i32.const 132000 i32.const {} i32.const 196608 i32.const 65536 call $e drop",
                command.len()
            )
        } else {
            String::new()
        };
        let wasm=wat::parse_str(format!(r#"(module (import "morrow_task_v1" "read_input" (func $read(param i32 i32)(result i32))) (import "morrow_task_v1" "complete" (func $done(param i32 i32)(result i32))) (import "morrow_v1" "exchange" (func $e(param i32 i32 i32 i32)(result i32))) (memory(export "memory") 4) (data(i32.const 132000) "{}") (data(i32.const 140000) "{}") (func(export "morrow_run")(result i32) i32.const 0 i32.const 131072 call $read drop {exchange} i32.const 140000 i32.const {} call $done drop {after}))"#,data(&command),data(&done),done.len())).unwrap();
        let mut manifest = Package::manifest_for_transform(
            "transform",
            "1.0.0",
            &wasm,
            vec![morrow_core::plugin_package::proto::TransformHandler {
                handler: "bytes.reverse".into(),
                input_type: "bytes".into(),
                output_type: "bytes".into(),
                max_input_bytes: 65536,
                max_output_bytes: 65536,
            }],
        );
        manifest
            .requested_capabilities
            .push(Capability::RenameCard as i32);
        let package = Package::build(manifest, &wasm).unwrap();
        let p = PreparedPackage::new(package, Limits::default()).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(&dir.path().join("db"), EventBudget::default()).unwrap();
        store
            .create_local(
                "seed",
                &CardRecord::new("card", "note", 1, "original", vec![]).unwrap(),
            )
            .unwrap();
        let mut host = HostRuntime::new(store).unwrap();
        let mut c = p.connect(&mut host).unwrap();
        host.grant(&mut c, GrantKind::Rename, "card", 100, 0)
            .unwrap();
        let result = p.run_task(
            &mut host,
            &c,
            &input,
            || panic!("pure profile must not enter core dispatch"),
            Cancellation::default(),
        );
        assert!(result.response.is_none());
        assert!(result.failure.is_none());
        if let Some(fault) = expected {
            assert_eq!(result.execution.outcome, Err(fault));
            assert!(result.output.is_none());
        } else {
            assert_eq!(result.execution.outcome, Ok(0));
            assert_eq!(result.output.unwrap().bytes, vec![2]);
        }
        assert!(matches!(
            host.store_local()
                .lookup_for_card("card", "unexpected")
                .unwrap(),
            Lookup::Absent
        ));
        assert_eq!(host.store_local().pending(0, 10).unwrap().len(), 1);
        let cancel = Cancellation::default();
        cancel.cancel();
        let result = p.run_task(&mut host, &c, &input, || panic!(), cancel);
        assert_eq!(result.execution.outcome, Err(Fault::Cancelled));
        assert!(result.output.is_none());
        assert!(result.response.is_none());
        assert!(result.failure.is_none());
    }
}

#[cfg(all(feature = "packages", not(target_arch = "wasm32")))]
#[test]
fn registrations_reject_before_guest_and_bound_completed_output() {
    use morrow_core::{
        dispatch::HostRuntime,
        plugin_package::{Package, proto::TransformHandler},
        store::{EventBudget, Store},
    };
    use morrow_plugin_runtime::{Cancellation, Fault, Limits, package::PreparedPackage};
    let input = input(vec![1, 2]);
    let completion = input.output_completion(&[2, 1]).unwrap();
    let data: String = completion
        .iter()
        .map(|b| format!("{}{:02x}", char::from(92), b))
        .collect();
    let wasm = wat::parse_str(format!(
        r#"(module
        (import "morrow_task_v1" "read_input" (func $read(param i32 i32)(result i32)))
        (import "morrow_task_v1" "complete" (func $done(param i32 i32)(result i32)))
        (memory(export "memory") 3) (data(i32.const 140000) "{data}")
        (func(export "morrow_run")(result i32)
          i32.const 0 i32.const 131072 call $read drop
          i32.const 140000 i32.const {} call $done drop i32.const 0))"#,
        completion.len()
    ))
    .unwrap();
    for case in 0..7 {
        let mut h = TransformHandler {
            handler: "bytes.reverse".into(),
            input_type: "bytes".into(),
            output_type: "bytes".into(),
            max_input_bytes: 2,
            max_output_bytes: 2,
        };
        match case {
            1 => h.handler = "other".into(),
            2 => h.input_type = "other".into(),
            3 => h.output_type = "other".into(),
            4 => h.max_input_bytes = 1,
            5 => h.max_output_bytes = 1,
            _ => {}
        }
        let manifest = if case == 0 {
            Package::manifest_for_task("unregistered", "1.0.0", &wasm, vec![])
        } else {
            Package::manifest_for_transform("registered", "1.0.0", &wasm, vec![h])
        };
        let p = PreparedPackage::new(Package::build(manifest, &wasm).unwrap(), Limits::default())
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
            || panic!("pure task entered core"),
            Cancellation::default(),
        );
        assert!(result.response.is_none());
        assert!(result.failure.is_none());
        assert_eq!(result.execution.host_calls, 0);
        if case < 6 {
            assert_eq!(result.execution.outcome, Err(Fault::TaskProtocol));
            assert!(result.output.is_none());
        } else {
            assert_eq!(result.execution.outcome, Ok(0));
            assert_eq!(result.output.unwrap().bytes, vec![2, 1]);
        }
        if case < 5 {
            assert_eq!(
                result.execution.fuel_remaining,
                p.limits().fuel,
                "guest must not run for case {case}"
            );
        } else {
            assert!(result.execution.fuel_remaining < p.limits().fuel);
        }
        assert!(host.store_local().pending(0, 10).unwrap().is_empty());
    }
}
