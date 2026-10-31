use super::*;
use capnp::{message::Builder, serialize};
use morrow_blob_upload_history_v1::{Metadata, OriginalResult, Outcome, Record, Stage, task_id};
use morrow_plugin_sdk::task_capnp as wire;
use sha2::{Digest, Sha256};

const HANDLER: &str = "history.inspect";
fn packet() -> Packet {
    Packet::new(
        [9; 32],
        Record {
            submission: [1; 32],
            package_id: "original.uploader".into(),
            package_digest: [2; 32],
            registry_revision: u64::MAX,
            operation_id: "upload-原记录".into(),
            byte_length: 0,
            whole_sha256: Sha256::digest([]).into(),
            handler: "old.upload".into(),
            input_sha256: [3; 32],
            request_binding_sha256: [4; 32],
            reserved_at: -10,
            metadata: Metadata::Acknowledged {
                result: OriginalResult {
                    outcome: Outcome::Error(7),
                    unknown: true,
                    calls: 0,
                    wire_bytes: 0,
                    native_work_bytes: 0,
                    execution: None,
                },
                observed_at: 0,
                at: 1,
            },
            current_stage: Stage::Staged,
        },
    )
    .unwrap()
}
fn message(id: &str) -> Builder<capnp::message::HeapAllocator> {
    let mut message = Builder::new_default();
    let mut root = message.init_root::<wire::invocation::Builder>();
    root.set_version(
        include_str!("../../../../sdk/rust/contracts/task-version.txt")
            .trim()
            .parse()
            .unwrap(),
    );
    let schema = include_str!("../../../../sdk/rust/contracts/task.capnp").replace("\r\n", "\n");
    root.set_schema_digest(&Sha256::digest(schema.as_bytes()));
    root.set_task_id(id);
    root.set_kind(wire::Kind::Transform);
    let mut transform = root.init_transform();
    transform.set_handler(HANDLER);
    transform.set_input_type(INPUT_TYPE);
    transform.set_output_type(INPUT_TYPE);
    transform.set_input(&packet().encode().unwrap());
    message
}
fn valid() -> Vec<u8> {
    serialize::write_message_to_words(&message(&task_id([1; 32], [9; 32]).unwrap()))
}
fn wrong_id() -> Vec<u8> {
    serialize::write_message_to_words(&message(&task_id([1; 32], [8; 32]).unwrap()))
}
unsafe fn decode(bytes: &[u8], out: *mut *mut MbhPacket) -> u32 {
    unsafe {
        mbh_packet_decode_invocation(
            bytes.as_ptr(),
            bytes.len() as u32,
            HANDLER.as_ptr(),
            HANDLER.len() as u32,
            INPUT_TYPE.as_ptr(),
            INPUT_TYPE.len() as u32,
            out,
        )
    }
}

#[test]
fn original_task_id_is_independent_of_packet_claims() {
    let bytes = valid();
    assert_eq!(
        decode_invocation(&bytes, HANDLER, INPUT_TYPE).unwrap(),
        packet()
    );
    for id in [
        task_id([2; 32], [9; 32]).unwrap(),
        task_id([1; 32], [8; 32]).unwrap(),
        "history-v1.invalid".into(),
    ] {
        let bytes = serialize::write_message_to_words(&message(&id));
        assert_eq!(
            decode_invocation(&bytes, HANDLER, INPUT_TYPE),
            Err(Error::Correlation)
        );
    }
    let decoded = decode_invocation(&bytes, HANDLER, INPUT_TYPE).unwrap();
    assert!(decoded.record().original_unknown());
    assert_eq!(decoded.record().current_stage, Stage::Staged);
}

#[test]
fn task_contract_kind_and_recipient_declarations_are_checked() {
    let bytes = valid();
    for (handler, output) in [("other.handler", INPUT_TYPE), (HANDLER, "other.type")] {
        assert_eq!(
            decode_invocation(&bytes, handler, output),
            Err(Error::Contract)
        );
    }
    for mode in 0..6 {
        let mut msg = message(&task_id([1; 32], [9; 32]).unwrap());
        let mut root = msg.get_root::<wire::invocation::Builder>().unwrap();
        match mode {
            0 => root.set_version(u16::MAX),
            1 => root.set_schema_digest(&[0; 32]),
            2 => root.get_transform().unwrap().set_input_type("another.type"),
            3 => root.set_kind(wire::Kind::ContentCommand),
            4 => root.set_command(&[1]),
            _ => root.get_transform().unwrap().set_input(&[]),
        }
        assert!(
            decode_invocation(
                &serialize::write_message_to_words(&msg),
                HANDLER,
                INPUT_TYPE
            )
            .is_err()
        );
    }
    for invalid in ["", "x/y", "x\\y", "x:y", "x\ny"] {
        assert_eq!(
            decode_invocation(&bytes, invalid, INPUT_TYPE),
            Err(Error::Invalid)
        );
        assert_eq!(
            decode_invocation(&bytes, HANDLER, invalid),
            Err(Error::Invalid)
        );
    }
    assert_eq!(
        decode_invocation(&bytes, &"x".repeat(257), INPUT_TYPE),
        Err(Error::Limit)
    );
}

#[test]
fn bounded_framing_rejects_every_truncation_and_trailing_data() {
    let bytes = valid();
    for end in 0..bytes.len() {
        assert!(decode_invocation(&bytes[..end], HANDLER, INPUT_TYPE).is_err());
    }
    for n in [1, 7, 8, 16] {
        let mut trailing = bytes.clone();
        trailing.extend(vec![0; n]);
        assert!(decode_invocation(&trailing, HANDLER, INPUT_TYPE).is_err());
    }
    assert_eq!(
        decode_invocation(&vec![0; MAX_TASK_BYTES + 1], HANDLER, INPUT_TYPE),
        Err(Error::Limit)
    );
}

#[test]
fn ffi_refusal_preserves_existing_owner_and_view() {
    let bytes = valid();
    let mut owner = std::ptr::null_mut();
    unsafe {
        assert_eq!(decode(&bytes, &mut owner), 0);
        let old = owner;
        assert_eq!(decode(&wrong_id(), &mut owner), Error::Correlation as u32);
        assert_eq!(owner, old);
        assert_eq!(
            mbh_packet_decode_invocation(
                bytes.as_ptr(),
                bytes.len() as u32,
                [0xff].as_ptr(),
                1,
                INPUT_TYPE.as_ptr(),
                INPUT_TYPE.len() as u32,
                &mut owner
            ),
            1
        );
        assert_eq!(owner, old);
        let mut view = std::mem::zeroed::<morrow_blob_upload_history_v1::ffi::View>();
        assert_eq!(
            morrow_blob_upload_history_v1::ffi::mbh_packet_view(
                owner,
                &mut view,
                size_of::<morrow_blob_upload_history_v1::ffi::View>() as u32
            ),
            0
        );
        assert_eq!(view.original_unknown, 1);
        assert_eq!(view.current_stage, 1);
        morrow_blob_upload_history_v1::ffi::mbh_packet_free(owner);
    }
}

#[test]
fn ffi_rejects_null_unaligned_overlapping_and_overflowing_ranges() {
    let bytes = valid();
    let mut output = [0xa5a5_a5a5_a5a5_a5a5u64; 8];
    let original = output;
    let slot = output.as_mut_ptr().cast::<*mut MbhPacket>();
    unsafe {
        assert_eq!(decode(&bytes, std::ptr::null_mut()), 5);
        assert_eq!(
            decode(&bytes, output.as_mut_ptr().cast::<u8>().add(1).cast()),
            5
        );
        assert_eq!(
            mbh_packet_decode_invocation(
                std::ptr::null(),
                1,
                HANDLER.as_ptr(),
                HANDLER.len() as u32,
                INPUT_TYPE.as_ptr(),
                INPUT_TYPE.len() as u32,
                slot
            ),
            5
        );
        assert_eq!(
            mbh_packet_decode_invocation(
                usize::MAX as *const u8,
                2,
                HANDLER.as_ptr(),
                HANDLER.len() as u32,
                INPUT_TYPE.as_ptr(),
                INPUT_TYPE.len() as u32,
                slot
            ),
            5
        );
        for which in 0..3 {
            let p = output.as_ptr().cast::<u8>();
            let (b, h, t) = (
                if which == 0 { p } else { bytes.as_ptr() },
                if which == 1 { p } else { HANDLER.as_ptr() },
                if which == 2 { p } else { INPUT_TYPE.as_ptr() },
            );
            assert_eq!(
                mbh_packet_decode_invocation(
                    b,
                    if which == 0 { 8 } else { bytes.len() as u32 },
                    h,
                    8,
                    t,
                    8,
                    slot
                ),
                5
            );
        }
        assert_eq!(
            mbh_packet_decode_invocation(
                bytes.as_ptr(),
                (MAX_TASK_BYTES + 1) as u32,
                HANDLER.as_ptr(),
                HANDLER.len() as u32,
                INPUT_TYPE.as_ptr(),
                INPUT_TYPE.len() as u32,
                slot
            ),
            3
        );
    }
    assert_eq!(output, original);
}

#[test]
fn unaligned_task_bytes_are_supported_without_changing_correlation() {
    let bytes = valid();
    let mut unaligned = vec![0xa5];
    unaligned.extend_from_slice(&bytes);
    assert_eq!(
        decode_invocation(&unaligned[1..], HANDLER, INPUT_TYPE).unwrap(),
        packet()
    );
    let mut owner = std::ptr::null_mut();
    unsafe {
        assert_eq!(decode(&unaligned[1..], &mut owner), 0);
        morrow_blob_upload_history_v1::ffi::mbh_packet_free(owner);
    }
}

#[test]
#[ignore = "Explicit vector export, not qualification credit"]
fn export_native_task_vectors() {
    let root = std::path::PathBuf::from(std::env::var("MORROW_HISTORY_TASK_VECTORS").unwrap());
    assert!(!root.exists());
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("valid.capnp"), valid()).unwrap();
    std::fs::write(root.join("wrong-id.capnp"), wrong_id()).unwrap();
}
