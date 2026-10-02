//! Independent local C channel codec. Handles own all returned spans.
use crate::channel::{self, Action, Request, Response};
use sha2::{Digest, Sha256};
use std::{ffi::c_void, panic::catch_unwind};
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Span {
    data: *const u8,
    length: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CRequest {
    abi_version: u32,
    struct_size: u32,
    kind: u32,
    call_id: Span,
    reference: Span,
    source_epoch: Span,
    sequence: u64,
    credit_bytes: u32,
    frame_sha256: Span,
    cursor: Span,
    bytes: Span,
}
#[repr(C)]
#[derive(Default)]
pub struct CView {
    status: u32,
    has_frame: u32,
    resource_reclaimed: u32,
    sequence: u64,
    last_acked: u64,
    accepted_sequence: u64,
    call_id: Span,
    reference: Span,
    source_epoch: Span,
    bytes: Span,
    cursor: Span,
    frame_sha256: Span,
    encoded_frame: Span,
}
struct Handle {
    response: Response,
    encoded: Vec<u8>,
    frame_sha256: [u8; 32],
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CBudget {
    max_channels: u32,
    max_frame_bytes: u32,
    max_bytes: u64,
    max_messages: u64,
    max_requests: u64,
    max_duration_ms: u64,
}
#[repr(C)]
pub struct CEndpoint {
    reference: Span,
    source_epoch: Span,
    kind: u32,
    budget: CBudget,
}
#[repr(C)]
#[derive(Default)]
pub struct CDirectoryView {
    scope_sha256: Span,
    channels: *const CEndpoint,
    channel_count: u32,
}
struct DirectoryHandle {
    directory: channel::Directory,
    views: Vec<CEndpoint>,
}
fn span(bytes: &[u8]) -> Span {
    Span {
        data: bytes.as_ptr(),
        length: bytes.len() as u32,
    }
}
fn guard(f: impl FnOnce() -> Result<(), u32> + std::panic::UnwindSafe) -> u32 {
    match catch_unwind(f) {
        Ok(Ok(())) => 0,
        Ok(Err(code)) => code,
        Err(_) => 20,
    }
}
fn code(error: channel::Error) -> u32 {
    match error {
        channel::Error::Invalid => 16,
        channel::Error::Limit => 17,
        channel::Error::Contract => 18,
        channel::Error::Correlation => 19,
    }
}
unsafe fn bytes(raw: Span, max: usize) -> Result<Vec<u8>, u32> {
    if raw.length as usize > max {
        return Err(17);
    }
    if raw.length == 0 {
        return Ok(Vec::new());
    }
    if raw.data.is_null() {
        return Err(16);
    }
    let mut out = Vec::new();
    out.try_reserve_exact(raw.length as usize)
        .map_err(|_| 4u32)?;
    out.extend_from_slice(unsafe { std::slice::from_raw_parts(raw.data, raw.length as usize) });
    Ok(out)
}
unsafe fn fixed(raw: Span) -> Result<[u8; 32], u32> {
    if raw.length != 32 {
        return Err(16);
    }
    unsafe { bytes(raw, 32) }?.try_into().map_err(|_| 16)
}
unsafe fn request(raw: *const CRequest) -> Result<Request, u32> {
    if raw.is_null() {
        return Err(16);
    }
    if !unsafe { crate::descriptor_prefix::matches(raw) } {
        return Err(18);
    }
    let r = unsafe { raw.read() };
    let action = match r.kind {
        1 => Action::Receive {
            last_acked: r.sequence,
            credit_bytes: r.credit_bytes,
        },
        2 => Action::Ack {
            sequence: r.sequence,
            frame_sha256: unsafe { fixed(r.frame_sha256) }?,
            cursor: unsafe { bytes(r.cursor, channel::MAX_CURSOR_BYTES) }?,
        },
        3 => Action::Send {
            sequence: r.sequence,
            bytes: unsafe { bytes(r.bytes, channel::MAX_PAYLOAD_BYTES) }?,
        },
        4 => Action::Close,
        5 => Action::Query,
        _ => return Err(16),
    };
    Ok(Request {
        call_id: unsafe { fixed(r.call_id) }?,
        reference: unsafe { fixed(r.reference) }?,
        source_epoch: unsafe { fixed(r.source_epoch) }?,
        action,
    })
}
/// # Safety
/// Descriptor and spans are readable; output/control slots are aligned live and
/// disjoint. Descriptor and all input spans are copied before output is written.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp_channel_request_encode(
    raw: *const CRequest,
    out: *mut u8,
    capacity: u32,
    length: *mut u32,
) -> u32 {
    if length.is_null() {
        return 16;
    }
    unsafe { *length = 0 };
    guard(|| {
        if out.is_null() {
            return Err(16);
        }
        let encoded = unsafe { request(raw) }?.encode().map_err(code)?;
        if encoded.len() > capacity as usize {
            return Err(17);
        }
        unsafe {
            std::ptr::copy_nonoverlapping(encoded.as_ptr(), out, encoded.len());
            *length = encoded.len() as u32
        };
        Ok(())
    })
}
/// # Safety
/// Input is live and readable for its bounded length.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp_channel_request_validate(input: *const u8, length: u32) -> u32 {
    guard(|| {
        if input.is_null() || length == 0 {
            return Err(16);
        }
        if length as usize > channel::MAX_WIRE_BYTES {
            return Err(17);
        }
        Request::decode(unsafe { std::slice::from_raw_parts(input, length as usize) })
            .map_err(code)?;
        Ok(())
    })
}
/// # Safety
/// Output is writable for capacity bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp_channel_schema_digest(out: *mut u8, capacity: u32) -> u32 {
    guard(|| {
        if out.is_null() {
            return Err(16);
        }
        if capacity < 32 {
            return Err(17);
        }
        unsafe { std::ptr::copy_nonoverlapping(channel::schema_digest().as_ptr(), out, 32) };
        Ok(())
    })
}
/// # Safety
/// Frames are bounded readable bytes; out is a disjoint writable handle slot.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp_channel_response_decode(
    input: *const u8,
    length: u32,
    original: *const u8,
    original_length: u32,
    out: *mut *mut c_void,
) -> u32 {
    if out.is_null() {
        return 16;
    }
    unsafe { *out = std::ptr::null_mut() };
    guard(|| {
        let request_bytes = unsafe {
            bytes(
                Span {
                    data: original,
                    length: original_length,
                },
                channel::MAX_WIRE_BYTES,
            )
        }?;
        let encoded = unsafe {
            bytes(
                Span {
                    data: input,
                    length,
                },
                channel::MAX_WIRE_BYTES,
            )
        }?;
        let request = Request::decode(&request_bytes).map_err(code)?;
        let response = Response::decode(&encoded).map_err(code)?;
        // Decode requires canonical encoding. Bind the original immutable bytes,
        // including their actual layout, rather than a reconstructed descriptor.
        let request_digest = Sha256::digest(&request_bytes).into();
        response
            .validate_for_digest(&request, &request_digest)
            .map_err(code)?;
        let frame_sha256 = match &response.frame {
            Some(f) => f.digest().map_err(code)?,
            None => [0; 32],
        };
        let handle = Box::new(Handle {
            response,
            encoded,
            frame_sha256,
        });
        unsafe { *out = Box::into_raw(handle).cast() };
        Ok(())
    })
}
/// # Safety
/// Handle is live SDK-owned; writable view is aligned and disjoint from handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp_channel_response_get(
    raw: *const c_void,
    out: *mut CView,
    size: u32,
) -> u32 {
    guard(|| {
        if out.is_null() {
            return Err(16);
        }
        if size < size_of::<CView>() as u32 {
            return Err(17);
        }
        unsafe { out.write(CView::default()) };
        if raw.is_null() {
            return Err(16);
        }
        let h = unsafe { &*raw.cast::<Handle>() };
        let r = &h.response;
        let (sequence, payload, cursor) = match &r.frame {
            Some(f) => (f.sequence, span(&f.bytes), span(&f.cursor)),
            None => (0, Span::default(), Span::default()),
        };
        unsafe {
            out.write(CView {
                status: r.status as u32,
                has_frame: u32::from(r.frame.is_some()),
                resource_reclaimed: u32::from(r.resource_reclaimed),
                sequence,
                last_acked: r.last_acked,
                accepted_sequence: r.accepted_sequence,
                call_id: span(&r.call_id),
                reference: span(&r.reference),
                source_epoch: span(&r.source_epoch),
                bytes: payload,
                cursor,
                frame_sha256: span(&h.frame_sha256),
                encoded_frame: span(&h.encoded),
            })
        };
        Ok(())
    })
}
/// # Safety
/// Null or a live SDK handle freed exactly once after every borrowed view expires.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp_channel_response_free(raw: *mut c_void) {
    if !raw.is_null() {
        unsafe { drop(Box::from_raw(raw.cast::<Handle>())) }
    }
}
/// # Safety
/// Input is readable for bounded length; out is a disjoint aligned handle slot.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp_channel_directory_decode(
    input: *const u8,
    length: u32,
    out: *mut *mut c_void,
) -> u32 {
    if out.is_null() {
        return 16;
    }
    unsafe { *out = std::ptr::null_mut() };
    guard(|| {
        let encoded = unsafe {
            bytes(
                Span {
                    data: input,
                    length,
                },
                channel::MAX_WIRE_BYTES,
            )
        }?;
        let directory = channel::Directory::decode(&encoded).map_err(code)?;
        let mut handle = Box::new(DirectoryHandle {
            directory,
            views: Vec::new(),
        });
        handle
            .views
            .try_reserve_exact(handle.directory.channels.len())
            .map_err(|_| 4u32)?;
        // Endpoints stay owned by the boxed handle and never move after views form.
        for endpoint in &handle.directory.channels {
            let b = &endpoint.budget;
            handle.views.push(CEndpoint {
                reference: span(&endpoint.reference),
                source_epoch: span(&endpoint.source_epoch),
                kind: match endpoint.kind {
                    channel::Kind::ByteStream => 0,
                    channel::Kind::Events => 1,
                },
                budget: CBudget {
                    max_channels: b.max_channels,
                    max_frame_bytes: b.max_frame_bytes,
                    max_bytes: b.max_bytes,
                    max_messages: b.max_messages,
                    max_requests: b.max_requests,
                    max_duration_ms: b.max_duration_ms,
                },
            });
        }
        unsafe { *out = Box::into_raw(handle).cast() };
        Ok(())
    })
}
/// # Safety
/// Handle is live SDK-owned; out is aligned disjoint writable view storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp_channel_directory_get(
    raw: *const c_void,
    out: *mut CDirectoryView,
    size: u32,
) -> u32 {
    guard(|| {
        if out.is_null() {
            return Err(16);
        }
        if size < size_of::<CDirectoryView>() as u32 {
            return Err(17);
        }
        unsafe { out.write(CDirectoryView::default()) };
        if raw.is_null() {
            return Err(16);
        }
        let h = unsafe { &*raw.cast::<DirectoryHandle>() };
        unsafe {
            out.write(CDirectoryView {
                scope_sha256: span(&h.directory.scope_sha256),
                channels: h.views.as_ptr(),
                channel_count: h.views.len() as u32,
            })
        };
        Ok(())
    })
}
/// # Safety
/// Null or a live SDK directory handle freed exactly once after views expire.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp_channel_directory_free(raw: *mut c_void) {
    if !raw.is_null() {
        unsafe { drop(Box::from_raw(raw.cast::<DirectoryHandle>())) }
    }
}
/// # Safety
/// Out is a disjoint aligned writable handle slot.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp_channel_digest_new(out: *mut *mut c_void) -> u32 {
    if out.is_null() {
        return 16;
    }
    unsafe { *out = std::ptr::null_mut() };
    guard(|| {
        unsafe { *out = Box::into_raw(Box::new(Sha256::new())).cast() };
        Ok(())
    })
}
/// # Safety
/// Digest is exclusively borrowed and input is live readable bytes, <=64KiB.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp_channel_digest_update(
    raw: *mut c_void,
    input: *const u8,
    length: u32,
) -> u32 {
    guard(|| {
        if raw.is_null() {
            return Err(16);
        }
        let value = unsafe {
            bytes(
                Span {
                    data: input,
                    length,
                },
                channel::MAX_PAYLOAD_BYTES,
            )
        }?;
        unsafe { &mut *raw.cast::<Sha256>() }.update(&value);
        Ok(())
    })
}
/// # Safety
/// Digest is live; out is disjoint writable capacity bytes. Finish preserves state.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp_channel_digest_finish(
    raw: *const c_void,
    out: *mut u8,
    capacity: u32,
) -> u32 {
    guard(|| {
        if raw.is_null() || out.is_null() {
            return Err(16);
        }
        if capacity < 32 {
            return Err(17);
        }
        let digest = unsafe { &*raw.cast::<Sha256>() }.clone().finalize();
        unsafe { std::ptr::copy_nonoverlapping(digest.as_ptr(), out, 32) };
        Ok(())
    })
}
/// # Safety
/// Null or live SDK digest handle freed exactly once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp_channel_digest_free(raw: *mut c_void) {
    if !raw.is_null() {
        unsafe { drop(Box::from_raw(raw.cast::<Sha256>())) }
    }
}
