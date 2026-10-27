//! Checked native owners; validity of caller memory is still a caller obligation.
use crate::{
    ActionKind, ActionRef, Client, ClientLimits, ClientPhase, Descriptor, Error, Phase, ReplyRef,
    Request, Response, Result, State, Status,
};
use morrow_blob_transfer_v1::Identity;
use std::{
    mem::{align_of, size_of},
    ptr,
};

pub struct MbuRequest(Request);
pub struct MbuResponse(Response);
pub struct MbuClient(Client);
#[repr(C)]
#[derive(Clone, Copy)]
pub struct RequestView {
    pub abi_version: u32,
    pub struct_size: u32,
    pub action: u32,
    pub reserved: u32,
    pub nomination_ref: [u8; 32],
    pub nonce: [u8; 32],
    pub frame: *const u8,
    pub frame_length: u32,
    pub reserved2: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DescriptorView {
    pub abi_version: u32,
    pub struct_size: u32,
    pub transfer_epoch: [u8; 32],
    pub object_ref: [u8; 32],
    pub operation_id: [u8; 32],
    pub sha256: [u8; 32],
    pub byte_length: u64,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ReplyView {
    pub abi_version: u32,
    pub struct_size: u32,
    pub kind: u32,
    pub status: u32,
    pub phase: u32,
    pub reserved: u32,
    pub descriptor: DescriptorView,
    pub received_bytes: u64,
    pub next_sequence: u64,
    pub receipt: *const u8,
    pub receipt_length: u32,
    pub reserved2: u32,
    pub reserved_pointer: usize,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ClientOptions {
    pub abi_version: u32,
    pub struct_size: u32,
    pub max_calls: u32,
    pub reserved: u32,
    pub max_wire_bytes: u64,
    pub descriptor: DescriptorView,
    pub nomination_ref: [u8; 32],
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Snapshot {
    pub abi_version: u32,
    pub struct_size: u32,
    pub phase: u32,
    pub pending: u32,
    pub calls: u32,
    pub reserved: u32,
    pub wire_bytes: u64,
    pub received_bytes: u64,
    pub next_sequence: u64,
}
pub type Transport =
    unsafe extern "C" fn(*mut std::ffi::c_void, *const u8, u32, *mut u8, u32) -> i32;
type Region = (usize, usize);
fn status(value: Result<()>) -> u32 {
    value.err().map_or(0, |e| e as u32)
}
fn region(address: usize, length: usize) -> Result<Region> {
    Ok((address, address.checked_add(length).ok_or(Error::Buffer)?))
}
fn disjoint(address: usize, length: usize, regions: &[Region]) -> Result<()> {
    let value = region(address, length)?;
    if regions.iter().any(|r| value.0 < r.1 && r.0 < value.1) {
        Err(Error::Buffer)
    } else {
        Ok(())
    }
}
fn aligned<T>(p: *const T) -> Result<()> {
    if p.is_null() || !p.addr().is_multiple_of(align_of::<T>()) {
        return Err(Error::Buffer);
    }
    region(p.addr(), size_of::<T>())?;
    Ok(())
}
fn output<T>(p: *mut T, bytes: u32, regions: &[Region]) -> Result<()> {
    if (bytes as usize) < size_of::<T>() {
        return Err(Error::Buffer);
    }
    aligned(p)?;
    disjoint(p.addr(), size_of::<T>(), regions)
}
fn slot<T>(p: *mut T, regions: &[Region]) -> Result<()> {
    output(p, size_of::<T>() as u32, regions)
}
unsafe fn input<T: Copy>(p: *const T, bytes: u32) -> Result<T> {
    if bytes as usize != size_of::<T>() {
        return Err(Error::Contract);
    }
    aligned(p)?;
    // SAFETY: the caller provides an aligned readable whole input structure.
    Ok(unsafe { ptr::read(p) })
}
unsafe fn bytes<'a>(p: *const u8, length: u32, max: usize) -> Result<&'a [u8]> {
    if length as usize > max {
        return Err(Error::Limit);
    }
    if length == 0 {
        return Ok(&[]);
    }
    if p.is_null() {
        return Err(Error::Buffer);
    }
    region(p.addr(), length as usize)?;
    // SAFETY: caller owns the complete readable input for this invocation.
    Ok(unsafe { std::slice::from_raw_parts(p, length as usize) })
}
fn request_regions(q: &MbuRequest) -> Result<Vec<Region>> {
    Ok(vec![
        region(ptr::from_ref(q).addr(), size_of::<MbuRequest>())?,
        region(q.0.wire.as_ptr().addr(), q.0.wire.capacity())?,
    ])
}
fn response_regions(q: &MbuResponse) -> Result<Vec<Region>> {
    Ok(vec![
        region(ptr::from_ref(q).addr(), size_of::<MbuResponse>())?,
        region(q.0.wire.as_ptr().addr(), q.0.wire.capacity())?,
    ])
}
fn client_regions(c: &MbuClient) -> Result<Vec<Region>> {
    let mut regions = vec![region(ptr::from_ref(c).addr(), size_of::<MbuClient>())?];
    for (address, length) in c.0.owned_regions() {
        regions.push(region(address, length)?);
    }
    Ok(regions)
}
fn descriptor(v: DescriptorView) -> Result<Descriptor> {
    if v.abi_version != 1 || v.struct_size as usize != size_of::<DescriptorView>() {
        return Err(Error::Contract);
    }
    let d = Descriptor {
        identity: Identity {
            transfer_epoch: v.transfer_epoch,
            object_ref: v.object_ref,
            operation_id: v.operation_id,
        },
        byte_length: v.byte_length,
        sha256: v.sha256,
    };
    d.validate()?;
    Ok(d)
}
fn descriptor_view(d: Descriptor) -> DescriptorView {
    DescriptorView {
        abi_version: 1,
        struct_size: size_of::<DescriptorView>() as u32,
        transfer_epoch: d.identity.transfer_epoch,
        object_ref: d.identity.object_ref,
        operation_id: d.identity.operation_id,
        sha256: d.sha256,
        byte_length: d.byte_length,
    }
}
fn zero_descriptor(v: DescriptorView) -> bool {
    v.abi_version == 0
        && v.struct_size == 0
        && v.transfer_epoch == [0; 32]
        && v.object_ref == [0; 32]
        && v.operation_id == [0; 32]
        && v.sha256 == [0; 32]
        && v.byte_length == 0
}
unsafe fn action<'a>(v: RequestView) -> Result<ActionRef<'a>> {
    if v.abi_version != 1 || v.struct_size as usize != size_of::<RequestView>() {
        return Err(Error::Contract);
    }
    if v.reserved != 0 || v.reserved2 != 0 {
        return Err(Error::Invalid);
    }
    let kind = ActionKind::try_from(u16::try_from(v.action).map_err(|_| Error::Invalid)?)?;
    if kind != ActionKind::Push && (!v.frame.is_null() || v.frame_length != 0) {
        return Err(Error::Invalid);
    }
    match kind {
        ActionKind::Open => Ok(ActionRef::Open),
        ActionKind::Inspect => Ok(ActionRef::Inspect),
        ActionKind::Release => Ok(ActionRef::Release),
        // SAFETY: caller keeps its checked bounded frame readable until return.
        ActionKind::Push => Ok(ActionRef::Push(unsafe {
            bytes(v.frame, v.frame_length, crate::MAX_EMBEDDED_FRAME_BYTES)
        }?)),
    }
}
unsafe fn reply<'a>(v: ReplyView) -> Result<ReplyRef<'a>> {
    if v.abi_version != 1 || v.struct_size as usize != size_of::<ReplyView>() {
        return Err(Error::Contract);
    }
    if v.reserved != 0 || v.reserved2 != 0 || v.reserved_pointer != 0 {
        return Err(Error::Invalid);
    }
    match v.kind {
        1 if v.status == 0 && v.receipt.is_null() && v.receipt_length == 0 => {
            Ok(ReplyRef::State(State {
                descriptor: descriptor(v.descriptor)?,
                phase: Phase::try_from(u16::try_from(v.phase).map_err(|_| Error::Invalid)?)?,
                received_bytes: v.received_bytes,
                next_sequence: v.next_sequence,
            }))
        }
        2 | 3
            if zero_descriptor(v.descriptor)
                && v.phase == 0
                && v.received_bytes == 0
                && v.next_sequence == 0 =>
        {
            if v.kind == 2 && v.status == 0 {
                // SAFETY: caller supplies the complete bounded receipt bytes.
                Ok(ReplyRef::ChunkReceipt(unsafe {
                    bytes(
                        v.receipt,
                        v.receipt_length,
                        crate::MAX_EMBEDDED_RECEIPT_BYTES,
                    )
                }?))
            } else if v.kind == 3 && v.receipt.is_null() && v.receipt_length == 0 {
                Ok(ReplyRef::Error(Status::try_from(
                    u16::try_from(v.status).map_err(|_| Error::Invalid)?,
                )?))
            } else {
                Err(Error::Invalid)
            }
        }
        _ => Err(Error::Invalid),
    }
}
fn reply_view(reply: ReplyRef<'_>) -> ReplyView {
    let mut view = ReplyView {
        abi_version: 1,
        struct_size: size_of::<ReplyView>() as u32,
        kind: 0,
        status: 0,
        phase: 0,
        reserved: 0,
        descriptor: DescriptorView {
            abi_version: 0,
            struct_size: 0,
            transfer_epoch: [0; 32],
            object_ref: [0; 32],
            operation_id: [0; 32],
            sha256: [0; 32],
            byte_length: 0,
        },
        received_bytes: 0,
        next_sequence: 0,
        receipt: ptr::null(),
        receipt_length: 0,
        reserved2: 0,
        reserved_pointer: 0,
    };
    match reply {
        ReplyRef::State(s) => {
            view.kind = 1;
            view.phase = s.phase as u32;
            view.descriptor = descriptor_view(s.descriptor);
            view.received_bytes = s.received_bytes;
            view.next_sequence = s.next_sequence;
        }
        ReplyRef::ChunkReceipt(bytes) => {
            view.kind = 2;
            view.receipt = bytes.as_ptr();
            view.receipt_length = bytes.len() as u32;
        }
        ReplyRef::Error(status) => {
            view.kind = 3;
            view.status = status as u32;
        }
    }
    view
}
unsafe fn copy_wire(
    wire: &[u8],
    out: *mut u8,
    capacity: u32,
    length: *mut u32,
    regions: &[Region],
) -> Result<()> {
    if out.is_null() || (capacity as usize) < wire.len() {
        return Err(Error::Buffer);
    }
    slot(length, regions)?;
    disjoint(out.addr(), wire.len(), regions)?;
    disjoint(
        length.addr(),
        size_of::<u32>(),
        &[region(out.addr(), wire.len())?],
    )?;
    // SAFETY: validated disjoint complete writable prefix and length slot.
    unsafe {
        ptr::copy_nonoverlapping(wire.as_ptr(), out, wire.len());
        ptr::write(length, wire.len() as u32);
    }
    Ok(())
}

/// # Safety
/// Input, frame and output slot follow the disjoint live-memory contract in the C header.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbu_request_new(
    v: *const RequestView,
    n: u32,
    out: *mut *mut MbuRequest,
) -> u32 {
    status((|| {
        // SAFETY: caller supplies a complete readable view of the checked size.
        let view = unsafe { input(v, n) }?;
        slot(
            out,
            &[
                region(v.addr(), size_of::<RequestView>())?,
                region(view.frame.addr(), view.frame_length as usize)?,
            ],
        )?;
        // SAFETY: original frame remains readable through construction.
        let request = Request::new(view.nomination_ref, view.nonce, unsafe { action(view) }?)?;
        // SAFETY: checked output slot is disjoint and owns the new allocation.
        unsafe {
            ptr::write(out, Box::into_raw(Box::new(MbuRequest(request))));
        }
        Ok(())
    })())
}
/// # Safety
/// Bytes are readable and the disjoint output slot is writable for one native pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbu_request_decode(
    p: *const u8,
    n: u32,
    out: *mut *mut MbuRequest,
) -> u32 {
    status((|| {
        slot(out, &[region(p.addr(), n as usize)?])?;
        // SAFETY: caller input is readable until decode copies validated bytes.
        let request = Request::decode(unsafe { bytes(p, n, crate::MAX_REQUEST_BYTES) }?)?;
        // SAFETY: validated disjoint output slot.
        unsafe {
            ptr::write(out, Box::into_raw(Box::new(MbuRequest(request))));
        }
        Ok(())
    })())
}
/// # Safety
/// Null or a unique live handle returned by this library; no borrowed views remain in use.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbu_request_free(q: *mut MbuRequest) {
    if !q.is_null() {
        // SAFETY: caller transfers its one owned allocation exactly once.
        unsafe {
            drop(Box::from_raw(q));
        }
    }
}
/// # Safety
/// Request is live; the writable output prefix is disjoint from all request storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbu_request_view_get(
    q: *const MbuRequest,
    out: *mut RequestView,
    n: u32,
) -> u32 {
    status((|| {
        aligned(q)?;
        // SAFETY: caller supplies a live immutable owner.
        let q = unsafe { &*q };
        output(out, n, &request_regions(q)?)?;
        let frame = q.0.action().frame();
        let view = RequestView {
            abi_version: 1,
            struct_size: size_of::<RequestView>() as u32,
            action: q.0.action().kind() as u32,
            reserved: 0,
            nomination_ref: q.0.nomination_ref(),
            nonce: q.0.nonce(),
            frame: if frame.is_empty() {
                ptr::null()
            } else {
                frame.as_ptr()
            },
            frame_length: frame.len() as u32,
            reserved2: 0,
        };
        // SAFETY: checked output borrows only the original owner's immutable wire.
        unsafe {
            ptr::write(out, view);
        }
        Ok(())
    })())
}
/// # Safety
/// Owner, output prefix and length slot obey the C header's disjoint storage contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbu_request_encode(
    q: *const MbuRequest,
    out: *mut u8,
    n: u32,
    length: *mut u32,
) -> u32 {
    status((|| {
        aligned(q)?;
        // SAFETY: immutable owner and caller outputs remain valid for this call.
        let q = unsafe { &*q };
        unsafe { copy_wire(q.0.wire(), out, n, length, &request_regions(q)?) }
    })())
}
/// # Safety
/// Original request, view/receipt bytes and disjoint output slot remain valid for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbu_response_new(
    q: *const MbuRequest,
    v: *const ReplyView,
    n: u32,
    out: *mut *mut MbuResponse,
) -> u32 {
    status((|| {
        aligned(q)?;
        // SAFETY: live original request and readable input view.
        let q = unsafe { &*q };
        let view = unsafe { input(v, n) }?;
        let mut regions = request_regions(q)?;
        regions.extend([
            region(v.addr(), size_of::<ReplyView>())?,
            region(view.receipt.addr(), view.receipt_length as usize)?,
        ]);
        slot(out, &regions)?;
        // SAFETY: borrowed receipt is consumed into the owned response before return.
        let response = Response::new(&q.0, unsafe { reply(view) }?)?;
        // SAFETY: all slot and original-owner overlap checks preceded allocation/publication.
        unsafe {
            ptr::write(out, Box::into_raw(Box::new(MbuResponse(response))));
        }
        Ok(())
    })())
}
/// # Safety
/// Input and original request are live; output is disjoint from both and their owned buffers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbu_response_decode_for(
    p: *const u8,
    n: u32,
    q: *const MbuRequest,
    out: *mut *mut MbuResponse,
) -> u32 {
    status((|| {
        aligned(q)?;
        // SAFETY: caller retains the immutable original request.
        let q = unsafe { &*q };
        let mut regions = request_regions(q)?;
        regions.push(region(p.addr(), n as usize)?);
        slot(out, &regions)?;
        // SAFETY: readable bounded caller bytes, copied only after original correlation checks.
        let response =
            Response::decode_for(unsafe { bytes(p, n, crate::MAX_RESPONSE_BYTES) }?, &q.0)?;
        // SAFETY: checked disjoint pointer slot.
        unsafe {
            ptr::write(out, Box::into_raw(Box::new(MbuResponse(response))));
        }
        Ok(())
    })())
}
/// # Safety
/// Null or one unique live response handle, with no outstanding borrowed-view access.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbu_response_free(q: *mut MbuResponse) {
    if !q.is_null() {
        // SAFETY: caller transfers its allocation once.
        unsafe {
            drop(Box::from_raw(q));
        }
    }
}
/// # Safety
/// Response is live and output prefix is valid, writable and disjoint from its storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbu_response_view_get(
    q: *const MbuResponse,
    out: *mut ReplyView,
    n: u32,
) -> u32 {
    status((|| {
        aligned(q)?;
        // SAFETY: immutable live owner supplied by caller.
        let q = unsafe { &*q };
        output(out, n, &response_regions(q)?)?;
        // SAFETY: checked caller prefix; returned receipt view borrows this response.
        unsafe {
            ptr::write(out, reply_view(q.0.reply()));
        }
        Ok(())
    })())
}
/// # Safety
/// Owner, output prefix and length slot obey the disjoint storage contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbu_response_encode(
    q: *const MbuResponse,
    out: *mut u8,
    n: u32,
    length: *mut u32,
) -> u32 {
    status((|| {
        aligned(q)?;
        // SAFETY: immutable owner and caller outputs remain valid for this call.
        let q = unsafe { &*q };
        unsafe { copy_wire(q.0.wire(), out, n, length, &response_regions(q)?) }
    })())
}
unsafe fn copy_digest(out: *mut u8, n: u32, value: [u8; 32]) -> u32 {
    status((|| {
        if out.is_null() || n < 32 {
            return Err(Error::Buffer);
        }
        region(out.addr(), 32)?;
        // SAFETY: caller provides a writable 32-byte prefix independent of local digest.
        unsafe {
            ptr::copy_nonoverlapping(value.as_ptr(), out, 32);
        }
        Ok(())
    })())
}
/// # Safety
/// Output supplies at least 32 writable bytes when the supplied size is at least 32.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbu_schema_digest(out: *mut u8, n: u32) -> u32 {
    // SAFETY: same output-prefix obligation as the exported function.
    unsafe { copy_digest(out, n, crate::SCHEMA_DIGEST) }
}
/// # Safety
/// Output supplies at least 32 writable bytes when the supplied size is at least 32.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbu_frame_schema_digest(out: *mut u8, n: u32) -> u32 {
    // SAFETY: same output-prefix obligation as the exported function.
    unsafe { copy_digest(out, n, morrow_blob_transfer_v1::SCHEMA_DIGEST) }
}
/// # Safety
/// Options are readable, output is writable and both are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbu_client_new(
    v: *const ClientOptions,
    n: u32,
    out: *mut *mut MbuClient,
) -> u32 {
    status((|| {
        // SAFETY: caller supplies readable whole options of the checked size.
        let v_value = unsafe { input(v, n) }?;
        slot(out, &[region(v.addr(), size_of::<ClientOptions>())?])?;
        if v_value.abi_version != 1 || v_value.struct_size as usize != size_of::<ClientOptions>() {
            return Err(Error::Contract);
        }
        if v_value.reserved != 0 {
            return Err(Error::Invalid);
        }
        let client = Client::new(
            v_value.nomination_ref,
            descriptor(v_value.descriptor)?,
            ClientLimits {
                max_calls: v_value.max_calls,
                max_wire_bytes: v_value.max_wire_bytes,
            },
        )?;
        // SAFETY: checked disjoint slot receives one owning pointer.
        unsafe {
            ptr::write(out, Box::into_raw(Box::new(MbuClient(client))));
        }
        Ok(())
    })())
}
/// # Safety
/// Null or the unique live client handle, not in a callback or another call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbu_client_free(c: *mut MbuClient) {
    if !c.is_null() {
        // SAFETY: caller transfers this complete native helper once.
        unsafe {
            drop(Box::from_raw(c));
        }
    }
}
/// # Safety
/// Client and disjoint output prefix are valid; no concurrent mutation of the client.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbu_client_snapshot(
    c: *const MbuClient,
    out: *mut Snapshot,
    n: u32,
) -> u32 {
    status((|| {
        aligned(c)?;
        // SAFETY: immutable live client provided by the caller.
        let c = unsafe { &*c };
        output(out, n, &client_regions(c)?)?;
        let s = c.0.snapshot();
        let phase = match s.phase {
            ClientPhase::Fresh => 0,
            ClientPhase::Ready => 1,
            ClientPhase::Receiving => 2,
            ClientPhase::Staged => 3,
            ClientPhase::Released => 4,
            ClientPhase::Observed => 5,
            ClientPhase::Rejected => 6,
            ClientPhase::Closed => 7,
            ClientPhase::Unknown => 8,
        };
        let view = Snapshot {
            abi_version: 1,
            struct_size: size_of::<Snapshot>() as u32,
            phase,
            pending: u32::from(s.pending),
            calls: s.calls,
            reserved: 0,
            wire_bytes: s.wire_bytes,
            received_bytes: s.received_bytes,
            next_sequence: s.next_sequence,
        };
        // SAFETY: checked disjoint output, with every published field initialized.
        unsafe {
            ptr::write(out, view);
        }
        Ok(())
    })())
}
unsafe fn client_input(
    c: *mut MbuClient,
    v: *const RequestView,
    n: u32,
    out: usize,
    out_size: usize,
) -> Result<RequestView> {
    aligned(c)?;
    // SAFETY: the caller supplies a live client, observed immutably before mutation.
    let regions = client_regions(unsafe { &*c })?;
    disjoint(v.addr(), size_of::<RequestView>(), &regions)?;
    // SAFETY: exact-sized readable view copied before client access becomes mutable.
    let view = unsafe { input(v, n) }?;
    disjoint(view.frame.addr(), view.frame_length as usize, &regions)?;
    disjoint(out, out_size, &regions)?;
    disjoint(
        out,
        out_size,
        &[
            region(v.addr(), size_of::<RequestView>())?,
            region(view.frame.addr(), view.frame_length as usize)?,
        ],
    )?;
    // SAFETY: only immutable client metadata is read here.
    if unsafe { &*c }.0.nomination_ref() != view.nomination_ref {
        return Err(Error::Correlation);
    }
    Ok(view)
}
/// # Safety
/// Client is exclusively owned; view/frame/output do not alias any client-owned allocation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbu_client_begin(
    c: *mut MbuClient,
    v: *const RequestView,
    n: u32,
    out: *mut *mut MbuRequest,
) -> u32 {
    status((|| {
        slot(out, &[])?;
        // SAFETY: checked view, frame and output are independent of the original client.
        let view = unsafe { client_input(c, v, n, out.addr(), size_of::<*mut MbuRequest>()) }?;
        // SAFETY: exclusive client; decoded frame view remains readable through begin.
        let request = unsafe { &mut *c }
            .0
            .begin(view.nonce, unsafe { action(view) }?)?
            .clone();
        // SAFETY: all pointer validation preceded accounting and publication.
        unsafe {
            ptr::write(out, Box::into_raw(Box::new(MbuRequest(request))));
        }
        Ok(())
    })())
}
/// # Safety
/// Exclusive client, independent readable response, and disjoint writable pointer slot.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbu_client_accept(
    c: *mut MbuClient,
    p: *const u8,
    n: u32,
    out: *mut *mut MbuResponse,
) -> u32 {
    status((|| {
        aligned(c)?;
        // SAFETY: immutable observation precedes taking exclusive client access.
        let mut regions = client_regions(unsafe { &*c })?;
        disjoint(p.addr(), n as usize, &regions)?;
        regions.push(region(p.addr(), n as usize)?);
        slot(out, &regions)?;
        if n as usize > crate::MAX_RESPONSE_BYTES {
            // SAFETY: the exact exclusive client is live; no oversized input is read.
            unsafe { &mut *c }.0.transport_unknown()?;
            return Err(Error::Limit);
        }
        // SAFETY: independent readable bytes remain valid while the exclusive client processes them.
        let response = unsafe { &mut *c }
            .0
            .accept(unsafe { bytes(p, n, crate::MAX_RESPONSE_BYTES) }?)?;
        // SAFETY: checked disjoint pointer slot.
        unsafe {
            ptr::write(out, Box::into_raw(Box::new(MbuResponse(response))));
        }
        Ok(())
    })())
}
/// # Safety
/// Client is a live unique handle with no callback or concurrent access in progress.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbu_client_transport_unknown(c: *mut MbuClient) -> u32 {
    status((|| {
        aligned(c)?;
        // SAFETY: caller supplies exclusive live client ownership.
        unsafe { &mut *c }.0.transport_unknown()
    })())
}
/// # Safety
/// Client is a live unique handle with no callback or concurrent access in progress.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbu_client_abandon(c: *mut MbuClient) -> u32 {
    status((|| {
        aligned(c)?;
        // SAFETY: caller supplies exclusive live client ownership.
        unsafe { &mut *c }.0.abandon();
        Ok(())
    })())
}
/// # Safety
/// Client/view/output obey begin's contract. Callback/context are live, synchronous,
/// non-reentrant, cannot retain buffers, and cannot throw/unwind across the C ABI.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbu_client_call_once(
    c: *mut MbuClient,
    v: *const RequestView,
    n: u32,
    call: Option<Transport>,
    context: *mut std::ffi::c_void,
    out: *mut *mut MbuResponse,
) -> u32 {
    status((|| {
        let call = call.ok_or(Error::Buffer)?;
        slot(out, &[])?;
        // SAFETY: checked view/frame and output are disjoint from the exclusive client.
        let view = unsafe { client_input(c, v, n, out.addr(), size_of::<*mut MbuResponse>()) }?;
        // SAFETY: original input data and exclusive client remain live; callback obeys its contract.
        let response = unsafe { &mut *c }.0.call_once_with(
            view.nonce,
            unsafe { action(view) }?,
            |input, output| {
                // SAFETY: caller's non-reentrant callback receives only current borrowed buffers.
                unsafe {
                    call(
                        context,
                        input.as_ptr(),
                        input.len() as u32,
                        output.as_mut_ptr(),
                        output.len() as u32,
                    )
                }
            },
        )?;
        // SAFETY: checked disjoint output slot, after a single successful callback/validation.
        unsafe {
            ptr::write(out, Box::into_raw(Box::new(MbuResponse(response))));
        }
        Ok(())
    })())
}
