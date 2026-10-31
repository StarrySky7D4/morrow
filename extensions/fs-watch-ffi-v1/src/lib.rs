//! Owning C ABI for the independent observation codec, never native authority.
//! Callers supply valid aligned memory and live, exclusively used SDK owners.
//! Numeric checks cannot establish memory validity or recover process-wide OOM.
#![deny(unsafe_op_in_unsafe_fn)]
use morrow_fs_watch_v1::{
    Batch, Error, MAX_WIRE_BYTES, SCHEMA_DIGEST,
    state::{ClientState, Limits, Phase},
    wire_digest,
};
use std::{
    mem::{align_of, size_of},
    ptr, slice,
};

pub const ABI_VERSION: u32 = 1;
const INVALID: u32 = 1;
const BUFFER: u32 = 10;
type AbiResult<T> = std::result::Result<T, u32>;
fn error(value: Error) -> u32 {
    match value {
        Error::Invalid => 1,
        Error::Contract => 2,
        Error::Limit => 3,
        Error::Allocation => 4,
        Error::Correlation => 5,
        Error::Pending => 6,
        Error::State => 7,
        Error::Budget => 8,
        Error::OutcomeUnknown => 9,
    }
}
fn status(value: AbiResult<()>) -> u32 {
    value.err().unwrap_or(0)
}
fn region(p: *const u8, n: usize) -> AbiResult<(usize, usize)> {
    if n != 0 && p.is_null() {
        return Err(INVALID);
    }
    let start = p as usize;
    let end = start.checked_add(n).ok_or(INVALID)?;
    Ok((start, end))
}
fn overlap(a: (usize, usize), b: (usize, usize)) -> bool {
    a.0 < a.1 && b.0 < b.1 && a.0 < b.1 && b.0 < a.1
}
fn checked<T>(p: *const T) -> AbiResult<()> {
    region(p.cast(), size_of::<T>())?;
    if p.is_null() || !(p as usize).is_multiple_of(align_of::<T>()) {
        return Err(INVALID);
    }
    Ok(())
}
fn disjoint_state(p: *const Client, other: *const u8, n: usize) -> AbiResult<()> {
    if overlap(region(p.cast(), size_of::<Client>())?, region(other, n)?) {
        Err(INVALID)
    } else {
        Ok(())
    }
}
unsafe fn bytes<'a>(p: *const u8, n: u32, max: usize) -> AbiResult<&'a [u8]> {
    if n as usize > max {
        return Err(3);
    }
    region(p, n as usize)?;
    if n == 0 {
        Ok(&[])
    } else {
        Ok(unsafe { slice::from_raw_parts(p, n as usize) })
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct FrameInfo {
    pub abi_version: u32,
    pub struct_size: u32,
    pub epoch: [u8; 32],
    pub sequence: u64,
    pub changes: u32,
    pub end: u32,
    pub reserved: u64,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ChangeView {
    pub kind: u32,
    pub reserved: u32,
    pub name: *const u8,
    pub name_bytes: u32,
    pub reserved2: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct StateLimits {
    pub abi_version: u32,
    pub struct_size: u32,
    pub frames: u32,
    pub changes: u32,
    pub wire_bytes: u64,
    pub reserved: u64,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct StateSnapshot {
    pub abi_version: u32,
    pub struct_size: u32,
    pub wire_bytes: u64,
    pub frames: u32,
    pub changes: u32,
    pub phase: u32,
    pub end: u32,
    pub reserved: u64,
}
pub struct Frame {
    value: Batch,
}
pub struct Client {
    value: ClientState,
}
impl Frame {
    fn disjoint(&self, p: *const u8, n: usize) -> AbiResult<()> {
        let r = region(p, n)?;
        if overlap(r, region(ptr::from_ref(self).cast(), size_of::<Self>())?)
            || overlap(
                r,
                region(
                    self.value.changes.as_ptr().cast(),
                    self.value.changes.len() * size_of::<morrow_fs_watch_v1::Change>(),
                )?,
            )
        {
            return Err(INVALID);
        }
        for c in &self.value.changes {
            if overlap(r, region(c.name_utf16_le.as_ptr(), c.name_utf16_le.len())?) {
                return Err(INVALID);
            }
        }
        Ok(())
    }
}
unsafe fn frame<'a>(p: *const Frame) -> AbiResult<&'a Frame> {
    checked(p)?;
    Ok(unsafe { &*p })
}

/// # Safety
/// Input is readable for length bytes; output is a valid aligned pointer slot.
/// Success transfers a new owner; it never frees an old value in the slot.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mfw_frame_v1_decode(
    input: *const u8,
    length: u32,
    output: *mut *mut Frame,
) -> u32 {
    status((|| {
        checked(output)?;
        let value =
            Batch::decode(unsafe { bytes(input, length, MAX_WIRE_BYTES)? }).map_err(error)?;
        let owned = Box::into_raw(Box::new(Frame { value }));
        unsafe { ptr::write(output, owned) };
        Ok(())
    })())
}

/// # Safety
/// Owner is live; outputs are writable and do not alias any SDK-owned storage.
/// The encoded prefix and output-length slot must be disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mfw_frame_v1_encode(
    owner: *const Frame,
    output: *mut u8,
    capacity: u32,
    written: *mut u32,
) -> u32 {
    status((|| {
        checked(written)?;
        let f = unsafe { frame(owner)? };
        let wire = f.value.encode().map_err(error)?;
        if wire.len() > capacity as usize {
            return Err(BUFFER);
        }
        let out = region(output, wire.len())?;
        if overlap(out, region(written.cast(), size_of::<u32>())?) {
            return Err(INVALID);
        }
        f.disjoint(output, wire.len())?;
        f.disjoint(written.cast(), size_of::<u32>())?;
        unsafe {
            ptr::copy_nonoverlapping(wire.as_ptr(), output, wire.len());
            ptr::write(written, wire.len() as u32);
        }
        Ok(())
    })())
}

/// # Safety
/// Owner is live; output is writable, aligned, and disjoint from its storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mfw_frame_v1_info(
    owner: *const Frame,
    output: *mut FrameInfo,
    size: u32,
) -> u32 {
    status((|| {
        if size as usize != size_of::<FrameInfo>() {
            return Err(INVALID);
        }
        checked(output)?;
        let f = unsafe { frame(owner)? };
        f.disjoint(output.cast(), size as usize)?;
        let info = FrameInfo {
            abi_version: ABI_VERSION,
            struct_size: size,
            epoch: f.value.watch_epoch,
            sequence: f.value.sequence,
            changes: f.value.changes.len() as u32,
            end: f.value.end as u32,
            reserved: 0,
        };
        unsafe { ptr::write(output, info) };
        Ok(())
    })())
}

/// # Safety
/// Owner is live; output is valid and disjoint. Returned name bytes borrow the
/// owner until its one free and must never be mutated by the caller.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mfw_frame_v1_change(
    owner: *const Frame,
    index: u32,
    output: *mut ChangeView,
    size: u32,
) -> u32 {
    status((|| {
        if size as usize != size_of::<ChangeView>() {
            return Err(INVALID);
        }
        checked(output)?;
        let f = unsafe { frame(owner)? };
        f.disjoint(output.cast(), size as usize)?;
        let change = f.value.changes.get(index as usize).ok_or(INVALID)?;
        let view = ChangeView {
            kind: change.kind as u32,
            reserved: 0,
            name: change.name_utf16_le.as_ptr(),
            name_bytes: change.name_utf16_le.len() as u32,
            reserved2: 0,
        };
        unsafe { ptr::write(output, view) };
        Ok(())
    })())
}

/// # Safety
/// NULL or a live owner created by this SDK, freed exactly once with no borrows.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mfw_frame_v1_free(owner: *mut Frame) {
    if !owner.is_null() {
        drop(unsafe { Box::from_raw(owner) });
    }
}

/// # Safety
/// Epoch and limits are readable and aligned; output is a valid pointer slot.
/// This creates consumer bookkeeping only, never a host grant or new credit.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mfw_state_v1_new(
    epoch: *const u8,
    limits: *const StateLimits,
    size: u32,
    output: *mut *mut Client,
) -> u32 {
    status((|| {
        if size as usize != size_of::<StateLimits>() {
            return Err(INVALID);
        }
        checked(limits)?;
        checked(output)?;
        let request = unsafe { ptr::read(limits) };
        if request.abi_version != ABI_VERSION
            || request.struct_size != size
            || request.reserved != 0
        {
            return Err(INVALID);
        }
        let epoch: [u8; 32] = unsafe { bytes(epoch, 32, 32)? }
            .try_into()
            .map_err(|_| INVALID)?;
        let value = ClientState::new(
            epoch,
            Limits {
                frames: request.frames,
                changes: request.changes,
                wire_bytes: request.wire_bytes,
            },
        )
        .map_err(error)?;
        let owner = Box::into_raw(Box::new(Client { value }));
        unsafe { ptr::write(output, owner) };
        Ok(())
    })())
}

/// # Safety
/// Client is live and exclusive; wire/output are valid and disjoint from it.
/// The output slot may alias input wire because the entire frame is copied first.
/// An error leaves the output slot intact, but may latch Unknown in the client.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mfw_state_v1_receive(
    owner: *mut Client,
    input: *const u8,
    length: u32,
    output: *mut *mut Frame,
) -> u32 {
    status((|| {
        checked(owner)?;
        checked(output)?;
        disjoint_state(owner, input, length as usize)?;
        disjoint_state(owner, output.cast(), size_of::<*mut Frame>())?;
        // Check pointer arithmetic without reading excessive input. An oversized
        // wire follows the same state transition as an invalid serialized size:
        // Ready becomes Unknown; Pending/Ended/Unknown keep their own refusal.
        region(input, length as usize)?;
        let wire = if length as usize > MAX_WIRE_BYTES {
            &[][..]
        } else {
            unsafe { bytes(input, length, MAX_WIRE_BYTES)? }
        };
        let state = unsafe { &mut *owner };
        let value = state.value.receive(wire).map_err(error)?;
        let frame = Box::into_raw(Box::new(Frame { value }));
        unsafe { ptr::write(output, frame) };
        Ok(())
    })())
}

/// # Safety
/// Client is live and exclusive; digest is readable for 32 bytes and disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mfw_state_v1_ack(
    owner: *mut Client,
    sequence: u64,
    digest: *const u8,
) -> u32 {
    status((|| {
        checked(owner)?;
        disjoint_state(owner, digest, 32)?;
        let exact: [u8; 32] = unsafe { bytes(digest, 32, 32)? }
            .try_into()
            .map_err(|_| INVALID)?;
        unsafe { &mut *owner }
            .value
            .acknowledge(sequence, exact)
            .map_err(error)
    })())
}

/// # Safety
/// Client is live; output is writable, aligned and disjoint from its allocation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mfw_state_v1_snapshot(
    owner: *const Client,
    output: *mut StateSnapshot,
    size: u32,
) -> u32 {
    status((|| {
        if size as usize != size_of::<StateSnapshot>() {
            return Err(INVALID);
        }
        checked(owner)?;
        checked(output)?;
        disjoint_state(owner, output.cast(), size as usize)?;
        let state = unsafe { &*owner };
        let used = state.value.usage();
        let (phase, end) = match state.value.phase() {
            Phase::Ready => (0, 0),
            Phase::Pending => (1, 0),
            Phase::Ended(end) => (2, end as u32),
            Phase::Unknown => (3, 0),
        };
        unsafe {
            ptr::write(
                output,
                StateSnapshot {
                    abi_version: ABI_VERSION,
                    struct_size: size,
                    wire_bytes: used.wire_bytes,
                    frames: used.frames,
                    changes: used.changes,
                    phase,
                    end,
                    reserved: 0,
                },
            )
        };
        Ok(())
    })())
}

/// # Safety
/// Client is a live exclusive SDK owner. This never contacts a native producer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mfw_state_v1_transport_lost(owner: *mut Client) -> u32 {
    status((|| {
        checked(owner)?;
        unsafe { &mut *owner }.value.transport_lost();
        Ok(())
    })())
}

/// # Safety
/// NULL or a live SDK client freed exactly once. This frees local bookkeeping
/// only; it does not retire/refund a host watch, native resources, or its budget.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mfw_state_v1_free(owner: *mut Client) {
    if !owner.is_null() {
        drop(unsafe { Box::from_raw(owner) });
    }
}

/// # Safety
/// Input is readable and output is writable for exactly 32 bytes. Inputs are
/// consumed before writing, so these byte regions may overlap.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mfw_wire_v1_digest(
    input: *const u8,
    length: u32,
    output: *mut u8,
    output_length: u32,
) -> u32 {
    status((|| {
        if output_length != 32 {
            return Err(INVALID);
        }
        region(output, 32)?;
        let digest = wire_digest(unsafe { bytes(input, length, MAX_WIRE_BYTES)? });
        unsafe { ptr::copy_nonoverlapping(digest.as_ptr(), output, 32) };
        Ok(())
    })())
}

/// # Safety
/// Output is writable for exactly 32 bytes and does not overlap any live owner.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mfw_schema_v1_digest(output: *mut u8, length: u32) -> u32 {
    status((|| {
        if length != 32 {
            return Err(INVALID);
        }
        region(output, 32)?;
        unsafe { ptr::copy_nonoverlapping(SCHEMA_DIGEST.as_ptr(), output, 32) };
        Ok(())
    })())
}
