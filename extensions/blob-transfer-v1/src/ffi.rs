//! Opaque owning handles, small checked borrowed views. Valid native memory and
//! exclusive mutable receiver access are caller obligations, not pointer isolation.
//! Ordinary allocator OOM is not promised recoverable. No authorization callbacks.
use crate::{
    Acceptance, ActionRef, Error, Frame, FrameRef, Identity, Limits, MAX_FRAME_BYTES,
    ReceiptStatus, Receiver, SCHEMA_DIGEST,
};
#[repr(C)]
pub struct CFrame {
    value: Frame,
}
#[repr(C)]
pub struct CReceiver {
    value: Receiver,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct View {
    pub abi_version: u32,
    pub struct_size: u32,
    pub kind: u32,
    pub status: u32,
    pub transfer_epoch: [u8; 32],
    pub object_ref: [u8; 32],
    pub operation_id: [u8; 32],
    pub total_length: u64,
    pub sequence: u64,
    pub offset: u64,
    pub length: u64,
    pub whole_sha256: [u8; 32],
    pub chunk_sha256: [u8; 32],
    pub request_digest: [u8; 32],
    pub payload: *const u8,
    pub payload_length: u32,
    pub reserved: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CLimits {
    pub abi_version: u32,
    pub struct_size: u32,
    pub max_payload_bytes: u64,
    pub max_wire_bytes: u64,
    pub max_request_bytes: u64,
    pub max_response_bytes: u64,
    pub max_requests: u64,
    pub max_receipts: u64,
    pub reserved: u64,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CSnapshot {
    pub abi_version: u32,
    pub struct_size: u32,
    pub phase: u32,
    pub has_descriptor: u32,
    pub retained_recent_requests: u32,
    pub retained_recent_wire_bytes: u32,
    pub transfer_epoch: [u8; 32],
    pub object_ref: [u8; 32],
    pub operation_id: [u8; 32],
    pub total_length: u64,
    pub received_bytes: u64,
    pub next_sequence: u64,
    pub request_bytes: u64,
    pub response_bytes: u64,
    pub wire_bytes: u64,
    pub requests: u64,
    pub receipts: u64,
    pub expected_sha256: [u8; 32],
    pub verified_sha256: [u8; 32],
}
fn overlap(a: *const u8, alen: usize, b: *const u8, blen: usize) -> Option<bool> {
    if alen == 0 || blen == 0 {
        return Some(false);
    }
    let a = a as usize;
    let b = b as usize;
    Some(a < b.checked_add(blen)? && b < a.checked_add(alen)?)
}
fn valid<T>(p: *const T, size: u32) -> bool {
    !p.is_null() && p.is_aligned() && size as usize >= std::mem::size_of::<T>()
}
fn slot<T>(p: *mut T) -> bool {
    !p.is_null() && p.is_aligned()
}
unsafe fn bytes<'a>(p: *const u8, len: u32) -> &'a [u8] {
    if len == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(p, len as usize) }
    }
}
fn identity(v: &View) -> Identity {
    Identity {
        transfer_epoch: v.transfer_epoch,
        object_ref: v.object_ref,
        operation_id: v.operation_id,
    }
}
impl View {
    fn blank(id: Identity) -> Self {
        Self {
            abi_version: 1,
            struct_size: std::mem::size_of::<Self>() as u32,
            kind: 0,
            status: 0,
            transfer_epoch: id.transfer_epoch,
            object_ref: id.object_ref,
            operation_id: id.operation_id,
            total_length: 0,
            sequence: 0,
            offset: 0,
            length: 0,
            whole_sha256: [0; 32],
            chunk_sha256: [0; 32],
            request_digest: [0; 32],
            payload: std::ptr::null(),
            payload_length: 0,
            reserved: 0,
        }
    }
    fn of(frame: &Frame) -> Self {
        let mut v = Self::blank(frame.identity);
        match frame.as_ref().action {
            ActionRef::Descriptor {
                total_length,
                whole_sha256,
            } => {
                v.kind = 0;
                v.total_length = total_length;
                v.whole_sha256 = whole_sha256;
            }
            ActionRef::Chunk {
                sequence,
                offset,
                payload,
                chunk_sha256,
            } => {
                v.kind = 1;
                v.sequence = sequence;
                v.offset = offset;
                v.payload = if payload.is_empty() {
                    std::ptr::null()
                } else {
                    payload.as_ptr()
                };
                v.payload_length = payload.len() as u32;
                v.chunk_sha256 = chunk_sha256;
            }
            ActionRef::Receipt {
                request_digest,
                sequence,
                offset,
                length,
                chunk_sha256,
                status,
            } => {
                v.kind = 2;
                v.request_digest = request_digest;
                v.sequence = sequence;
                v.offset = offset;
                v.length = length;
                v.chunk_sha256 = chunk_sha256;
                v.status = status as u32;
            }
            ActionRef::End {
                total_length,
                whole_sha256,
            } => {
                v.kind = 3;
                v.total_length = total_length;
                v.whole_sha256 = whole_sha256;
            }
        }
        v
    }
    unsafe fn frame(&self) -> crate::Result<Frame> {
        if self.abi_version != 1
            || self.struct_size as usize != std::mem::size_of::<Self>()
            || self.reserved != 0
        {
            return Err(Error::Invalid);
        }
        let action = match self.kind {
            0 | 3 => {
                if self.status != 0
                    || self.sequence != 0
                    || self.offset != 0
                    || self.length != 0
                    || self.chunk_sha256 != [0; 32]
                    || self.request_digest != [0; 32]
                    || !self.payload.is_null()
                    || self.payload_length != 0
                {
                    return Err(Error::Invalid);
                }
                if self.kind == 0 {
                    ActionRef::Descriptor {
                        total_length: self.total_length,
                        whole_sha256: self.whole_sha256,
                    }
                } else {
                    ActionRef::End {
                        total_length: self.total_length,
                        whole_sha256: self.whole_sha256,
                    }
                }
            }
            1 => {
                if self.total_length != 0
                    || self.status != 0
                    || self.length != 0
                    || self.whole_sha256 != [0; 32]
                    || self.request_digest != [0; 32]
                    || (self.payload.is_null() && self.payload_length != 0)
                {
                    return Err(Error::Invalid);
                }
                if self.payload_length as usize > crate::MAX_CHUNK_BYTES {
                    return Err(Error::Limit);
                }
                ActionRef::Chunk {
                    sequence: self.sequence,
                    offset: self.offset,
                    payload: unsafe { bytes(self.payload, self.payload_length) },
                    chunk_sha256: self.chunk_sha256,
                }
            }
            2 => {
                if self.total_length != 0
                    || self.whole_sha256 != [0; 32]
                    || !self.payload.is_null()
                    || self.payload_length != 0
                {
                    return Err(Error::Invalid);
                }
                ActionRef::Receipt {
                    request_digest: self.request_digest,
                    sequence: self.sequence,
                    offset: self.offset,
                    length: self.length,
                    chunk_sha256: self.chunk_sha256,
                    status: match self.status {
                        0 => ReceiptStatus::Accepted,
                        1 => ReceiptStatus::Existing,
                        _ => return Err(Error::Invalid),
                    },
                }
            }
            _ => return Err(Error::Invalid),
        };
        FrameRef {
            identity: identity(self),
            action,
        }
        .to_owned()
    }
}
fn frame_output_overlap(frame: *const CFrame, v: &Frame, out: *const u8, length: usize) -> bool {
    if overlap(frame.cast(), std::mem::size_of::<CFrame>(), out, length) != Some(false) {
        return true;
    }
    if let ActionRef::Chunk { payload, .. } = v.as_ref().action
        && overlap(payload.as_ptr(), payload.len(), out, length) != Some(false)
    {
        return true;
    }
    false
}
/// # Safety
/// Input readable for length; out aligned/writable for one opaque pointer slot.
/// Null/zero or >64KiB input rejects before read. Input/slot may alias because
/// the validated frame is owned before the output write. Every error keeps slot
/// unchanged. Successful handle must be freed once; an old slot is not auto-freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbt_frame_decode(
    input: *const u8,
    length: u32,
    out: *mut *mut CFrame,
) -> u32 {
    if !slot(out) || input.is_null() {
        return Error::Invalid as u32;
    }
    if length == 0 || length as usize > MAX_FRAME_BYTES {
        return Error::Limit as u32;
    }
    match Frame::decode(unsafe { bytes(input, length) }) {
        Ok(frame) => {
            let p = Box::into_raw(Box::new(CFrame { value: frame }));
            unsafe { out.write(p) };
            0
        }
        Err(e) => e as u32,
    }
}
/// # Safety
/// View aligned/readable for sizeof(View), out one writable pointer slot. Payload
/// readable for its declared bound; null only for zero. View/payload/output may
/// alias: all fields and payload are owned before output write. Errors preserve
/// slot. Unused view fields must be zero; ABI version/struct_size must match.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbt_frame_set(view: *const View, size: u32, out: *mut *mut CFrame) -> u32 {
    if !valid(view, size) || !slot(out) {
        return Error::Invalid as u32;
    }
    let view = unsafe { view.read() };
    match unsafe { view.frame() } {
        Ok(frame) => {
            let p = Box::into_raw(Box::new(CFrame { value: frame }));
            unsafe { out.write(p) };
            0
        }
        Err(e) => e as u32,
    }
}
/// # Safety
/// Null is harmless; otherwise frame must be a live unique handle created by this
/// library, with no use-after-free/double-free/concurrent borrower. Native forged
/// handles are not sandboxed. All borrowed views expire here.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbt_frame_free(frame: *mut CFrame) {
    if !frame.is_null() {
        unsafe {
            drop(Box::from_raw(frame));
        }
    }
}
/// # Safety
/// Live frame handle; out aligned/writable for View. Output may not overlap the
/// handle or its owned payload (checked). View owns metadata but payload is read
/// only borrowed until frame free; no write/reallocate/concurrent mutation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbt_frame_view(frame: *const CFrame, out: *mut View, size: u32) -> u32 {
    if !valid(frame, std::mem::size_of::<CFrame>() as u32) || !valid(out, size) {
        return Error::Invalid as u32;
    }
    let value = unsafe { &(*frame).value };
    if frame_output_overlap(frame, value, out.cast(), std::mem::size_of::<View>()) {
        return Error::Invalid as u32;
    }
    let view = View::of(value);
    unsafe { out.write(view) };
    0
}
/// # Safety
/// Live frame; output writable for capacity and out_length aligned/writable for
/// u32. Output prefix and length may not overlap each other or handle/payload
/// memory (checked). Encoding happens before any write; errors preserve both
/// outputs. Success writes only encoded prefix and length. OOM may abort.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbt_frame_encode(
    frame: *const CFrame,
    out: *mut u8,
    capacity: u32,
    out_length: *mut u32,
) -> u32 {
    if !valid(frame, std::mem::size_of::<CFrame>() as u32) || out.is_null() || !slot(out_length) {
        return Error::Invalid as u32;
    }
    let value = unsafe { &(*frame).value };
    let encoded = match value.encode() {
        Ok(v) => v,
        Err(e) => return e as u32,
    };
    if (capacity as usize) < encoded.len() {
        return Error::Buffer as u32;
    }
    if overlap(
        out,
        encoded.len(),
        out_length.cast(),
        std::mem::size_of::<u32>(),
    ) != Some(false)
        || frame_output_overlap(frame, value, out, encoded.len())
        || frame_output_overlap(frame, value, out_length.cast(), std::mem::size_of::<u32>())
    {
        return Error::Invalid as u32;
    }
    unsafe {
        std::ptr::copy_nonoverlapping(encoded.as_ptr(), out, encoded.len());
        out_length.write(encoded.len() as u32)
    };
    0
}
/// # Safety
/// Out writable for capacity. Errors preserve output; success writes32 bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbt_schema_digest(out: *mut u8, capacity: u32) -> u32 {
    if out.is_null() {
        return Error::Invalid as u32;
    }
    if capacity < 32 {
        return Error::Buffer as u32;
    }
    unsafe { std::ptr::copy_nonoverlapping(SCHEMA_DIGEST.as_ptr(), out, 32) };
    0
}
impl CLimits {
    fn from_limits(v: Limits) -> Self {
        Self {
            abi_version: 1,
            struct_size: std::mem::size_of::<Self>() as u32,
            max_payload_bytes: v.max_payload_bytes,
            max_wire_bytes: v.max_wire_bytes,
            max_request_bytes: v.max_request_bytes,
            max_response_bytes: v.max_response_bytes,
            max_requests: v.max_requests,
            max_receipts: v.max_receipts,
            reserved: 0,
        }
    }
    fn limits(&self) -> crate::Result<Limits> {
        if self.abi_version != 1
            || self.struct_size as usize != std::mem::size_of::<Self>()
            || self.reserved != 0
        {
            return Err(Error::Invalid);
        }
        let v = Limits {
            max_payload_bytes: self.max_payload_bytes,
            max_wire_bytes: self.max_wire_bytes,
            max_request_bytes: self.max_request_bytes,
            max_response_bytes: self.max_response_bytes,
            max_requests: self.max_requests,
            max_receipts: self.max_receipts,
        };
        v.validate()?;
        Ok(v)
    }
}
/// # Safety
/// Out aligned/writable for CLimits; size covers it. Errors preserve bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbt_limits_default(out: *mut CLimits, size: u32) -> u32 {
    if !valid(out, size) {
        return Error::Invalid as u32;
    }
    unsafe { out.write(CLimits::from_limits(Limits::default())) };
    0
}
/// # Safety
/// Limits aligned/readable for CLimits and size covers it; null is allowed only
/// with size0 to select defaults. Out aligned/writable pointer slot. Inputs may
/// alias output slot (copied before write). Errors preserve slot. Limits are
/// helper accounting only, never an IoBinding or live authority replacement.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbt_receiver_new(
    limits: *const CLimits,
    size: u32,
    out: *mut *mut CReceiver,
) -> u32 {
    if !slot(out) {
        return Error::Invalid as u32;
    }
    let limits = if limits.is_null() {
        if size != 0 {
            return Error::Invalid as u32;
        }
        Limits::default()
    } else {
        if !valid(limits, size) {
            return Error::Invalid as u32;
        }
        let v = unsafe { limits.read() };
        match v.limits() {
            Ok(v) => v,
            Err(e) => return e as u32,
        }
    };
    match Receiver::new(limits) {
        Ok(value) => {
            let p = Box::into_raw(Box::new(CReceiver { value }));
            unsafe { out.write(p) };
            0
        }
        Err(e) => e as u32,
    }
}
/// # Safety
/// Null harmless; otherwise a live unique library-created receiver, no concurrent
/// calls or borrowed access. Release preserves no external effects/refund/authority.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbt_receiver_free(receiver: *mut CReceiver) {
    if !receiver.is_null() {
        unsafe {
            drop(Box::from_raw(receiver));
        }
    }
}
/// # Safety
/// Live receiver with exclusive mutable access. Cancellation releases its one
/// retained request but keeps all accounting/history; verified state remains
/// terminal. This signal is neither original owner cancellation nor a worker join.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbt_receiver_cancel(receiver: *mut CReceiver) -> u32 {
    if !valid(receiver, std::mem::size_of::<CReceiver>() as u32) {
        return Error::Invalid as u32;
    }
    unsafe { (*receiver).value.cancel() };
    0
}
/// # Safety
/// Live receiver; output aligned/writable for CSnapshot and disjoint from receiver
/// allocation (checked). Errors preserve output. Snapshot contains no borrowed
/// pointer or authority. VerifiedBytes reports actual byte digest only.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbt_receiver_snapshot(
    receiver: *const CReceiver,
    out: *mut CSnapshot,
    size: u32,
) -> u32 {
    if !valid(receiver, std::mem::size_of::<CReceiver>() as u32)
        || !valid(out, size)
        || overlap(
            receiver.cast(),
            std::mem::size_of::<CReceiver>(),
            out.cast(),
            std::mem::size_of::<CSnapshot>(),
        ) != Some(false)
    {
        return Error::Invalid as u32;
    }
    let s = unsafe { (*receiver).value.snapshot() };
    let id = s.identity.unwrap_or(Identity {
        transfer_epoch: [0; 32],
        object_ref: [0; 32],
        operation_id: [0; 32],
    });
    let v = CSnapshot {
        abi_version: 1,
        struct_size: std::mem::size_of::<CSnapshot>() as u32,
        phase: s.phase as u32,
        has_descriptor: u32::from(s.identity.is_some()),
        retained_recent_requests: s.retained_recent_requests,
        retained_recent_wire_bytes: s.retained_recent_wire_bytes,
        transfer_epoch: id.transfer_epoch,
        object_ref: id.object_ref,
        operation_id: id.operation_id,
        total_length: s.total_length,
        received_bytes: s.received_bytes,
        next_sequence: s.next_sequence,
        request_bytes: s.request_bytes,
        response_bytes: s.response_bytes,
        wire_bytes: s.wire_bytes,
        requests: s.requests,
        receipts: s.receipts,
        expected_sha256: s.expected_sha256,
        verified_sha256: s.verified_sha256,
    };
    unsafe { out.write(v) };
    0
}
/// # Safety
/// Live exclusively borrowed receiver, input readable for length. out_kind and
/// out_receipt writable/aligned pointer slots, disjoint from each other/receiver
/// allocation (checked), and from any other live opaque storage. Input may alias
/// output slots, but never receiver allocation/retained wire (checked); accept
/// reads/owns it before writing. Errors preserve both output
/// slots and semantic progress, but admitted request fees remain charged. Success
/// kind0=descriptor (null receipt),1=owned receipt,2=VerifiedBytes(null receipt).
/// Out receipt's old value is not auto-freed; new nonnull receipt must be freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbt_receiver_accept(
    receiver: *mut CReceiver,
    input: *const u8,
    length: u32,
    out_kind: *mut u32,
    out_receipt: *mut *mut CFrame,
) -> u32 {
    if !valid(receiver, std::mem::size_of::<CReceiver>() as u32)
        || !slot(out_kind)
        || !slot(out_receipt)
        || input.is_null()
        || overlap(
            out_kind.cast(),
            4,
            out_receipt.cast(),
            std::mem::size_of::<*mut CFrame>(),
        ) != Some(false)
        || overlap(
            receiver.cast(),
            std::mem::size_of::<CReceiver>(),
            out_kind.cast(),
            4,
        ) != Some(false)
        || overlap(
            receiver.cast(),
            std::mem::size_of::<CReceiver>(),
            out_receipt.cast(),
            std::mem::size_of::<*mut CFrame>(),
        ) != Some(false)
    {
        return Error::Invalid as u32;
    }
    if length == 0 || length as usize > MAX_FRAME_BYTES {
        return Error::Limit as u32;
    }
    let retained = unsafe { (*receiver).value.retained_wire() };
    if overlap(
        receiver.cast(),
        std::mem::size_of::<CReceiver>(),
        input,
        length as usize,
    ) != Some(false)
        || overlap(retained.as_ptr(), retained.len(), input, length as usize) != Some(false)
        || overlap(retained.as_ptr(), retained.len(), out_kind.cast(), 4) != Some(false)
        || overlap(
            retained.as_ptr(),
            retained.len(),
            out_receipt.cast(),
            std::mem::size_of::<*mut CFrame>(),
        ) != Some(false)
    {
        return Error::Invalid as u32;
    }
    let result = unsafe { (*receiver).value.accept(bytes(input, length)) };
    let (kind, pointer) = match result {
        Ok(Acceptance::DescriptorAccepted) => (0, std::ptr::null_mut()),
        Ok(Acceptance::Receipt(frame)) => (1, Box::into_raw(Box::new(CFrame { value: frame }))),
        Ok(Acceptance::VerifiedBytes { .. }) => (2, std::ptr::null_mut()),
        Err(e) => return e as u32,
    };
    unsafe {
        out_kind.write(kind);
        out_receipt.write(pointer)
    };
    0
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{chunk, identity};
    use crate::{Action, Phase, digest};
    fn descriptor(payload: &[u8]) -> Vec<u8> {
        FrameRef {
            identity: identity(),
            action: ActionRef::Descriptor {
                total_length: payload.len() as u64,
                whole_sha256: digest(payload),
            },
        }
        .encode()
        .unwrap()
    }
    unsafe fn decode(raw: &[u8]) -> *mut CFrame {
        let mut p = std::ptr::null_mut();
        assert_eq!(
            unsafe { mbt_frame_decode(raw.as_ptr(), raw.len() as u32, &mut p) },
            0
        );
        p
    }
    #[test]
    fn opaque_owned_frame_outlives_original_input_and_view_is_small() {
        let mut raw = chunk(1, 0, b"\0\xffmetadata").encode().unwrap();
        let frame = unsafe { decode(&raw) };
        raw.fill(0);
        let mut view = View::blank(identity());
        assert!(std::mem::size_of::<View>() < 512);
        assert!(std::mem::size_of::<CFrame>() < 512);
        assert_eq!(
            unsafe { mbt_frame_view(frame, &mut view, std::mem::size_of::<View>() as u32) },
            0
        );
        assert_eq!(view.payload_length, 10);
        assert_eq!(
            unsafe { bytes(view.payload, view.payload_length) },
            b"\0\xffmetadata"
        );
        assert_eq!(view.sequence, 1);
        unsafe { mbt_frame_free(frame) };
    }
    #[test]
    fn invalid_views_sizes_reserved_and_oversize_preserve_handle_before_payload_read() {
        let mut view = View::blank(identity());
        view.kind = 1;
        view.sequence = 1;
        view.payload = std::ptr::dangling();
        view.payload_length = crate::MAX_CHUNK_BYTES as u32 + 1;
        let sentinel = std::ptr::dangling_mut();
        let mut p = sentinel;
        assert_eq!(
            unsafe { mbt_frame_set(&view, std::mem::size_of::<View>() as u32, &mut p) },
            Error::Limit as u32
        );
        assert_eq!(p, sentinel);
        view.payload_length = 0;
        view.chunk_sha256 = digest(b"");
        view.reserved = 1;
        assert_eq!(
            unsafe { mbt_frame_set(&view, std::mem::size_of::<View>() as u32, &mut p) },
            Error::Invalid as u32
        );
        assert_eq!(p, sentinel);
        view.reserved = 0;
        assert_eq!(
            unsafe { mbt_frame_set(&view, 1, &mut p) },
            Error::Invalid as u32
        );
        assert_eq!(p, sentinel);
    }
    #[test]
    fn set_input_output_slot_alias_is_owned_before_write() {
        let mut view = View::blank(identity());
        view.kind = 1;
        view.sequence = 1;
        view.payload = view.object_ref.as_ptr();
        view.payload_length = 4;
        view.chunk_sha256 = digest(&[2; 4]);
        let p = &mut view as *mut View;
        let slot = p.cast::<*mut CFrame>();
        assert_eq!(
            unsafe { mbt_frame_set(p, std::mem::size_of::<View>() as u32, slot) },
            0
        );
        let handle = unsafe { slot.read() };
        assert_eq!(
            unsafe { &(*handle).value.action },
            &Action::Chunk {
                sequence: 1,
                offset: 0,
                payload: vec![2; 4],
                chunk_sha256: digest(&[2; 4])
            }
        );
        unsafe { mbt_frame_free(handle) };
    }
    #[test]
    fn encode_buffer_and_overlapping_outputs_keep_bytes_length_unchanged() {
        let raw = chunk(1, 0, b"binary").encode().unwrap();
        let frame = unsafe { decode(&raw) };
        let mut out = [0xa5u8; 1024];
        let mut length = 0x76543210;
        assert_eq!(
            unsafe { mbt_frame_encode(frame, out.as_mut_ptr(), 1, &mut length) },
            Error::Buffer as u32
        );
        assert_eq!(length, 0x76543210);
        assert!(out.iter().all(|x| *x == 0xa5));
        let mut aligned = [0xa5a5a5a5u32; 256];
        assert_eq!(
            unsafe {
                mbt_frame_encode(
                    frame,
                    aligned.as_mut_ptr().cast(),
                    1024,
                    aligned.as_mut_ptr(),
                )
            },
            Error::Invalid as u32
        );
        assert!(aligned.iter().all(|x| *x == 0xa5a5a5a5));
        assert_eq!(
            unsafe { mbt_frame_encode(frame, out.as_mut_ptr(), 1024, &mut length) },
            0
        );
        assert_eq!(&out[..length as usize], raw);
        assert!(out[length as usize..].iter().all(|x| *x == 0xa5));
        unsafe { mbt_frame_free(frame) };
    }
    #[test]
    fn opaque_receiver_accept_snapshot_verified_and_receipt_ownership() {
        let mut state = std::ptr::null_mut();
        assert_eq!(
            unsafe { mbt_receiver_new(std::ptr::null(), 0, &mut state) },
            0
        );
        let mut kind = 99;
        let mut receipt = std::ptr::null_mut();
        let initial = descriptor(b"abc");
        assert_eq!(
            unsafe {
                mbt_receiver_accept(
                    state,
                    initial.as_ptr(),
                    initial.len() as u32,
                    &mut kind,
                    &mut receipt,
                )
            },
            0
        );
        assert_eq!(kind, 0);
        assert!(receipt.is_null());
        let raw = chunk(1, 0, b"abc").encode().unwrap();
        assert_eq!(
            unsafe {
                mbt_receiver_accept(
                    state,
                    raw.as_ptr(),
                    raw.len() as u32,
                    &mut kind,
                    &mut receipt,
                )
            },
            0
        );
        assert_eq!(kind, 1);
        assert!(!receipt.is_null());
        let mut v = View::blank(identity());
        assert_eq!(
            unsafe { mbt_frame_view(receipt, &mut v, std::mem::size_of::<View>() as u32) },
            0
        );
        assert_eq!(v.request_digest, digest(&raw));
        unsafe { mbt_frame_free(receipt) };
        let end = FrameRef {
            identity: identity(),
            action: ActionRef::End {
                total_length: 3,
                whole_sha256: digest(b"abc"),
            },
        }
        .encode()
        .unwrap();
        assert_eq!(
            unsafe {
                mbt_receiver_accept(
                    state,
                    end.as_ptr(),
                    end.len() as u32,
                    &mut kind,
                    &mut receipt,
                )
            },
            0
        );
        assert_eq!(kind, 2);
        assert!(receipt.is_null());
        let mut snapshot = std::mem::MaybeUninit::<CSnapshot>::uninit();
        assert_eq!(
            unsafe {
                mbt_receiver_snapshot(
                    state,
                    snapshot.as_mut_ptr(),
                    std::mem::size_of::<CSnapshot>() as u32,
                )
            },
            0
        );
        let snapshot = unsafe { snapshot.assume_init() };
        assert_eq!(snapshot.phase, Phase::VerifiedBytes as u32);
        assert_eq!(snapshot.verified_sha256, digest(b"abc"));
        assert_eq!(snapshot.received_bytes, 3);
        unsafe { mbt_receiver_free(state) };
    }
    #[test]
    fn receiver_semantic_rejection_keeps_output_slots_but_admitted_fee_and_cancel() {
        let mut state = std::ptr::null_mut();
        unsafe { mbt_receiver_new(std::ptr::null(), 0, &mut state) };
        let mut kind = 99;
        let sentinel = std::ptr::dangling_mut();
        let mut receipt = sentinel;
        let raw = [0; 8];
        assert_eq!(
            unsafe { mbt_receiver_accept(state, raw.as_ptr(), 8, &mut kind, &mut receipt) },
            Error::Invalid as u32
        );
        assert_eq!(kind, 99);
        assert_eq!(receipt, sentinel);
        assert_eq!(unsafe { (*state).value.snapshot().requests }, 1);
        assert_eq!(unsafe { (*state).value.snapshot().received_bytes }, 0);
        assert_eq!(unsafe { mbt_receiver_cancel(state) }, 0);
        assert_eq!(
            unsafe { mbt_receiver_accept(state, raw.as_ptr(), 8, &mut kind, &mut receipt) },
            Error::Cancelled as u32
        );
        assert_eq!(kind, 99);
        assert_eq!(receipt, sentinel);
        assert_eq!(unsafe { (*state).value.snapshot().requests }, 1);
        unsafe { mbt_receiver_free(state) };
    }
    #[test]
    fn receiver_invalid_pointer_slots_and_limit_structs_do_not_admit() {
        let mut state = std::ptr::null_mut();
        let mut limits = CLimits::from_limits(Limits::default());
        limits.max_payload_bytes = crate::MAX_OBJECT_BYTES + 1;
        assert_eq!(
            unsafe { mbt_receiver_new(&limits, std::mem::size_of::<CLimits>() as u32, &mut state) },
            Error::Limit as u32
        );
        assert!(state.is_null());
        unsafe { mbt_receiver_new(std::ptr::null(), 0, &mut state) };
        let mut slots = [0usize; 2];
        let raw = descriptor(b"");
        assert_eq!(
            unsafe {
                mbt_receiver_accept(
                    state,
                    raw.as_ptr(),
                    raw.len() as u32,
                    slots.as_mut_ptr().cast(),
                    slots.as_mut_ptr().cast(),
                )
            },
            Error::Invalid as u32
        );
        let mut kind = 99;
        let mut receipt = std::ptr::null_mut();
        assert_eq!(
            unsafe { mbt_receiver_accept(state, state.cast(), 8, &mut kind, &mut receipt) },
            Error::Invalid as u32
        );
        assert_eq!(kind, 99);
        assert!(receipt.is_null());
        assert_eq!(slots, [0; 2]);
        assert_eq!(unsafe { (*state).value.snapshot().requests }, 0);
        unsafe { mbt_receiver_free(state) };
    }
}
