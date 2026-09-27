//! Pure C ABI for the independent mutation wire codec. No import or host call.
use crate::mutation::{self, Action, Error, Request, Response};
use std::{ffi::c_void, mem::align_of, panic::catch_unwind};

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Span {
    data: *const u8,
    length: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct DescriptorPrefix {
    abi_version: u32,
    struct_size: u32,
}

#[repr(C)]
pub struct CRequest {
    abi_version: u32,
    struct_size: u32,
    kind: u32,
    call_id: u64,
    reference: Span,
    submission: Span,
    operation_id: Span,
    deadline_ms: u32,
    content_length: u64,
    content_sha256: Span,
    offset: u64,
    bytes: Span,
}

#[repr(C)]
#[derive(Default)]
pub struct CResponseView {
    kind: u32,
    status: u32,
    phase: u32,
    effect: u32,
    call_id: u64,
    staged_bytes: u64,
    durable_content: u32,
    reference: Span,
    submission: Span,
    operation_id: Span,
    encoded_frame: Span,
}

struct ResponseHandle {
    response: Response,
    encoded_frame: Vec<u8>,
}

fn code(error: Error) -> u32 {
    match error {
        Error::Invalid => 16,
        Error::Limit => 17,
        Error::Contract => 18,
        Error::Correlation => 19,
        Error::Unsupported => 21,
    }
}

fn guard(f: impl FnOnce() -> Result<(), Error> + std::panic::UnwindSafe) -> u32 {
    match catch_unwind(f) {
        Ok(Ok(())) => 0,
        Ok(Err(error)) => code(error),
        Err(_) => 20,
    }
}

fn aligned<T>(pointer: *const T) -> bool {
    !pointer.is_null() && (pointer as usize).is_multiple_of(align_of::<T>())
}

fn empty(span: Span) -> bool {
    span.data.is_null() && span.length == 0
}

fn view_span(bytes: &[u8]) -> Span {
    if bytes.is_empty() {
        Span::default()
    } else {
        Span {
            data: bytes.as_ptr(),
            length: bytes.len() as u32,
        }
    }
}

unsafe fn bytes(raw: Span, limit: usize) -> Result<Vec<u8>, Error> {
    if raw.length as usize > limit {
        return Err(Error::Limit);
    }
    if raw.length == 0 {
        return Ok(Vec::new());
    }
    if raw.data.is_null() {
        return Err(Error::Invalid);
    }
    // SAFETY: The caller promises a readable span for the duration of this call.
    Ok(unsafe { std::slice::from_raw_parts(raw.data, raw.length as usize) }.to_vec())
}

unsafe fn fixed32(raw: Span) -> Result<[u8; 32], Error> {
    if raw.length != 32 {
        return Err(Error::Invalid);
    }
    unsafe { bytes(raw, 32) }?
        .try_into()
        .map_err(|_| Error::Invalid)
}

unsafe fn request(raw: *const CRequest) -> Result<Request, Error> {
    if !aligned(raw) {
        return Err(Error::Invalid);
    }
    // SAFETY: The caller guarantees a live descriptor prefix. Read only that
    // prefix before checking size; never form a full reference for a short one.
    let prefix = unsafe { std::ptr::read(raw.cast::<DescriptorPrefix>()) };
    if prefix.abi_version != 1 || prefix.struct_size < size_of::<CRequest>() as u32 {
        return Err(Error::Contract);
    }
    // SAFETY: A passing prefix promises the complete aligned descriptor exists.
    let raw = unsafe { &*raw };
    if !(1..=8).contains(&raw.kind) {
        return Err(Error::Invalid);
    }
    if raw.call_id == 0 || raw.deadline_ms == 0 {
        return Err(Error::Invalid);
    }
    if raw.deadline_ms > mutation::MAX_DEADLINE_MS
        || raw.operation_id.length as usize > mutation::MAX_OPERATION_BYTES
        || raw.bytes.length as usize > mutation::MAX_CHUNK_BYTES
    {
        return Err(Error::Limit);
    }
    if raw.reference.length != 32
        || raw.submission.length != 32
        || raw.operation_id.length == 0
        || raw.reference.data.is_null()
        || raw.submission.data.is_null()
        || raw.operation_id.data.is_null()
    {
        return Err(Error::Invalid);
    }
    if (raw.kind == 1 && raw.content_length > mutation::MAX_CONTENT_BYTES)
        || (raw.kind == 3
            && raw
                .offset
                .checked_add(raw.bytes.length as u64)
                .is_none_or(|end| end > mutation::MAX_CONTENT_BYTES))
    {
        return Err(Error::Limit);
    }
    // Reject variant garbage before copying any caller span. The default form
    // is strict even when the span length is zero but its pointer is non-null.
    if (raw.kind != 1 && (raw.content_length != 0 || !empty(raw.content_sha256)))
        || (raw.kind != 3 && (raw.offset != 0 || !empty(raw.bytes)))
    {
        return Err(Error::Invalid);
    }
    if raw.kind == 1 && (raw.content_sha256.length != 32 || raw.content_sha256.data.is_null()) {
        return Err(Error::Invalid);
    }
    if raw.kind == 3 && (raw.bytes.length == 0 || raw.bytes.data.is_null()) {
        return Err(Error::Invalid);
    }
    let reference = unsafe { fixed32(raw.reference) }?;
    let submission = unsafe { fixed32(raw.submission) }?;
    let operation_id =
        String::from_utf8(unsafe { bytes(raw.operation_id, mutation::MAX_OPERATION_BYTES) }?)
            .map_err(|_| Error::Invalid)?;
    let action = match raw.kind {
        1 => Action::Create {
            content_length: raw.content_length,
            content_sha256: unsafe { fixed32(raw.content_sha256) }?,
        },
        2 => Action::Delete,
        3 => Action::Chunk {
            offset: raw.offset,
            bytes: unsafe { bytes(raw.bytes, mutation::MAX_CHUNK_BYTES) }?,
        },
        4 => Action::Commit,
        5 => Action::Execute,
        6 => Action::Query,
        7 => Action::CancelPlan,
        8 => Action::Release,
        _ => unreachable!(),
    };
    let request = Request {
        call_id: raw.call_id,
        reference,
        submission,
        operation_id,
        deadline_ms: raw.deadline_ms,
        action,
    };
    request.validate()?;
    Ok(request)
}

/// # Safety
/// `raw` names a live aligned descriptor prefix and, when its size is valid,
/// a full descriptor. All spans, output and length are live and disjoint.
/// Failure sets length to zero and leaves the output buffer untouched.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp_mutation_request_encode(
    raw: *const CRequest,
    out: *mut u8,
    capacity: u32,
    length: *mut u32,
) -> u32 {
    if !aligned(length) {
        return 16;
    }
    unsafe { *length = 0 };
    guard(|| {
        if out.is_null() {
            return Err(Error::Invalid);
        }
        let request = unsafe { request(raw) }?;
        let encoded = request.encode()?;
        if encoded.len() > capacity as usize {
            return Err(Error::Limit);
        }
        // SAFETY: The caller promises a writable disjoint buffer of capacity.
        unsafe {
            std::ptr::copy_nonoverlapping(encoded.as_ptr(), out, encoded.len());
            *length = encoded.len() as u32;
        }
        Ok(())
    })
}

/// # Safety
/// `bytes` is readable for `length` bytes for this synchronous pure check.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp_mutation_request_validate(bytes: *const u8, length: u32) -> u32 {
    guard(|| {
        if bytes.is_null() || length == 0 {
            return Err(Error::Invalid);
        }
        if length as usize > mutation::MAX_FRAME_BYTES {
            return Err(Error::Limit);
        }
        Request::decode(unsafe { std::slice::from_raw_parts(bytes, length as usize) })?;
        Ok(())
    })
}

/// # Safety
/// `out` is writable for at least `capacity` bytes and disjoint from all input.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp_mutation_schema_digest(out: *mut u8, capacity: u32) -> u32 {
    guard(|| {
        if out.is_null() {
            return Err(Error::Invalid);
        }
        if capacity < 32 {
            return Err(Error::Limit);
        }
        let digest = mutation::schema_digest();
        unsafe { std::ptr::copy_nonoverlapping(digest.as_ptr(), out, 32) };
        Ok(())
    })
}

/// # Safety
/// Both frames are readable for their stated lengths. `out` is an aligned,
/// disjoint writable slot. The original request frame remains immutable here.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp_mutation_response_decode(
    bytes: *const u8,
    length: u32,
    request_frame: *const u8,
    request_length: u32,
    out: *mut *mut c_void,
) -> u32 {
    if !aligned(out) {
        return 16;
    }
    unsafe { *out = std::ptr::null_mut() };
    guard(|| {
        if bytes.is_null() || length == 0 || request_frame.is_null() || request_length == 0 {
            return Err(Error::Invalid);
        }
        if length as usize > mutation::MAX_FRAME_BYTES
            || request_length as usize > mutation::MAX_FRAME_BYTES
        {
            return Err(Error::Limit);
        }
        let original =
            unsafe { std::slice::from_raw_parts(request_frame, request_length as usize) };
        let reply = unsafe { std::slice::from_raw_parts(bytes, length as usize) };
        let request = Request::decode(original)?;
        let response = Response::decode(&request, reply)?;
        let handle = Box::new(ResponseHandle {
            response,
            encoded_frame: reply.to_vec(),
        });
        unsafe { *out = Box::into_raw(handle).cast() };
        Ok(())
    })
}

/// # Safety
/// `raw` is a live SDK handle; `out` is an aligned, disjoint writable view slot.
/// Borrowed spans remain valid only until that handle is freed exactly once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp_mutation_response_get(
    raw: *const c_void,
    out: *mut CResponseView,
    size: u32,
) -> u32 {
    guard(|| {
        if !aligned(out) {
            return Err(Error::Invalid);
        }
        if size < size_of::<CResponseView>() as u32 {
            return Err(Error::Limit);
        }
        unsafe { out.write(CResponseView::default()) };
        if !aligned(raw.cast::<ResponseHandle>()) {
            return Err(Error::Invalid);
        }
        let handle = unsafe { &*raw.cast::<ResponseHandle>() };
        let response = &handle.response;
        unsafe {
            out.write(CResponseView {
                kind: response.kind as u32,
                status: response.status as u32,
                phase: response.phase as u32,
                effect: response.effect as u32,
                call_id: response.call_id,
                staged_bytes: response.staged_bytes,
                durable_content: u32::from(response.durable_content),
                reference: view_span(&response.reference),
                submission: view_span(&response.submission),
                operation_id: view_span(response.operation_id.as_bytes()),
                encoded_frame: view_span(&handle.encoded_frame),
            });
        }
        Ok(())
    })
}

/// # Safety
/// `raw` is null or a live SDK handle freed exactly once after all views expire.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp_mutation_response_free(raw: *mut c_void) {
    if !raw.is_null() && aligned(raw.cast::<ResponseHandle>()) {
        unsafe { drop(Box::from_raw(raw.cast::<ResponseHandle>())) };
    }
}
