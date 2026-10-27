use super::*;
use crate::{MAX_BOUND_INPUT_BYTES, MAX_PROPOSAL_BYTES};
fn view_input() -> ProposalView {
    ProposalView {
        abi_version: 1,
        struct_size: size_of::<ProposalView>() as u32,
        byte_length: 3,
        whole_sha256: [9; 32],
        label: b"Sample".as_ptr(),
        label_length: 6,
        reserved: 0,
        producer_input: b"payload".as_ptr(),
        producer_input_length: 7,
        reserved2: 0,
    }
}
fn id() -> IdentityView {
    IdentityView {
        abi_version: 1,
        struct_size: size_of::<IdentityView>() as u32,
        transfer_epoch: [1; 32],
        object_ref: [2; 32],
        operation_id: [3; 32],
    }
}
fn new_proposal() -> *mut MbpProposal {
    let mut owner = ptr::null_mut();
    let v = view_input();
    // SAFETY: complete input and separate live output slot.
    assert_eq!(
        unsafe { mbp_proposal_new(&v, size_of::<ProposalView>() as u32, &mut owner) },
        0
    );
    owner
}
#[test]
fn native_round_trip_bound_owner_keeps_its_own_proposal_copy() {
    // SAFETY: owners are unique; all buffers live and are freed exactly once.
    unsafe {
        let owner = new_proposal();
        let mut bound = ptr::null_mut();
        let identity = id();
        assert_eq!(
            mbp_bound_new(
                owner,
                &identity,
                size_of::<IdentityView>() as u32,
                &mut bound
            ),
            0
        );
        mbp_proposal_free(owner);
        let mut v = view_input();
        assert_eq!(
            mbp_bound_proposal_view(bound, &mut v, size_of::<ProposalView>() as u32),
            0
        );
        assert_eq!(
            std::slice::from_raw_parts(v.label, v.label_length as usize),
            b"Sample"
        );
        let mut actual = id();
        actual.operation_id = [0; 32];
        assert_eq!(
            mbp_bound_identity(bound, &mut actual, size_of::<IdentityView>() as u32),
            0
        );
        assert_eq!(actual.operation_id, identity.operation_id);
        let mut wire = vec![0xaa; MAX_BOUND_INPUT_BYTES];
        let mut length = 0;
        assert_eq!(
            mbp_bound_encode(bound, wire.as_mut_ptr(), wire.len() as u32, &mut length),
            0
        );
        assert!(wire[length as usize..].iter().all(|b| *b == 0xaa));
        let mut decoded = ptr::null_mut();
        assert_eq!(mbp_bound_decode(wire.as_ptr(), length, &mut decoded), 0);
        mbp_bound_free(decoded);
        mbp_bound_free(bound);
    }
}
#[test]
fn bad_version_size_and_reserved_fields_leave_owner_slot_untouched() {
    let sentinel = ptr::dangling_mut::<MbpProposal>();
    for case in 0..5 {
        let mut v = view_input();
        let mut size = size_of::<ProposalView>() as u32;
        match case {
            0 => v.abi_version = 2,
            1 => v.struct_size -= 1,
            2 => v.reserved = 1,
            3 => v.reserved2 = 1,
            _ => size -= 1,
        }
        let mut out = sentinel;
        // SAFETY: input is live and the error output slot is never dereferenced as an owner.
        assert_ne!(unsafe { mbp_proposal_new(&v, size, &mut out) }, 0);
        assert_eq!(out, sentinel);
    }
}
#[test]
fn null_and_oversized_ranges_refuse_without_accessing_the_bytes() {
    for (label, length, producer, producer_length) in [
        (ptr::null(), 1, ptr::null(), 0),
        (ptr::dangling::<u8>(), 513, ptr::null(), 0),
        (b"a".as_ptr(), 1, ptr::null(), 1),
        (b"a".as_ptr(), 1, ptr::dangling::<u8>(), 32769),
    ] {
        let mut v = view_input();
        v.label = label;
        v.label_length = length;
        v.producer_input = producer;
        v.producer_input_length = producer_length;
        let mut out = ptr::null_mut();
        // SAFETY: invalid length/null cases are rejected before dereference.
        assert_ne!(
            unsafe { mbp_proposal_new(&v, size_of::<ProposalView>() as u32, &mut out) },
            0
        );
        assert!(out.is_null());
    }
}
#[test]
fn invalid_utf8_and_label_controls_are_not_exposed_as_valid_views() {
    for bytes in [&b"\xff"[..], &b"bad\0label"[..], &b"bad\nlabel"[..]] {
        let mut v = view_input();
        v.label = bytes.as_ptr();
        v.label_length = bytes.len() as u32;
        let mut out = ptr::null_mut();
        // SAFETY: live bounded input and output.
        assert_eq!(
            unsafe { mbp_proposal_new(&v, size_of::<ProposalView>() as u32, &mut out) },
            Error::Invalid as u32
        );
        assert!(out.is_null());
    }
}
#[test]
fn failed_encoding_preserves_output_and_written_and_rejects_overlap() {
    // SAFETY: live owner and buffers; overlap cases must reject before writing.
    unsafe {
        let owner = new_proposal();
        let mut wire = vec![0xab; MAX_PROPOSAL_BYTES];
        let mut written = 91;
        assert_eq!(
            mbp_proposal_encode(owner, wire.as_mut_ptr(), 1, &mut written),
            Error::Buffer as u32
        );
        assert_eq!(written, 91);
        assert!(wire.iter().all(|b| *b == 0xab));
        let alias = wire.as_mut_ptr().cast::<u32>();
        assert_eq!(
            mbp_proposal_encode(owner, wire.as_mut_ptr(), wire.len() as u32, alias),
            Error::Buffer as u32
        );
        assert!(wire.iter().all(|b| *b == 0xab));
        let mut v = view_input();
        assert_eq!(
            mbp_proposal_view_get(owner, &mut v, size_of::<ProposalView>() as u32),
            0
        );
        assert_eq!(
            mbp_proposal_encode(
                owner,
                v.label.cast_mut(),
                MAX_PROPOSAL_BYTES as u32,
                &mut written
            ),
            Error::Buffer as u32
        );
        assert_eq!(
            mbp_proposal_view_get(
                owner,
                owner.cast::<ProposalView>(),
                size_of::<ProposalView>() as u32
            ),
            Error::Buffer as u32
        );
        let mut id = id();
        let mut bound = ptr::null_mut();
        assert_eq!(
            mbp_bound_new(owner, &id, size_of::<IdentityView>() as u32, &mut bound),
            0
        );
        assert_eq!(
            mbp_bound_identity(
                bound,
                bound.cast::<IdentityView>(),
                size_of::<IdentityView>() as u32
            ),
            Error::Buffer as u32
        );
        assert_eq!(
            mbp_bound_proposal_view(
                bound,
                bound.cast::<ProposalView>(),
                size_of::<ProposalView>() as u32
            ),
            Error::Buffer as u32
        );
        assert_eq!(mbp_bound_identity(bound, &mut id, 1), Error::Buffer as u32);
        assert_eq!(
            mbp_bound_encode(bound, wire.as_mut_ptr(), 1, &mut written),
            Error::Buffer as u32
        );
        assert_eq!(written, 91);
        mbp_bound_free(bound);
        mbp_proposal_free(owner);
    }
}
#[test]
fn malformed_decode_does_not_replace_slots_and_overlapping_inputs_refuse() {
    // SAFETY: live readable bytes; invalid output aliases must not be written.
    unsafe {
        let mut bytes = vec![0; 128];
        let p = ptr::dangling_mut::<MbpProposal>();
        let mut out = p;
        assert_ne!(
            mbp_proposal_decode(bytes.as_ptr(), bytes.len() as u32, &mut out),
            0
        );
        assert_eq!(out, p);
        let b = ptr::dangling_mut::<MbpBoundInput>();
        let mut bound = b;
        assert_ne!(
            mbp_bound_decode(bytes.as_ptr(), bytes.len() as u32, &mut bound),
            0
        );
        assert_eq!(bound, b);
        assert_eq!(
            mbp_proposal_decode(
                bytes.as_ptr(),
                bytes.len() as u32,
                bytes.as_mut_ptr().cast()
            ),
            Error::Buffer as u32
        );
        assert_eq!(
            mbp_bound_decode(
                bytes.as_ptr(),
                bytes.len() as u32,
                bytes.as_mut_ptr().cast()
            ),
            Error::Buffer as u32
        );
        assert!(bytes.iter().all(|b| *b == 0));
    }
}
#[test]
fn proposal_and_identity_inputs_cannot_overlap_their_new_owner_slots() {
    // SAFETY: all inputs live; deliberately overlapping outputs reject unchanged.
    unsafe {
        let mut v = view_input();
        let ptr = ptr::from_mut(&mut v);
        let old_version = v.abi_version;
        assert_eq!(
            mbp_proposal_new(ptr, size_of::<ProposalView>() as u32, ptr.cast()),
            Error::Buffer as u32
        );
        assert_eq!(v.abi_version, old_version);
        let owner = new_proposal();
        let mut identity = id();
        let ptr = ptr::from_mut(&mut identity);
        assert_eq!(
            mbp_bound_new(owner, ptr, size_of::<IdentityView>() as u32, ptr.cast()),
            Error::Buffer as u32
        );
        assert_eq!(identity.abi_version, 1);
        let mut out = ptr::null_mut();
        identity.operation_id = [0; 32];
        assert_ne!(
            mbp_bound_new(owner, &identity, size_of::<IdentityView>() as u32, &mut out),
            0
        );
        assert!(out.is_null());
        mbp_proposal_free(owner);
    }
}
#[test]
fn input_is_copied_and_null_handles_fail_without_touching_views() {
    // SAFETY: owned source bytes remain live until new returns; handles used once.
    unsafe {
        let mut label = b"Mutable".to_vec();
        let mut data = [1, 2, 3];
        let mut v = view_input();
        v.label = label.as_ptr();
        v.label_length = label.len() as u32;
        v.producer_input = data.as_ptr();
        v.producer_input_length = 3;
        let mut p = ptr::null_mut();
        assert_eq!(
            mbp_proposal_new(&v, size_of::<ProposalView>() as u32, &mut p),
            0
        );
        label.fill(b'x');
        data.fill(9);
        assert_eq!(
            mbp_proposal_view_get(p, &mut v, size_of::<ProposalView>() as u32),
            0
        );
        assert_eq!(
            std::slice::from_raw_parts(v.label, v.label_length as usize),
            b"Mutable"
        );
        assert_eq!(std::slice::from_raw_parts(v.producer_input, 3), [1, 2, 3]);
        assert_eq!(
            mbp_proposal_view_get(ptr::null(), &mut v, size_of::<ProposalView>() as u32),
            Error::Buffer as u32
        );
        assert_eq!(v.byte_length, 3);
        mbp_proposal_free(p);
        mbp_proposal_free(ptr::null_mut());
        mbp_bound_free(ptr::null_mut());
    }
}
