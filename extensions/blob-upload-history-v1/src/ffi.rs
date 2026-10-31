//! Local C adapter. Valid readable/writable memory and live opaque owners remain
//! caller obligations. These pointers are never an IPC or guest authority.
use crate::{Error, Metadata, Outcome, Packet, Result};
use std::{
    mem::{align_of, size_of},
    ptr,
};

pub struct MbhPacket(Packet);
#[repr(C)]
#[derive(Clone, Copy)]
pub struct TextView {
    pub data: *const u8,
    pub bytes: u32,
    pub reserved: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct View {
    pub abi_version: u32,
    pub struct_size: u32,
    pub query_nonce: [u8; 32],
    pub submission: [u8; 32],
    pub package_digest: [u8; 32],
    pub whole_sha256: [u8; 32],
    pub input_sha256: [u8; 32],
    pub request_binding_sha256: [u8; 32],
    pub package_id: TextView,
    pub operation_id: TextView,
    pub handler: TextView,
    pub registry_revision: u64,
    pub byte_length: u64,
    pub reserved_at: i64,
    pub observed_at: i64,
    pub acknowledged_at: i64,
    pub wire_bytes: u64,
    pub native_work_bytes: u64,
    pub revision: u32,
    pub current_stage: u32,
    pub original_unknown: u32,
    pub outcome: u32,
    pub error_code: u32,
    pub calls: u32,
    pub execution: u32,
    pub exit_code: i32,
    pub execution_error: u32,
    pub host_calls: u32,
}
fn text(s: &str) -> TextView {
    TextView {
        data: s.as_ptr(),
        bytes: s.len() as u32,
        reserved: 0,
    }
}
type Region = (usize, usize);
fn region(p: usize, len: usize) -> Result<Region> {
    Ok((p, p.checked_add(len).ok_or(Error::Buffer)?))
}
fn aligned<T>(p: *const T) -> Result<()> {
    if p.is_null() || !p.addr().is_multiple_of(align_of::<T>()) {
        return Err(Error::Buffer);
    }
    region(p.addr(), size_of::<T>())?;
    Ok(())
}
fn disjoint(p: usize, len: usize, inputs: &[Region]) -> Result<()> {
    let output = region(p, len)?;
    if inputs.iter().any(|r| output.0 < r.1 && r.0 < output.1) {
        Err(Error::Buffer)
    } else {
        Ok(())
    }
}
fn status(r: Result<()>) -> u32 {
    r.err().map_or(0, |e| e as u32)
}

/// Encode the canonical task ID without a trailing NUL. Both output slots are
/// preserved on failure. The ID correlates a selection; it is not an approval.
///
/// # Safety
/// Each input addresses 32 readable bytes. Output addresses `capacity` writable
/// bytes, and `written` addresses an aligned writable u32. The output slots are
/// mutually disjoint and disjoint from the inputs for the entire call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbh_task_id_write(
    submission: *const u8,
    nonce: *const u8,
    out: *mut u8,
    capacity: u32,
    written: *mut u32,
) -> u32 {
    status((|| {
        aligned(submission)?;
        aligned(nonce)?;
        aligned(out)?;
        aligned(written)?;
        if (capacity as usize) < crate::TASK_ID_BYTES {
            return Err(Error::Buffer);
        }
        let inputs = [region(submission.addr(), 32)?, region(nonce.addr(), 32)?];
        let output = region(out.addr(), crate::TASK_ID_BYTES)?;
        disjoint(out.addr(), crate::TASK_ID_BYTES, &inputs)?;
        disjoint(
            written.addr(),
            size_of::<u32>(),
            &[inputs[0], inputs[1], output],
        )?;
        let submission = unsafe { std::slice::from_raw_parts(submission, 32) }
            .try_into()
            .map_err(|_| Error::Buffer)?;
        let nonce = unsafe { std::slice::from_raw_parts(nonce, 32) }
            .try_into()
            .map_err(|_| Error::Buffer)?;
        let value = crate::task_id(submission, nonce)?;
        unsafe {
            ptr::copy_nonoverlapping(value.as_ptr(), out, value.len());
            ptr::write(written, value.len() as u32);
        }
        Ok(())
    })())
}

/// Decode against the task envelope's canonical ID. A trusted host must have
/// separately approved the selected record and recipient before dispatch.
///
/// # Safety
/// Inputs address their advertised readable lengths. The aligned output slot
/// is exclusively writable and disjoint from both inputs. Failure preserves it.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbh_packet_decode_task(
    bytes: *const u8,
    length: u32,
    task_id: *const u8,
    task_id_length: u32,
    out: *mut *mut MbhPacket,
) -> u32 {
    status((|| {
        aligned(bytes)?;
        aligned(task_id)?;
        aligned(out)?;
        if length == 0 || length as usize > crate::MAX_PACKET_BYTES {
            return Err(Error::Limit);
        }
        if task_id_length as usize != crate::TASK_ID_BYTES {
            return Err(Error::Correlation);
        }
        let inputs = [
            region(bytes.addr(), length as usize)?,
            region(task_id.addr(), task_id_length as usize)?,
        ];
        disjoint(out.addr(), size_of::<*mut MbhPacket>(), &inputs)?;
        let wire = unsafe { std::slice::from_raw_parts(bytes, length as usize) };
        let task = unsafe { std::slice::from_raw_parts(task_id, task_id_length as usize) };
        let task = std::str::from_utf8(task).map_err(|_| Error::Correlation)?;
        let value = Packet::decode_task(wire, task)?;
        unsafe {
            ptr::write(out, Box::into_raw(Box::new(MbhPacket(value))));
        }
        Ok(())
    })())
}

/// Input memory must remain readable for the call. The output slot is unchanged
/// on every reported failure, and ownership transfers only on zero status.
///
/// # Safety
/// Inputs must address the stated readable lengths. `out` must be writable and
/// aligned, disjoint from all inputs, and exclusively accessed during the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbh_packet_decode(
    bytes: *const u8,
    length: u32,
    submission: *const u8,
    nonce: *const u8,
    out: *mut *mut MbhPacket,
) -> u32 {
    status((|| {
        aligned(bytes)?;
        aligned(submission)?;
        aligned(nonce)?;
        aligned(out)?;
        if length == 0 || length as usize > crate::MAX_PACKET_BYTES {
            return Err(Error::Limit);
        }
        let inputs = [
            region(bytes.addr(), length as usize)?,
            region(submission.addr(), 32)?,
            region(nonce.addr(), 32)?,
        ];
        disjoint(out.addr(), size_of::<*mut MbhPacket>(), &inputs)?;
        let wire = unsafe { std::slice::from_raw_parts(bytes, length as usize) };
        let submission = unsafe { std::slice::from_raw_parts(submission, 32) }
            .try_into()
            .map_err(|_| Error::Buffer)?;
        let nonce = unsafe { std::slice::from_raw_parts(nonce, 32) }
            .try_into()
            .map_err(|_| Error::Buffer)?;
        let packet = Packet::decode_for(wire, submission, nonce)?;
        unsafe {
            ptr::write(out, Box::into_raw(Box::new(MbhPacket(packet))));
        }
        Ok(())
    })())
}

/// Views borrow the packet's immutable strings until that same packet is freed.
///
/// # Safety
/// `packet` must be a live owner from this API. `out` must address an aligned,
/// writable `View`, disjoint from the owner and its borrowed storage. Neither
/// owner nor output may be mutated or freed concurrently.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbh_packet_view(
    packet: *const MbhPacket,
    out: *mut View,
    bytes: u32,
) -> u32 {
    status((|| {
        aligned(packet)?;
        aligned(out)?;
        if (bytes as usize) < size_of::<View>() {
            return Err(Error::Buffer);
        }
        let value = unsafe { &(*packet).0 };
        let r = value.record();
        let inputs = [
            region(packet.addr(), size_of::<MbhPacket>())?,
            region(r.package_id.as_ptr().addr(), r.package_id.capacity())?,
            region(r.operation_id.as_ptr().addr(), r.operation_id.capacity())?,
            region(r.handler.as_ptr().addr(), r.handler.capacity())?,
        ];
        disjoint(out.addr(), size_of::<View>(), &inputs)?;
        let mut v = View {
            abi_version: 1,
            struct_size: size_of::<View>() as u32,
            query_nonce: value.nonce(),
            submission: r.submission,
            package_digest: r.package_digest,
            whole_sha256: r.whole_sha256,
            input_sha256: r.input_sha256,
            request_binding_sha256: r.request_binding_sha256,
            package_id: text(&r.package_id),
            operation_id: text(&r.operation_id),
            handler: text(&r.handler),
            registry_revision: r.registry_revision,
            byte_length: r.byte_length,
            reserved_at: r.reserved_at,
            observed_at: 0,
            acknowledged_at: 0,
            wire_bytes: 0,
            native_work_bytes: 0,
            revision: r.revision() as u32,
            current_stage: r.current_stage as u32,
            original_unknown: u32::from(r.original_unknown()),
            outcome: 0,
            error_code: 0,
            calls: 0,
            execution: 0,
            exit_code: 0,
            execution_error: 0,
            host_calls: 0,
        };
        if let Some(original) = r.original() {
            v.outcome = match original.outcome {
                Outcome::Absent => 1,
                Outcome::Staged => 2,
                Outcome::Released => 3,
                Outcome::Error(code) => {
                    v.error_code = code as u32;
                    4
                }
            };
            v.calls = original.calls;
            v.wire_bytes = original.wire_bytes;
            v.native_work_bytes = original.native_work_bytes;
            if let Some(e) = original.execution {
                v.host_calls = e.host_calls;
                match e.outcome {
                    Ok(code) => {
                        v.execution = 1;
                        v.exit_code = code;
                    }
                    Err(code) => {
                        v.execution = 2;
                        v.execution_error = code as u32;
                    }
                }
            }
        }
        match r.metadata {
            Metadata::Reserved => (),
            Metadata::Observed { at, .. } => v.observed_at = at,
            Metadata::Acknowledged {
                observed_at, at, ..
            } => {
                v.observed_at = observed_at;
                v.acknowledged_at = at;
            }
        }
        unsafe {
            ptr::write(out, v);
        }
        Ok(())
    })())
}

/// Null is a no-op; every other pointer must be this API's live unique owner.
///
/// # Safety
/// The owner must not have been freed or be in use by another call. Freeing it
/// invalidates every view; it must never be passed to this function again.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbh_packet_free(packet: *mut MbhPacket) {
    if !packet.is_null() {
        unsafe {
            drop(Box::from_raw(packet));
        }
    }
}
