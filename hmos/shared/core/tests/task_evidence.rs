use morrow_core::{
    Error,
    plugin_package::{MAX_MODULE_BYTES, Package, proto as package_proto},
    task::{FailureCode, Invocation, MAX_TASK_BYTES, MAX_VALUE_BYTES, PluginFailure, Transform},
    task_evidence::{
        self as evidence,
        proto::{ExecutionBudget, StableFault, TaskEvidence},
    },
};
use prost::Message;
use sha2::{Digest, Sha256};
fn invocation() -> Invocation {
    Invocation::new_transform(
        "task",
        Transform {
            handler: "plain.transform".into(),
            input_type: "bytes".into(),
            output_type: "answer".into(),
            input: vec![0, 1, 255],
        },
    )
    .unwrap()
}
fn package(module: &[u8]) -> Package {
    Package::build(
        Package::manifest_for_transform(
            "test.evidence",
            "1.0.0",
            module,
            vec![package_proto::TransformHandler {
                handler: "plain.transform".into(),
                input_type: "bytes".into(),
                output_type: "answer".into(),
                max_input_bytes: 65536,
                max_output_bytes: 65536,
            }],
        ),
        module,
    )
    .unwrap()
}
fn data() -> TaskEvidence {
    let p = package(b"\0asm\x01\0\0\0");
    let task = invocation();
    TaskEvidence {
        schema_version: 1,
        package_archive: p.archive().to_vec(),
        invocation: task.bytes().to_vec(),
        budget: Some(ExecutionBudget {
            fuel: 100000,
            memory_bytes: 65536,
            host_calls: 4,
        }),
        backend: evidence::BACKEND.into(),
        completion: task.output_completion(&[9, 0, 255]).unwrap(),
        fault: StableFault::Unspecified as i32,
        exit_code: Some(0),
        observed_host_calls: 0,
        fuel_remaining: 90000,
        batch: None,
    }
}
fn pack_raw(raw: &[u8]) -> (Vec<u8>, [u8; 32]) {
    let packed = lz4_flex::block::compress(raw);
    let digest: [u8; 32] = Sha256::digest(raw).into();
    let mut container = b"MORROWE1".to_vec();
    container.extend(1u16.to_le_bytes());
    container.extend((raw.len() as u32).to_le_bytes());
    container.extend((packed.len() as u32).to_le_bytes());
    container.extend(digest);
    container.extend(packed);
    (container, digest)
}
fn raw_result(raw: &[u8]) -> morrow_core::Result<evidence::Evidence> {
    let (c, d) = pack_raw(raw);
    evidence::decode(&c, d)
}
#[test]
fn completion_roundtrip_retains_original_bytes_and_fixed_external_digest() {
    let input = data();
    let e = evidence::encode(input.clone()).unwrap();
    assert_eq!(e.data(), &input);
    assert_eq!(e.raw(), input.encode_to_vec());
    assert_eq!(e.digest(), <[u8; 32]>::from(Sha256::digest(e.raw())));
    let d = evidence::decode(e.container(), e.digest()).unwrap();
    assert_eq!(d.raw(), e.raw());
    assert_eq!(d.container(), e.container());
    assert!(matches!(
        evidence::decode(e.container(), [0; 32]),
        Err(Error::Integrity)
    ));
    let mut changed = input;
    changed.exit_code = Some(7);
    let altered = evidence::encode(changed).unwrap();
    assert!(matches!(
        evidence::decode(altered.container(), e.digest()),
        Err(Error::Integrity)
    ));
}
#[test]
fn plugin_failures_and_all_stable_faults_have_distinct_observation_rules() {
    let mut d = data();
    d.completion = invocation()
        .failure_completion(&PluginFailure {
            code: FailureCode::UnsupportedInput,
            message: "unsupported".into(),
        })
        .unwrap();
    evidence::encode(d).unwrap();
    for fault in [
        StableFault::TaskProtocol,
        StableFault::Deadline,
        StableFault::PackageBinding,
        StableFault::InactiveConnection,
        StableFault::InvalidModule,
        StableFault::UnsupportedAbi,
        StableFault::Limits,
        StableFault::Cancelled,
        StableFault::Trap,
    ] {
        let mut d = data();
        d.fault = fault as i32;
        d.exit_code = None;
        d.completion = vec![255, 0, 1];
        let e = evidence::encode(d).unwrap();
        assert_eq!(
            evidence::decode(e.container(), e.digest())
                .unwrap()
                .data()
                .completion,
            [255, 0, 1]
        );
        let mut mixed = e.data().clone();
        mixed.exit_code = Some(0);
        assert!(evidence::encode(mixed).is_err());
    }
    for completion in [
        vec![],
        vec![255],
        Invocation::new_transform("another", invocation().transform().unwrap().clone())
            .unwrap()
            .output_completion(b"unrelated")
            .unwrap(),
    ] {
        let mut d = data();
        d.completion = completion;
        assert!(evidence::encode(d).is_err());
    }
    let mut missing = data();
    missing.exit_code = None;
    assert!(evidence::encode(missing).is_err());
}
#[test]
fn unknown_optional_fields_are_retained_but_unknown_versions_and_faults_rejected() {
    let mut raw = data().encode_to_vec();
    raw.extend([0xa2, 0x06, 3, 1, 2, 3]); // opaque future field 100
    let e = raw_result(&raw).unwrap();
    assert_eq!(e.raw(), raw);
    assert_ne!(e.raw(), e.data().encode_to_vec());
    for version in [0, 3, u32::MAX] {
        let mut d = data();
        d.schema_version = version;
        assert!(matches!(
            raw_result(&d.encode_to_vec()),
            Err(Error::UnsupportedVersion)
        ));
    }
    let mut d = data();
    d.fault = 999;
    assert!(matches!(
        raw_result(&d.encode_to_vec()),
        Err(Error::UnsupportedVersion)
    ));
    let mut d = data();
    d.backend = "other-runtime/2".into();
    assert!(evidence::encode(d).is_ok());
}
#[test]
fn container_lengths_compression_tampering_and_tail_are_bounded() {
    let e = evidence::encode(data()).unwrap();
    for offset in [0, 8, 10, 14, 18, 50, e.container().len() - 1] {
        let mut c = e.container().to_vec();
        c[offset] ^= 0xff;
        assert!(evidence::decode(&c, e.digest()).is_err(), "offset {offset}");
    }
    for length in [0, 8, 49, e.container().len() - 1] {
        assert!(evidence::decode(&e.container()[..length], e.digest()).is_err());
    }
    let mut tail = e.container().to_vec();
    tail.push(0);
    assert!(evidence::decode(&tail, e.digest()).is_err());
    let mut bomb = e.container().to_vec();
    bomb[10..14].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(matches!(
        evidence::decode(&bomb, e.digest()),
        Err(Error::Limit)
    ));
    assert!(matches!(
        evidence::decode(&vec![0; evidence::MAX_CONTAINER_BYTES + 1], e.digest()),
        Err(Error::Limit)
    ));
}
#[test]
fn protobuf_duplicates_groups_field_flood_and_oversized_spans_reject_before_decode() {
    for suffix in [
        vec![8, 1],
        vec![0xa3, 0x06, 0xa4, 0x06],
        vec![0x12, 0xff, 0xff, 0xff, 0xff, 0x0f],
        vec![0x1a, 0x80, 0x80, 0x80, 0x01],
    ] {
        let mut raw = data().encode_to_vec();
        raw.extend(suffix);
        assert!(raw_result(&raw).is_err());
    }
    let mut raw = data().encode_to_vec();
    for _ in 0..129 {
        raw.extend([0xa0, 0x06, 1]);
    }
    assert!(matches!(raw_result(&raw), Err(Error::Limit)));
    let mut raw = data().encode_to_vec();
    raw.extend([0]);
    assert!(raw_result(&raw).is_err());
    // A duplicate field nested inside budget is rejected, not silently merged.
    let mut d = data();
    d.budget = None;
    let mut raw = d.encode_to_vec();
    raw.extend([0x22, 4, 8, 1, 8, 2]);
    assert!(raw_result(&raw).is_err());
}
#[test]
fn budgets_observed_usage_and_backend_limits_are_checked() {
    for case in 0..10 {
        let mut d = data();
        let b = d.budget.as_mut().unwrap();
        match case {
            0 => b.fuel = 0,
            1 => b.fuel = 100_000_001,
            2 => b.memory_bytes = 65535,
            3 => b.memory_bytes = 64 * 1024 * 1024 + 65536,
            4 => b.host_calls = 1025,
            5 => d.observed_host_calls = b.host_calls + 1,
            6 => d.fuel_remaining = b.fuel + 1,
            7 => d.budget = None,
            8 => d.backend = String::new(),
            _ => d.backend = "x".repeat(129),
        }
        assert!(evidence::encode(d).is_err(), "case {case}");
    }
    let mut d = data();
    d.budget.as_mut().unwrap().fuel = 21_000_000;
    d.fuel_remaining = 0;
    assert!(matches!(evidence::encode(d), Err(Error::Limit)));
}
#[test]
fn content_commands_dynamic_dependency_packages_and_wrong_contracts_reject() {
    let mut d = data();
    d.invocation = Invocation::new(
        "read",
        &morrow_core::runtime::Command::ReadSummary {
            request_id: "read".into(),
            card_id: "card".into(),
        },
    )
    .unwrap()
    .bytes()
    .to_vec();
    assert!(evidence::encode(d).is_err());
    let p = package(b"\0asm\x01\0\0\0");
    let mut manifest = p.manifest().clone();
    manifest
        .required_features
        .push("dependency-calls-v1".into());
    manifest.required_features.push("dependencies-v1".into());
    manifest.dependency_schema_sha256 = morrow_core::dependency_call::schema_digest().to_vec();
    let dynamic = Package::build(manifest, b"\0asm\x01\0\0\0").unwrap();
    let mut d = data();
    d.package_archive = dynamic.archive().to_vec();
    assert!(evidence::encode(d).is_err());
    let mut d = data();
    let mut wrong = invocation().transform().unwrap().clone();
    wrong.handler = "unknown".into();
    let t = Invocation::new_transform("task", wrong).unwrap();
    d.invocation = t.bytes().to_vec();
    d.completion = t.output_completion(&[1]).unwrap();
    assert!(evidence::encode(d).is_err());
    let v1 = Package::build(
        Package::manifest_for("v1", "1.0.0", b"\0asm\x01\0\0\0", vec![]),
        b"\0asm\x01\0\0\0",
    )
    .unwrap();
    let mut d = data();
    d.package_archive = v1.archive().to_vec();
    assert!(evidence::encode(d).is_err());
}
#[test]
fn full_module_full_task_and_64k_output_fit_without_losing_original_frames() {
    // A syntactically valid 4 MiB Wasm custom section; core does not execute it.
    let mut module = b"\0asm\x01\0\0\0".to_vec();
    module.push(0);
    let mut n = MAX_MODULE_BYTES - 13;
    loop {
        let part = (n & 127) as u8;
        n >>= 7;
        module.push(part | if n > 0 { 128 } else { 0 });
        if n == 0 {
            break;
        }
    }
    module.push(0);
    let mut x = 0x12345678u32;
    while module.len() < MAX_MODULE_BYTES {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        module.push(x as u8);
    }
    let p = package(&module);
    let task = invocation();
    let mut frame = task.bytes().to_vec();
    let segments = u32::from_le_bytes(frame[0..4].try_into().unwrap()) as usize + 1;
    let last_length = 4 * segments;
    let words = u32::from_le_bytes(frame[last_length..last_length + 4].try_into().unwrap());
    let extra = (MAX_TASK_BYTES - frame.len()) / 8;
    frame[last_length..last_length + 4].copy_from_slice(&(words + extra as u32).to_le_bytes());
    frame.resize(MAX_TASK_BYTES, 0);
    let full = Invocation::decode(&frame).unwrap();
    let mut d = data();
    d.package_archive = p.archive().to_vec();
    d.invocation = frame;
    d.completion = full.output_completion(&vec![7; MAX_VALUE_BYTES]).unwrap();
    let e = evidence::encode(d).unwrap();
    let decoded = evidence::decode(e.container(), e.digest()).unwrap();
    assert_eq!(decoded.data().invocation.len(), MAX_TASK_BYTES);
    assert_eq!(decoded.data(), e.data());
    let mut oversized = e.data().clone();
    oversized.completion = vec![0; MAX_TASK_BYTES + 1];
    assert!(matches!(evidence::encode(oversized), Err(Error::Limit)));
}
#[test]
fn successful_output_respects_the_actual_package_registration() {
    let p = package(b"\0asm\x01\0\0\0");
    let mut m = p.manifest().clone();
    m.transform_handlers[0].max_output_bytes = 1;
    let p = Package::build(m, b"\0asm\x01\0\0\0").unwrap();
    let mut d = data();
    d.package_archive = p.archive().to_vec();
    assert!(matches!(evidence::encode(d), Err(Error::Limit)));
    let d = data();
    assert!(
        evidence::validate_plan(
            &package(b"\0asm\x01\0\0\0"),
            &invocation(),
            d.budget.as_ref().unwrap()
        )
        .is_ok()
    );
}

#[test]
fn byte_granularity_memory_ceiling_is_preserved_but_dependency_graphs_are_not_evidence() {
    let mut d = data();
    d.budget.as_mut().unwrap().memory_bytes = 65537;
    let e = evidence::encode(d).unwrap();
    assert_eq!(
        evidence::decode(e.container(), e.digest())
            .unwrap()
            .data()
            .budget
            .as_ref()
            .unwrap()
            .memory_bytes,
        65537
    );
    let p = package(b"\0asm\x01\0\0\0");
    let mut m = p.manifest().clone();
    m.required_features.push("dependencies-v1".into());
    m.dependencies.push(package_proto::DependencyRequirement {
        slot: "dependency".into(),
        handler: "plain.transform".into(),
        input_type: "bytes".into(),
        output_type: "answer".into(),
        provider_version: "*".into(),
        optional: true,
    });
    let p = Package::build(m, b"\0asm\x01\0\0\0").unwrap();
    let mut d = data();
    d.package_archive = p.archive().to_vec();
    assert!(evidence::encode(d).is_err());
}
