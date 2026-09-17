//! Candidate codec/state-machine tests only: no Store persistence, external IO,
//! crash recovery, permission grant or response-evidence authenticity is claimed.
use morrow_core::{
    Error,
    io_intent::{self, Command, ObservationSource, Phase, Record, Recovery, proto},
    plugin_package::io,
};
use prost::Message;
use sha2::{Digest, Sha256};

fn command() -> Command {
    Command {
        operation_id: "io-operation-1".into(),
        subject: "plugin.io-test".into(),
        package_sha256: [1; 32],
        capability: io::IoCapability::HttpRequest,
        protocol_sha256: io::schema_digest(),
        request_sha256: [2; 32],
        approval_sha256: [3; 32],
        target_sha256: [4; 32],
        request_bytes: 27,
        response_limit: 4096,
    }
}
fn prepared() -> Record {
    Record::prepared(command()).unwrap()
}
// Recompute the legitimate outer hash so malformed protobuf/semantics reach the
// corresponding decoder checks instead of merely failing checksum validation.
fn pack(raw: &[u8]) -> Vec<u8> {
    let compressed = lz4_flex::block::compress(raw);
    let mut bytes = Vec::new();
    bytes.extend_from_slice(io_intent::MAGIC);
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&(raw.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&Sha256::digest(raw));
    bytes.extend_from_slice(&compressed);
    bytes
}
fn decode(value: &proto::Record) -> morrow_core::Result<Record> {
    Record::decode(&pack(&value.encode_to_vec()))
}
fn reject(value: &proto::Record) {
    assert!(
        decode(value).is_err(),
        "accepted malformed record: {value:?}"
    );
}

#[test]
fn prepared_unknown_observed_preserve_exact_command_and_predecessors() {
    for source in [
        ObservationSource::OriginalResponse,
        ObservationSource::Reconciliation,
    ] {
        let first = prepared();
        assert_eq!(first.phase(), Phase::Prepared);
        assert_eq!(first.recovery(), Recovery::AwaitFreshAuthorization);
        let pending = first.propose_dispatch_boundary().unwrap();
        assert_eq!(pending.revision(), 2);
        assert_eq!(pending.recovery(), Recovery::ReconcileOnly);
        assert_eq!(pending.data().previous_sha256, first.digest());
        let observed = pending.propose_observation([5; 32], source).unwrap();
        assert_eq!(observed.phase(), Phase::Observed);
        assert_eq!(observed.revision(), 3);
        assert_eq!(observed.recovery(), Recovery::AlreadyObserved);
        assert_eq!(observed.data().observation_source, source as i32);
        assert_eq!(observed.data().previous_sha256, pending.digest());
        assert_eq!(observed.command(), &command());
        first.verify_successor(&pending).unwrap();
        pending.verify_successor(&observed).unwrap();
    }
}

#[test]
fn unknown_cannot_resend_or_cancel_and_terminals_cannot_transition() {
    let first = prepared();
    assert!(
        first
            .propose_observation([5; 32], ObservationSource::OriginalResponse)
            .is_err()
    );
    let pending = first.propose_dispatch_boundary().unwrap();
    assert!(pending.propose_dispatch_boundary().is_err());
    assert!(pending.propose_cancel_before_dispatch().is_err());
    for terminal in [
        first.propose_cancel_before_dispatch().unwrap(),
        pending
            .propose_observation([5; 32], ObservationSource::Reconciliation)
            .unwrap(),
    ] {
        assert!(terminal.propose_dispatch_boundary().is_err());
        assert!(terminal.propose_cancel_before_dispatch().is_err());
        assert!(
            terminal
                .propose_observation([6; 32], ObservationSource::OriginalResponse)
                .is_err()
        );
        assert!(terminal.verify_successor(&first).is_err());
        assert!(terminal.verify_successor(&pending).is_err());
    }
    assert_eq!(
        first.propose_cancel_before_dispatch().unwrap().recovery(),
        Recovery::CancelledBeforeDispatch
    );
}

#[test]
fn every_command_field_is_bound_to_operation_identity() {
    let original = prepared();
    original.matches_command(&command()).unwrap();
    for field in 0..11 {
        let mut changed = command();
        match field {
            0 => changed.operation_id.push('2'),
            1 => changed.subject.push('2'),
            2 => changed.package_sha256[0] ^= 1,
            3 => changed.capability = io::IoCapability::FileRead,
            4 => changed.protocol_sha256[0] ^= 1,
            5 => changed.request_sha256[0] ^= 1,
            6 => changed.approval_sha256[0] ^= 1,
            7 => changed.target_sha256[0] ^= 1,
            8 => changed.request_bytes += 1,
            9 => changed.response_limit += 1,
            10 => changed.request_bytes = 0,
            _ => unreachable!(),
        }
        let error = original.matches_command(&changed).unwrap_err();
        if field == 4 {
            assert!(matches!(error, Error::UnsupportedVersion));
        } else {
            assert!(
                matches!(error, Error::OperationConflict),
                "field {field}: {error:?}"
            );
        }
    }
}

#[test]
fn successor_requires_original_predecessor_and_same_command() {
    let first = prepared();
    let pending = first.propose_dispatch_boundary().unwrap();
    let mut value = pending.data().clone();
    value.previous_sha256 = vec![9; 32];
    // A standalone well-formed record does not authenticate its alleged history.
    let forged = decode(&value).unwrap();
    assert!(matches!(
        first.verify_successor(&forged),
        Err(Error::RevisionConflict)
    ));
    let mut other = command();
    other.subject = "other-plugin".into();
    let other = Record::prepared(other)
        .unwrap()
        .propose_dispatch_boundary()
        .unwrap();
    assert!(matches!(
        first.verify_successor(&other),
        Err(Error::OperationConflict)
    ));
    assert!(pending.verify_successor(&pending).is_err());
    let observed = pending
        .propose_observation([5; 32], ObservationSource::OriginalResponse)
        .unwrap();
    assert!(first.verify_successor(&observed).is_err());
}

#[test]
fn redecoding_original_unknown_bytes_stays_reconcile_only() {
    let unknown = prepared().propose_dispatch_boundary().unwrap();
    let saved = unknown.container().to_vec();
    let expected_digest = unknown.digest();
    drop(unknown);
    let restored = Record::decode(&saved).unwrap();
    assert_eq!(restored.recovery(), Recovery::ReconcileOnly);
    assert_eq!(restored.container(), saved);
    assert_eq!(restored.digest(), expected_digest);
    assert_eq!(restored.raw(), restored.data().encode_to_vec());
    assert!(restored.propose_dispatch_boundary().is_err());
    assert!(restored.propose_cancel_before_dispatch().is_err());
    restored
        .propose_observation([8; 32], ObservationSource::Reconciliation)
        .unwrap();
}

#[test]
fn unknown_duplicate_wrong_wire_and_noncanonical_protobuf_are_rejected() {
    let initial = prepared();
    for suffix in [
        vec![0x88, 0x01, 1],
        vec![0x08, 1],
        vec![0x0a, 0],
        vec![0],
        vec![0x80],
        vec![0x12, 0],
    ] {
        let mut raw = initial.raw().to_vec();
        raw.extend(suffix);
        assert!(Record::decode(&pack(&raw)).is_err());
    }
    // Canonical schema_version=1 rewritten as an overlong varint.
    let mut raw = initial.raw().to_vec();
    assert_eq!(&raw[..2], &[8, 1]);
    raw.splice(1..2, [0x81, 0]);
    assert!(Record::decode(&pack(&raw)).is_err());
    // Scalar u32 overflow must not truncate to the supported schema version.
    let mut raw = initial.raw().to_vec();
    raw.splice(1..2, [0x81, 0x80, 0x80, 0x80, 0x10]);
    assert!(Record::decode(&pack(&raw)).is_err());
    // Unique wrong-wire field, without an earlier duplicate masking the check.
    assert!(Record::decode(&pack(&[0x0a, 1, 1])).is_err());
    assert!(Record::decode(&pack(&[0x10, 1])).is_err());
}

#[test]
fn ownership_lengths_and_compressed_container_limits_are_bounded() {
    let base = prepared().data().clone();
    for field in [2, 3, 4, 6, 7, 8, 9, 13, 15] {
        let mut raw = Vec::new();
        raw.push((field << 3) | 2);
        let length = if field == 2 || field == 3 {
            257u64
        } else {
            33u64
        };
        prost::encoding::encode_varint(length, &mut raw);
        raw.extend(vec![b'x'; length as usize]);
        assert!(matches!(Record::decode(&pack(&raw)), Err(Error::Limit)));
    }
    for mut value in [base.clone(), base] {
        value.request_bytes = io::MAX_JOB_BYTES + 1;
        reject(&value);
        value.request_bytes = 0;
        value.response_limit = io::MAX_JOB_BYTES + 1;
        reject(&value);
    }
    assert!(matches!(
        Record::decode(&pack(&vec![0; io_intent::MAX_RAW_BYTES + 1])),
        Err(Error::Limit)
    ));
    assert!(matches!(
        Record::decode(&vec![0; io_intent::MAX_CONTAINER_BYTES + 1]),
        Err(Error::Limit)
    ));
    let mut oversized_header = prepared().container().to_vec();
    oversized_header[10..14].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(matches!(
        Record::decode(&oversized_header),
        Err(Error::Limit)
    ));
}

#[test]
fn all_versions_capabilities_identities_and_digests_are_validated() {
    let base = prepared().data().clone();
    for version in [0, 2, u32::MAX] {
        let mut v = base.clone();
        v.schema_version = version;
        reject(&v);
    }
    for capability in [0, 11, u32::MAX] {
        let mut v = base.clone();
        v.capability = capability;
        reject(&v);
    }
    for id in ["".to_string(), "x".repeat(257), "bad\0id".into()] {
        let mut v = base.clone();
        v.operation_id = id.clone();
        reject(&v);
        let mut v = base.clone();
        v.subject = id;
        reject(&v);
    }
    for field in 0..6 {
        for bytes in [vec![], vec![1; 31], vec![1; 33], vec![0; 32]] {
            let mut v = base.clone();
            *match field {
                0 => &mut v.package_sha256,
                1 => &mut v.protocol_sha256,
                2 => &mut v.request_sha256,
                3 => &mut v.approval_sha256,
                4 => &mut v.target_sha256,
                5 => {
                    v.phase = Phase::OutcomeUnknown as i32;
                    v.revision = 2;
                    &mut v.previous_sha256
                }
                _ => unreachable!(),
            } = bytes;
            reject(&v);
        }
    }
    let mut v = base;
    v.protocol_sha256 = vec![7; 32];
    reject(&v);
}

#[test]
fn phase_source_revision_and_observation_semantics_reject_invalid_records() {
    let first = prepared();
    let unknown = first.propose_dispatch_boundary().unwrap();
    let observed = unknown
        .propose_observation([5; 32], ObservationSource::OriginalResponse)
        .unwrap();
    let cancelled = first.propose_cancel_before_dispatch().unwrap();
    for original in [&first, &unknown, &observed, &cancelled] {
        for phase in [0, -1, 5, i32::MAX] {
            let mut v = original.data().clone();
            v.phase = phase;
            reject(&v);
        }
        for source in [-1, 3, i32::MAX] {
            let mut v = original.data().clone();
            v.observation_source = source;
            reject(&v);
        }
        for revision in [0, 1, 2, 3, 4, u64::MAX] {
            if revision != original.revision() {
                let mut v = original.data().clone();
                v.revision = revision;
                reject(&v);
            }
        }
    }
    let mut v = first.data().clone();
    v.previous_sha256 = vec![9; 32];
    reject(&v);
    for original in [&first, &unknown, &cancelled] {
        let mut v = original.data().clone();
        v.observation_sha256 = vec![5; 32];
        reject(&v);
        let mut v = original.data().clone();
        v.observation_source = ObservationSource::OriginalResponse as i32;
        reject(&v);
    }
    for bytes in [vec![], vec![0; 32], vec![1; 31], vec![1; 33]] {
        let mut v = observed.data().clone();
        v.observation_sha256 = bytes;
        reject(&v);
    }
    let mut v = observed.data().clone();
    v.observation_source = ObservationSource::NoObservation as i32;
    reject(&v);
    assert!(
        unknown
            .propose_observation([0; 32], ObservationSource::OriginalResponse)
            .is_err()
    );
    assert!(
        unknown
            .propose_observation([5; 32], ObservationSource::NoObservation)
            .is_err()
    );
}

#[test]
fn corruption_truncation_trailing_bytes_and_invalid_lz4_never_decode() {
    let original = prepared().container().to_vec();
    for length in 0..original.len() {
        assert!(
            Record::decode(&original[..length]).is_err(),
            "truncation {length}"
        );
    }
    for position in [0, 8, 18, 49] {
        let mut bytes = original.clone();
        bytes[position] ^= 0x40;
        assert!(Record::decode(&bytes).is_err());
    }
    let mut bytes = original.clone();
    bytes.push(0);
    assert!(Record::decode(&bytes).is_err());
    let mut bytes = original.clone();
    bytes.extend_from_slice(&original);
    assert!(Record::decode(&bytes).is_err());
    let mut bytes = original;
    bytes.truncate(50);
    bytes[14..18].copy_from_slice(&1u32.to_le_bytes());
    bytes.push(0xff);
    assert!(Record::decode(&bytes).is_err());
}
