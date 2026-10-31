use super::*;

fn active() -> OwnerRecord {
    OwnerRecord {
        model_version: 2,
        identity: Some(Identity {
            profile: vec![1; 32],
            generation: 7,
            incarnation: vec![2; 32],
            host_sha256: vec![3; 32],
            package_sha256: vec![4; 32],
            supervisor_sha256: vec![5; 32],
            supervisor_pid: 10,
            supervisor_creation_filetime: 123,
            child_pid: 11,
            child_creation_filetime: 124,
            recovery_history_head: vec![],
        }),
        phase: OwnerPhase::Active as i32,
        resources: Some(ResourceFacts::default()),
        business_outcome: BusinessOutcome::Unknown as i32,
        protocol_ack: ProtocolAck::NotApplicable as i32,
        ..Default::default()
    }
}

fn closed(normal: bool) -> OwnerRecord {
    let mut r = active();
    r.phase = if normal {
        OwnerPhase::Released
    } else {
        OwnerPhase::Closing
    } as i32;
    r.resources = Some(ResourceFacts {
        resource_reclaimed: true,
        gate_closed: true,
        tree_empty: true,
        child_exit: true,
        stdout_eof: true,
        stderr_eof: true,
        control_reaped: true,
        business_gate_revoked: !normal,
        normal_shutdown: if normal {
            ObservedTruth::Yes
        } else {
            ObservedTruth::No
        } as i32,
        child_exit_code: Some(if normal { 0 } else { 2 }),
    });
    r
}

#[test]
fn binary_roundtrip_preserves_absent_exit_and_never_accepts_legacy_json() {
    for value in [active(), closed(true), closed(false)] {
        let bytes = encode_owner(&value).unwrap();
        assert!(bytes.starts_with(OWNER_MAGIC));
        assert_eq!(decode_owner(&bytes).unwrap(), value);
    }
    assert_eq!(
        decode_owner(&encode_owner(&active()).unwrap())
            .unwrap()
            .resources
            .unwrap()
            .child_exit_code,
        None
    );
    assert!(decode_owner(br#"{"version":2,"phase":"Released"}"#).is_err());
    let mut legacy = active();
    legacy.model_version = 1;
    assert!(encode_owner(&legacy).is_err());
}

#[test]
fn malformed_lengths_digest_truncation_and_trailing_bytes_fail_closed() {
    let original = encode_owner(&active()).unwrap();
    for size in 0..original.len() {
        assert!(decode_owner(&original[..size]).is_err());
    }
    for offset in [0, 8, 10, 14, 18, 49, original.len() - 1] {
        let mut bytes = original.clone();
        bytes[offset] ^= 128;
        assert!(decode_owner(&bytes).is_err(), "offset {offset}");
    }
    let mut trailing = original.clone();
    trailing.push(0);
    assert!(decode_owner(&trailing).is_err());
    let mut huge = original;
    huge[10..14].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(decode_owner(&huge).is_err());
    assert!(decode_owner(&vec![0; MAX_CONTAINER + 1]).is_err());
}

#[test]
fn unknown_repeated_and_future_fields_do_not_silently_normalize() {
    for suffix in [vec![0xf8, 0x07, 0x01], vec![0x08, 0x02]] {
        let mut raw = active().encode_to_vec();
        raw.extend(suffix);
        assert!(decode_owner(&pack(OWNER_MAGIC, &raw).unwrap()).is_err());
    }
    for field in 0..5 {
        let mut r = active();
        match field {
            0 => r.model_version = 3,
            1 => r.phase = 99,
            2 => r.business_outcome = 0,
            3 => r.protocol_ack = 99,
            _ => r.resources.as_mut().unwrap().normal_shutdown = 99,
        }
        assert!(decode_owner(&pack(OWNER_MAGIC, &r.encode_to_vec()).unwrap()).is_err());
    }
}

#[test]
fn every_normal_resource_fact_and_observed_zero_exit_is_required() {
    let clearers: [fn(&mut ResourceFacts); 7] = [
        |f| f.resource_reclaimed = false,
        |f| f.gate_closed = false,
        |f| f.tree_empty = false,
        |f| f.child_exit = false,
        |f| f.stdout_eof = false,
        |f| f.stderr_eof = false,
        |f| f.control_reaped = false,
    ];
    for clear in clearers {
        let mut r = closed(true);
        clear(r.resources.as_mut().unwrap());
        assert!(encode_owner(&r).is_err());
    }
    for exit in [None, Some(2), Some(-1)] {
        let mut r = closed(true);
        r.resources.as_mut().unwrap().child_exit_code = exit;
        assert!(encode_owner(&r).is_err());
    }
    for shutdown in [0, ObservedTruth::No as i32] {
        let mut r = closed(true);
        r.resources.as_mut().unwrap().normal_shutdown = shutdown;
        assert!(encode_owner(&r).is_err());
    }
}

#[test]
fn recovery_retains_abnormal_unknown_and_requires_original_bindings() {
    let mut r = closed(false);
    assert!(complete_abnormal_proof(&r));
    assert!(!normal_release(&r));
    r.phase = OwnerPhase::Recovered as i32;
    assert!(encode_owner(&r).is_err());
    r.recovery = Some(proto::RecoveryBinding {
        version: 1,
        original_digest: vec![6; 32],
        receipt_digest: vec![7; 32],
        history_head: vec![8; 32],
    });
    assert_eq!(decode_owner(&encode_owner(&r).unwrap()).unwrap(), r);
    for key in 0..4 {
        let mut bad = r.clone();
        match key {
            0 => bad.resources.as_mut().unwrap().business_gate_revoked = false,
            1 => bad.resources.as_mut().unwrap().normal_shutdown = ObservedTruth::Yes as i32,
            2 => bad.resources.as_mut().unwrap().child_exit_code = None,
            _ => bad
                .recovery
                .as_mut()
                .unwrap()
                .original_digest
                .pop()
                .map(|_| ())
                .unwrap(),
        }
        assert!(encode_owner(&bad).is_err());
    }
    let mut direct = active();
    direct.phase = OwnerPhase::Recovered as i32;
    assert!(validate_transition(&active(), &direct).is_err());
}

#[test]
fn transition_rejects_changed_generation_artifact_and_process_birth() {
    let prior = active();
    let mut next = prior.clone();
    next.phase = OwnerPhase::Closing as i32;
    validate_transition(&prior, &next).unwrap();
    let changes: [fn(&mut Identity); 11] = [
        |i| i.profile[0] ^= 1,
        |i| i.generation += 1,
        |i| i.incarnation[0] ^= 1,
        |i| i.host_sha256[0] ^= 1,
        |i| i.package_sha256[0] ^= 1,
        |i| i.supervisor_sha256[0] ^= 1,
        |i| i.supervisor_pid += 1,
        |i| i.supervisor_creation_filetime += 1,
        |i| i.child_pid += 1,
        |i| i.child_creation_filetime += 1,
        |i| i.recovery_history_head = vec![9; 32],
    ];
    for change in changes {
        let mut bad = next.clone();
        change(bad.identity.as_mut().unwrap());
        assert!(validate_transition(&prior, &bad).is_err());
    }
    assert!(validate_transition(&prior, &closed(true)).is_err());
    let mut pending = active();
    pending.phase = OwnerPhase::Pending as i32;
    pending.identity.as_mut().unwrap().child_pid = 0;
    pending.identity.as_mut().unwrap().child_creation_filetime = 0;
    validate_transition(&pending, &active()).unwrap();
    let mut bad = active();
    bad.identity.as_mut().unwrap().child_creation_filetime = 0;
    assert!(validate_transition(&pending, &bad).is_err());
}

#[test]
fn absent_identity_and_incomplete_joins_never_become_proof() {
    let mut r = active();
    r.identity = None;
    assert!(encode_owner(&r).is_err());
    let mut r = active();
    r.resources = None;
    assert!(encode_owner(&r).is_err());
    let full = ReaderJoins {
        version: 1,
        stdout_reader_joined: true,
        stderr_reader_joined: true,
        controller_reader_joined: true,
    };
    assert!(joined(&full));
    for key in 0..4 {
        let mut j = full.clone();
        match key {
            0 => j.version = 0,
            1 => j.stdout_reader_joined = false,
            2 => j.stderr_reader_joined = false,
            _ => j.controller_reader_joined = false,
        }
        assert!(!joined(&j));
    }
}

#[test]
fn receipt_is_separate_from_owner_and_cannot_omit_unknown_acknowledgment() {
    let r = proto::RecoveryReceipt {
        version: 1,
        acknowledged_unknown: true,
        original_digest: vec![1; 32],
        profile: vec![2; 32],
        generation: 1,
        incarnation: vec![3; 32],
        manager_pid: 9,
        manager_creation_filetime: 125,
        manager_sha256: vec![4; 32],
        original_supervisor_ended: true,
    };
    let bytes = encode_receipt(&r).unwrap();
    assert_eq!(decode_receipt(&bytes).unwrap(), r);
    assert!(decode_owner(&bytes).is_err());
    for key in 0..4 {
        let mut bad = r.clone();
        match key {
            0 => bad.acknowledged_unknown = false,
            1 => bad.original_supervisor_ended = false,
            2 => bad.manager_creation_filetime = 0,
            _ => bad.manager_sha256.clear(),
        }
        assert!(encode_receipt(&bad).is_err());
    }
}
