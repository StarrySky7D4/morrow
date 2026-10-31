use super::*;

#[test]
fn task_envelope_selection_is_canonical_and_distinct_from_packet_claims() {
    let packet = sample(Metadata::Reserved);
    let bytes = packet.encode().unwrap();
    let correct = task_id([1; 32], [9; 32]).unwrap();
    assert_eq!(correct.len(), TASK_ID_BYTES);
    assert_eq!(TASK_ID_BYTES, 140);
    assert_eq!(Packet::decode_task(&bytes, &correct).unwrap(), packet);
    assert!(task_id([0; 32], [9; 32]).is_err());
    assert!(task_id([1; 32], [0; 32]).is_err());
    for wrong in [
        task_id([2; 32], [9; 32]).unwrap(),
        task_id([1; 32], [8; 32]).unwrap(),
    ] {
        assert_eq!(Packet::decode_task(&bytes, &wrong), Err(Error::Correlation));
    }
    for wrong in [
        format!("{correct}."),
        correct[..correct.len() - 1].to_owned(),
        correct.replacen("history-v1.", "history-v2.", 1),
        correct.replacen('.', "/", 1),
        correct.replacen("01", "00", 32),
    ] {
        assert_eq!(Packet::decode_task(&bytes, &wrong), Err(Error::Correlation));
    }
    let canonical = task_id([0xab; 32], [0xcd; 32]).unwrap();
    assert_eq!(
        task::selection(&canonical).unwrap(),
        ([0xab; 32], [0xcd; 32])
    );
    assert!(task::selection(&canonical.to_ascii_uppercase()).is_err());
    let mut invalid = canonical.into_bytes();
    invalid[11] = 0xff;
    assert!(std::str::from_utf8(&invalid).is_err());
}

#[test]
fn task_query_correlation_never_changes_original_unknown_or_current_stage() {
    let packet = sample(Metadata::Acknowledged {
        result: original(),
        observed_at: 0,
        at: 1,
    });
    for byte in [1, 2, 127, 255] {
        let mut record = packet.record().clone();
        record.submission = [byte; 32];
        let expected = Packet::new([byte.wrapping_add(1).max(1); 32], record).unwrap();
        let selected = task_id(expected.record().submission, expected.nonce()).unwrap();
        let actual = Packet::decode_task(&expected.encode().unwrap(), &selected).unwrap();
        assert_eq!(actual, expected);
        assert!(actual.record().original_unknown());
        assert_eq!(actual.record().current_stage, Stage::Staged);
        assert_eq!(
            actual
                .record()
                .original()
                .unwrap()
                .execution
                .unwrap()
                .outcome,
            Ok(i32::MIN)
        );
    }
}

#[test]
fn task_c_adapter_preserves_outputs_and_opaque_owner_on_refusal() {
    let wire = sample(Metadata::Reserved).encode().unwrap();
    let mut text = [0xa5; TASK_ID_BYTES + 8];
    let original_text = text;
    let mut written = 0x1234u32;
    let mut owner = std::ptr::null_mut();
    let submission = [1u8; 32];
    let nonce = [9u8; 32];
    unsafe {
        assert_eq!(
            ffi::mbh_task_id_write(
                submission.as_ptr(),
                nonce.as_ptr(),
                text.as_mut_ptr(),
                (TASK_ID_BYTES - 1) as u32,
                &mut written
            ),
            5
        );
        assert_eq!(text, original_text);
        assert_eq!(written, 0x1234);
        assert_eq!(
            ffi::mbh_task_id_write(
                [0u8; 32].as_ptr(),
                nonce.as_ptr(),
                text.as_mut_ptr(),
                text.len() as u32,
                &mut written
            ),
            1
        );
        assert_eq!(text, original_text);
        assert_eq!(written, 0x1234);
        assert_eq!(
            ffi::mbh_task_id_write(
                submission.as_ptr(),
                nonce.as_ptr(),
                text.as_mut_ptr(),
                text.len() as u32,
                &mut written
            ),
            0
        );
        assert_eq!(written as usize, TASK_ID_BYTES);
        assert_eq!(&text[TASK_ID_BYTES..], &[0xa5; 8]);
        assert_eq!(
            &text[..TASK_ID_BYTES],
            task_id(submission, nonce).unwrap().as_bytes()
        );
        assert_eq!(
            ffi::mbh_packet_decode_task(
                wire.as_ptr(),
                wire.len() as u32,
                text.as_ptr(),
                written,
                &mut owner
            ),
            0
        );
        let original_owner = owner;
        text[11] = b'f';
        assert_eq!(
            ffi::mbh_packet_decode_task(
                wire.as_ptr(),
                wire.len() as u32,
                text.as_ptr(),
                written,
                &mut owner
            ),
            4
        );
        assert_eq!(owner, original_owner);
        text[11] = 0xff;
        assert_eq!(
            ffi::mbh_packet_decode_task(
                wire.as_ptr(),
                wire.len() as u32,
                text.as_ptr(),
                written,
                &mut owner
            ),
            4
        );
        assert_eq!(owner, original_owner);
        let mut aligned_output = [0x11223344u32; 40];
        let before = aligned_output;
        let out = aligned_output.as_mut_ptr().cast::<u8>();
        assert_eq!(
            ffi::mbh_task_id_write(
                submission.as_ptr(),
                nonce.as_ptr(),
                out,
                160,
                aligned_output.as_mut_ptr()
            ),
            5
        );
        assert_eq!(aligned_output, before);
        ffi::mbh_packet_free(owner);
    }
}

fn sample(metadata: Metadata) -> Packet {
    Packet::new(
        [9; 32],
        Record {
            submission: [1; 32],
            package_id: "org.example.history".into(),
            package_digest: [2; 32],
            registry_revision: u64::MAX,
            operation_id: "upload-原记录".into(),
            byte_length: MAX_OBJECT_BYTES,
            whole_sha256: [3; 32],
            handler: "inspect-history".into(),
            input_sha256: [4; 32],
            request_binding_sha256: [5; 32],
            reserved_at: -10,
            metadata,
            current_stage: Stage::Staged,
        },
    )
    .unwrap()
}
fn original() -> OriginalResult {
    OriginalResult {
        outcome: Outcome::Error(7),
        unknown: true,
        calls: u32::MAX,
        wire_bytes: u64::MAX,
        native_work_bytes: u64::MAX,
        execution: Some(Execution {
            outcome: Ok(i32::MIN),
            host_calls: u32::MAX,
        }),
    }
}
fn decode(p: &Packet) -> Packet {
    Packet::decode_for(&p.encode().unwrap(), [1; 32], [9; 32]).unwrap()
}

#[test]
fn reservation_is_uncertain_even_when_current_bytes_exist() {
    let p = decode(&sample(Metadata::Reserved));
    assert_eq!(p.record().revision(), 1);
    assert!(p.record().original_unknown());
    assert_eq!(p.record().current_stage, Stage::Staged);
    assert!(p.record().original().is_none());
}
#[test]
fn acknowledgement_never_rewrites_original_unknown_or_signed_exit() {
    let p = sample(Metadata::Acknowledged {
        result: original(),
        observed_at: -2,
        at: 0,
    });
    assert_eq!(decode(&p), p);
    assert!(decode(&p).record().original_unknown());
    assert_eq!(
        decode(&p)
            .record()
            .original()
            .unwrap()
            .execution
            .unwrap()
            .outcome,
        Ok(i32::MIN)
    );
}
#[test]
fn metadata_variants_outcomes_and_execution_errors_preserve_original_facts() {
    for outcome in [
        Outcome::Absent,
        Outcome::Staged,
        Outcome::Released,
        Outcome::Error(1),
        Outcome::Error(10),
    ] {
        for execution in [
            None,
            Some(Execution {
                outcome: Ok(0),
                host_calls: 0,
            }),
            Some(Execution {
                outcome: Err(9),
                host_calls: 3,
            }),
        ] {
            let result = OriginalResult {
                outcome,
                unknown: false,
                execution,
                ..original()
            };
            let p = sample(Metadata::Observed { result, at: 0 });
            assert_eq!(decode(&p), p);
        }
    }
}
#[test]
fn wrong_selection_nonce_and_transport_shape_are_rejected() {
    let p = sample(Metadata::Reserved);
    let wire = p.encode().unwrap();
    assert_eq!(
        Packet::decode_for(&wire, [2; 32], [9; 32]),
        Err(Error::Correlation)
    );
    assert_eq!(
        Packet::decode_for(&wire, [1; 32], [8; 32]),
        Err(Error::Correlation)
    );
    for len in 0..wire.len() {
        assert!(Packet::decode_for(&wire[..len], [1; 32], [9; 32]).is_err());
    }
    let mut extra = wire.clone();
    extra.extend_from_slice(&[0; 8]);
    assert!(Packet::decode_for(&extra, [1; 32], [9; 32]).is_err());
    assert!(Packet::decode_for(b"{\"revision\":3}", [1; 32], [9; 32]).is_err());
    assert!(Packet::decode_for(&vec![0; MAX_PACKET_BYTES + 1], [1; 32], [9; 32]).is_err());
    let mut unaligned = vec![0];
    unaligned.extend_from_slice(&wire);
    assert_eq!(
        Packet::decode_for(&unaligned[1..], [1; 32], [9; 32]).unwrap(),
        p
    );
}
fn altered(p: &Packet, change: impl FnOnce(wire::packet::Builder<'_>)) -> Vec<u8> {
    let bytes = p.encode().unwrap();
    let mut slice = bytes.as_slice();
    let reader = serialize::read_message_from_flat_slice(&mut slice, ReaderOptions::new()).unwrap();
    let mut message = Builder::new_default();
    message
        .set_root(reader.get_root::<wire::packet::Reader>().unwrap())
        .unwrap();
    change(message.get_root::<wire::packet::Builder>().unwrap());
    serialize::write_message_to_words(&message)
}
#[test]
fn incompatible_contract_and_nonempty_inactive_fields_are_rejected() {
    let p = sample(Metadata::Reserved);
    let reject = |bytes: Vec<u8>| assert!(Packet::decode_for(&bytes, [1; 32], [9; 32]).is_err());
    reject(altered(&p, |mut r| r.set_version(2)));
    reject(altered(&p, |mut r| r.set_schema_sha256(&[7; 32])));
    reject(altered(&p, |mut r| r.set_original_unknown(false)));
    reject(altered(&p, |mut r| r.set_calls(1)));
    reject(altered(&p, |mut r| r.set_observed_at(1)));
    reject(altered(&p, |mut r| r.set_execution_error(1)));
    reject(altered(&p, |mut r| r.set_error_code(1)));
    reject(altered(&p, |mut r| r.set_revision(3)));
}
#[test]
fn clock_order_original_error_semantics_and_identity_limits_are_checked() {
    let mut p = sample(Metadata::Reserved);
    p.record.metadata = Metadata::Observed {
        result: original(),
        at: -11,
    };
    assert!(p.encode().is_err());
    p.record.metadata = Metadata::Acknowledged {
        result: original(),
        observed_at: 3,
        at: 2,
    };
    assert!(p.encode().is_err());
    let mut result = original();
    result.unknown = false;
    p.record.metadata = Metadata::Observed { result, at: 0 };
    assert!(p.encode().is_err());
    p.record.metadata = Metadata::Reserved;
    p.record.package_id = "x".repeat(257);
    assert_eq!(p.encode(), Err(Error::Limit));
    p.record.package_id = "bad/path".into();
    assert!(p.encode().is_err());
    p.record.package_id = "ok".into();
    p.record.byte_length = MAX_OBJECT_BYTES + 1;
    assert_eq!(p.encode(), Err(Error::Limit));
}
#[test]
fn ffi_failure_preserves_outputs_and_views_borrow_the_original_owner() {
    use crate::ffi::*;
    let p = sample(Metadata::Acknowledged {
        result: original(),
        observed_at: 0,
        at: 1,
    });
    let bytes = p.encode().unwrap();
    let mut owner = std::ptr::null_mut();
    unsafe {
        assert_eq!(
            mbh_packet_decode(
                bytes.as_ptr(),
                bytes.len() as u32,
                [1u8; 32].as_ptr(),
                [8u8; 32].as_ptr(),
                &mut owner
            ),
            Error::Correlation as u32
        );
        assert!(owner.is_null());
        assert_eq!(
            mbh_packet_decode(
                bytes.as_ptr(),
                bytes.len() as u32,
                [1u8; 32].as_ptr(),
                [9u8; 32].as_ptr(),
                &mut owner
            ),
            0
        );
        let mut out = std::mem::MaybeUninit::<View>::uninit();
        assert_eq!(
            mbh_packet_view(
                owner,
                out.as_mut_ptr(),
                std::mem::size_of::<View>() as u32 - 1
            ),
            Error::Buffer as u32
        );
        assert_eq!(
            mbh_packet_view(owner, out.as_mut_ptr(), std::mem::size_of::<View>() as u32),
            0
        );
        let out = out.assume_init();
        assert_eq!(out.revision, 3);
        assert_eq!(out.original_unknown, 1);
        assert_eq!(out.current_stage, 1);
        assert_eq!(out.exit_code, i32::MIN);
        assert_eq!(
            std::slice::from_raw_parts(out.operation_id.data, out.operation_id.bytes as usize),
            "upload-原记录".as_bytes()
        );
        mbh_packet_free(owner);
        mbh_packet_free(std::ptr::null_mut());
    }
}

#[test]
fn ffi_rejects_overlapping_and_misaligned_outputs_before_writing() {
    use crate::ffi::*;
    use std::{mem::size_of, ptr};
    let bytes = sample(Metadata::Reserved).encode().unwrap();
    let mut storage = vec![0u64; bytes.len().div_ceil(8) + 1];
    let raw = storage.as_mut_ptr().cast::<u8>();
    unsafe { ptr::copy_nonoverlapping(bytes.as_ptr(), raw, bytes.len()) };
    let before = storage.clone();
    let submission = [1u8; 32];
    let nonce = [9u8; 32];
    unsafe {
        // All memory is allocated. These invalid output regions must be
        // rejected before the decoder writes into an input or an unaligned slot.
        assert_eq!(
            mbh_packet_decode(
                raw,
                bytes.len() as u32,
                submission.as_ptr(),
                nonce.as_ptr(),
                raw.cast()
            ),
            Error::Buffer as u32
        );
        assert_eq!(storage, before);
        assert_eq!(
            mbh_packet_decode(
                bytes.as_ptr(),
                bytes.len() as u32,
                submission.as_ptr(),
                nonce.as_ptr(),
                raw.add(1).cast()
            ),
            Error::Buffer as u32
        );
        assert_eq!(storage, before);
        let mut owner = ptr::null_mut();
        assert_eq!(
            mbh_packet_decode(
                bytes.as_ptr(),
                bytes.len() as u32,
                submission.as_ptr(),
                nonce.as_ptr(),
                &mut owner
            ),
            0
        );
        // Rejection must also preserve the live owner for a subsequent view.
        assert_eq!(
            mbh_packet_view(owner, owner.cast(), size_of::<View>() as u32),
            Error::Buffer as u32
        );
        let mut view = std::mem::MaybeUninit::<View>::uninit();
        assert_eq!(
            mbh_packet_view(owner, view.as_mut_ptr(), size_of::<View>() as u32),
            0
        );
        let view = view.assume_init();
        assert_eq!(view.revision, 1);
        assert_eq!(view.original_unknown, 1);
        mbh_packet_free(owner);
    }
}

#[test]
#[ignore = "explicit conformance-vector producer, not a runtime test"]
fn export_native_consumer_vector_when_explicitly_requested() {
    let path =
        std::env::var_os("MORROW_HISTORY_CONFORMANCE_VECTOR").expect("explicit fresh vector path");
    let p = sample(Metadata::Acknowledged {
        result: original(),
        observed_at: 0,
        at: 1,
    });
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .unwrap();
    std::io::Write::write_all(&mut file, &p.encode().unwrap()).unwrap();
}
