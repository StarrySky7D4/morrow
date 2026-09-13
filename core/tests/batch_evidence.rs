//! Synthetic codec fixtures, not claims of actual guest execution or source authenticity.
use morrow_core::{
    plugin_package::{Package, proto::TransformHandler},
    task::{FailureCode, Invocation, PluginFailure, Transform},
    task_evidence::{
        self as evidence,
        proto::{Batch, ExecutionBudget, Observation, StableFault, TaskEvidence},
    },
};
use prost::Message;
use sha2::{Digest, Sha256};
fn observation(i: usize) -> Observation {
    let invocation = Invocation::new_transform(
        &format!("page-{i}"),
        Transform {
            handler: "convert".into(),
            input_type: "bytes".into(),
            output_type: "bytes".into(),
            input: vec![i as u8],
        },
    )
    .unwrap();
    Observation {
        invocation: invocation.bytes().to_vec(),
        budget: Some(ExecutionBudget {
            fuel: 100000,
            memory_bytes: 65536,
            host_calls: 4,
        }),
        backend: evidence::BACKEND.into(),
        completion: invocation.output_completion(&[i as u8]).unwrap(),
        fault: 0,
        exit_code: Some(0),
        observed_host_calls: 0,
        fuel_remaining: 90000,
    }
}
fn data(count: usize) -> TaskEvidence {
    let module = b"\0asm\x01\0\0\0";
    let package = Package::build(
        Package::manifest_for_transform(
            "test.batch",
            "1.0.0",
            module,
            vec![TransformHandler {
                handler: "convert".into(),
                input_type: "bytes".into(),
                output_type: "bytes".into(),
                max_input_bytes: 65536,
                max_output_bytes: 65536,
            }],
        ),
        module,
    )
    .unwrap();
    TaskEvidence {
        schema_version: 2,
        package_archive: package.archive().to_vec(),
        batch: Some(Batch {
            intent_type: "test.intent.v1".into(),
            intent: b"stable user intent".to_vec(),
            total_fuel: 100000000,
            observations: (0..count).map(observation).collect(),
        }),
        ..Default::default()
    }
}
fn packed(raw: &[u8]) -> (Vec<u8>, [u8; 32]) {
    let compressed = lz4_flex::block::compress(raw);
    let digest: [u8; 32] = Sha256::digest(raw).into();
    let mut out = b"MORROWE1".to_vec();
    out.extend(1u16.to_le_bytes());
    out.extend((raw.len() as u32).to_le_bytes());
    out.extend((compressed.len() as u32).to_le_bytes());
    out.extend(digest);
    out.extend(compressed);
    (out, digest)
}
fn decode_raw(raw: &[u8]) -> morrow_core::Result<evidence::Evidence> {
    let (c, d) = packed(raw);
    evidence::decode(&c, d)
}
fn varint(mut n: u64) -> Vec<u8> {
    let mut out = vec![];
    while n >= 128 {
        out.push(n as u8 | 128);
        n >>= 7;
    }
    out.push(n as u8);
    out
}
fn field(number: u32, payload: &[u8]) -> Vec<u8> {
    let mut out = varint((u64::from(number) << 3) | 2);
    out.extend(varint(payload.len() as u64));
    out.extend(payload);
    out
}
fn with_batch_bytes(batch: &[u8]) -> Vec<u8> {
    let mut d = data(1);
    d.batch = None;
    let mut raw = d.encode_to_vec();
    raw.extend(field(11, batch));
    raw
}
#[test]
fn ordered_batch_roundtrip_retains_one_package_and_exact_unknown_bytes() {
    let d = data(3);
    let e = evidence::encode(d.clone()).unwrap();
    assert_eq!(e.data(), &d);
    let restored = evidence::decode(e.container(), e.digest()).unwrap();
    assert_eq!(restored.raw(), e.raw());
    assert_eq!(restored.container(), e.container());
    let mut raw = e.raw().to_vec();
    raw.extend(field(100, b"future optional field"));
    let extended = decode_raw(&raw).unwrap();
    assert_eq!(extended.raw(), raw);
    assert_ne!(extended.raw(), extended.data().encode_to_vec());
    let mut reversed = d;
    reversed.batch.as_mut().unwrap().observations.reverse();
    let reversed = evidence::encode(reversed).unwrap();
    assert_ne!(reversed.digest(), e.digest());
    assert!(evidence::decode(reversed.container(), e.digest()).is_err());
}
#[test]
fn batch_requires_every_successful_page_and_matching_completion() {
    for case in 0..8 {
        let mut d = data(3);
        let page = &mut d.batch.as_mut().unwrap().observations[1];
        match case {
            0 => page.completion.clear(),
            1 => page.exit_code = None,
            2 => page.exit_code = Some(1),
            3 => {
                page.fault = StableFault::Trap as i32;
                page.exit_code = None;
            }
            4 => page.budget = None,
            5 => page.completion = observation(99).completion,
            6 => {
                page.completion = Invocation::decode(&page.invocation)
                    .unwrap()
                    .failure_completion(&PluginFailure {
                        code: FailureCode::UnsupportedInput,
                        message: "business failure".into(),
                    })
                    .unwrap()
            }
            _ => page.backend.clear(),
        };
        assert!(evidence::encode(d).is_err(), "case {case}");
    }
    assert!(evidence::encode(data(0)).is_err());
}
#[test]
fn batch_count_and_intent_boundaries_are_enforced_before_decode() {
    assert!(evidence::encode(data(1024)).is_ok());
    assert!(decode_raw(&data(1025).encode_to_vec()).is_err());
    let mut d = data(1);
    d.batch.as_mut().unwrap().intent = vec![7; evidence::MAX_INTENT_BYTES];
    assert!(evidence::encode(d.clone()).is_ok());
    d.batch.as_mut().unwrap().intent.push(7);
    assert!(decode_raw(&d.encode_to_vec()).is_err());
    for kind in [
        "".to_string(),
        "spaces forbidden".into(),
        "非ASCII".into(),
        "x".repeat(129),
    ] {
        let mut d = data(1);
        d.batch.as_mut().unwrap().intent_type = kind;
        assert!(evidence::encode(d).is_err());
    }
}
#[test]
fn cumulative_fuel_uses_actual_consumption_and_each_remaining_budget() {
    let mut d = data(2);
    let b = d.batch.as_mut().unwrap();
    b.total_fuel = 110000;
    assert!(evidence::encode(d.clone()).is_ok());
    d.batch.as_mut().unwrap().total_fuel = 109999;
    assert!(evidence::encode(d).is_err());
    for total in [0, 10000000001, u64::MAX] {
        let mut d = data(1);
        d.batch.as_mut().unwrap().total_fuel = total;
        assert!(evidence::encode(d).is_err());
    }
    let mut d = data(1);
    d.batch.as_mut().unwrap().observations[0].fuel_remaining = 100001;
    assert!(evidence::encode(d).is_err());
}
#[test]
fn legacy_and_batch_observation_fields_cannot_mix() {
    for case in 0..9 {
        let mut d = data(1);
        match case {
            0 => d.schema_version = 1,
            1 => d.invocation = observation(0).invocation,
            2 => d.budget = observation(0).budget,
            3 => d.backend = evidence::BACKEND.into(),
            4 => d.completion = observation(0).completion,
            5 => d.fault = 1,
            6 => d.exit_code = Some(0),
            7 => d.observed_host_calls = 1,
            _ => d.fuel_remaining = 1,
        };
        assert!(evidence::encode(d).is_err(), "mixed {case}");
    }
    let mut d = data(1);
    d.batch = None;
    assert!(evidence::encode(d).is_err());
}
#[test]
fn duplicate_known_fields_at_all_three_nested_levels_fail_closed() {
    let d = data(1);
    let b = d.batch.as_ref().unwrap();
    let o = &b.observations[0];
    let mut raw = d.encode_to_vec();
    raw.extend(field(11, &b.encode_to_vec()));
    assert!(decode_raw(&raw).is_err());
    for duplicate in [field(1, b"other"), field(2, b"other"), vec![24, 1]] {
        let mut encoded = b.encode_to_vec();
        encoded.extend(duplicate);
        assert!(decode_raw(&with_batch_bytes(&encoded)).is_err());
    }
    for duplicate in [
        field(1, &o.invocation),
        field(2, &o.budget.as_ref().unwrap().encode_to_vec()),
        field(3, b"other"),
        field(4, &o.completion),
        vec![40, 0],
        vec![48, 0],
        vec![56, 0],
        vec![64, 0],
    ] {
        let mut encoded = o.encode_to_vec();
        if duplicate == [40, 0] || duplicate == [56, 0] {
            encoded.extend(&duplicate);
        }
        encoded.extend(duplicate);
        let mut b = b.clone();
        b.observations.clear();
        let mut batch = b.encode_to_vec();
        batch.extend(field(4, &encoded));
        assert!(decode_raw(&with_batch_bytes(&batch)).is_err());
    }
    let mut budget = o.budget.as_ref().unwrap().encode_to_vec();
    budget.extend([8, 1]);
    let mut page = o.clone();
    page.budget = None;
    let mut encoded = page.encode_to_vec();
    encoded.extend(field(2, &budget));
    let mut b = b.clone();
    b.observations.clear();
    let mut batch = b.encode_to_vec();
    batch.extend(field(4, &encoded));
    assert!(decode_raw(&with_batch_bytes(&batch)).is_err());
}
#[test]
fn malformed_lengths_wire_types_groups_and_aggregate_size_are_bounded() {
    for suffix in [
        vec![0x5a, 0xff, 0xff, 0xff, 0xff, 0x7f],
        vec![0x58, 1],
        vec![0x5b, 0x5c],
    ] {
        let mut d = data(1);
        d.batch = None;
        let mut raw = d.encode_to_vec();
        raw.extend(suffix);
        assert!(decode_raw(&raw).is_err());
    }
    let mut batch = data(1).batch.unwrap().encode_to_vec();
    batch.extend([0x22, 0xff, 0xff, 0xff, 0xff, 0x7f]);
    assert!(decode_raw(&with_batch_bytes(&batch)).is_err());
    let mut raw = data(1).encode_to_vec();
    raw.extend(field(100, &vec![0; evidence::MAX_RAW_BYTES]));
    assert!(decode_raw(&raw).is_err());
}
#[test]
fn original_v1_container_is_retained_without_batch_reencoding() {
    let d = data(1);
    let o = observation(0);
    let single = TaskEvidence {
        schema_version: 1,
        package_archive: d.package_archive,
        invocation: o.invocation,
        budget: o.budget,
        backend: o.backend,
        completion: o.completion,
        fault: o.fault,
        exit_code: o.exit_code,
        observed_host_calls: o.observed_host_calls,
        fuel_remaining: o.fuel_remaining,
        batch: None,
    };
    let raw = single.encode_to_vec();
    let (container, digest) = packed(&raw);
    let decoded = evidence::decode(&container, digest).unwrap();
    assert_eq!(decoded.container(), container);
    assert_eq!(decoded.raw(), raw);
    assert_eq!(decoded.digest(), digest);
    assert_eq!(decoded.data().batch, None);
}
