//! Experimental local C IO codec ABI. No host authority or transport lives here.
use crate::io::{
    self, Action, Error, Header, HttpRequest, Request, Response, Submission, SubmissionKind,
};
use std::{ffi::c_void, panic::catch_unwind};

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Span {
    data: *const u8,
    length: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct CHeader {
    name: Span,
    value: Span,
}

#[repr(C)]
pub struct CRequest {
    abi_version: u32,
    struct_size: u32,
    kind: u32,
    call_id: u64,
    reference: Span,
    operation_id: Span,
    endpoint: Span,
    method: Span,
    relative_target: Span,
    credential: Span,
    body: Span,
    headers: *const CHeader,
    header_count: u32,
    deadline_ms: u64,
    offset: u64,
    limit: u32,
}

#[repr(C)]
#[derive(Default)]
pub struct CResponseView {
    status: u32,
    reference: Span,
    bytes: Span,
    encoded_frame: Span,
    offset: u64,
    eof: u32,
    http_status: u32,
    headers: *const CHeader,
    header_count: u32,
}

struct ResponseHandle {
    response: Response,
    header_views: Vec<CHeader>,
}

fn span(bytes: &[u8]) -> Span {
    if bytes.is_empty() {
        Span::default()
    } else {
        Span {
            data: bytes.as_ptr(),
            length: bytes.len() as u32,
        }
    }
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

unsafe fn read_bytes(raw: Span, limit: usize) -> Result<Vec<u8>, Error> {
    if raw.length as usize > limit {
        return Err(Error::Limit);
    }
    if raw.length == 0 {
        return Ok(Vec::new());
    }
    if raw.data.is_null() {
        return Err(Error::Invalid);
    }
    // SAFETY: The caller promises a readable span for this synchronous call.
    Ok(unsafe { std::slice::from_raw_parts(raw.data, raw.length as usize) }.to_vec())
}

unsafe fn read_text(raw: Span, limit: usize) -> Result<String, Error> {
    String::from_utf8(unsafe { read_bytes(raw, limit) }?).map_err(|_| Error::Invalid)
}

unsafe fn read_reference(raw: Span) -> Result<[u8; 32], Error> {
    if raw.length != 32 {
        return Err(Error::Invalid);
    }
    unsafe { read_bytes(raw, 32) }?
        .try_into()
        .map_err(|_| Error::Invalid)
}

unsafe fn read_headers(raw: *const CHeader, count: u32) -> Result<Vec<Header>, Error> {
    if count as usize > io::MAX_HEADERS {
        return Err(Error::Limit);
    }
    if count == 0 {
        return Ok(Vec::new());
    }
    if raw.is_null() {
        return Err(Error::Invalid);
    }
    // SAFETY: Count is bounded before taking the caller-owned array view.
    unsafe { std::slice::from_raw_parts(raw, count as usize) }
        .iter()
        .map(|header| unsafe {
            Ok(Header {
                name: read_text(header.name, io::MAX_HEADER_NAME_BYTES)?,
                value: read_bytes(header.value, io::MAX_HEADER_VALUE_BYTES)?,
            })
        })
        .collect()
}

unsafe fn request(raw: *const CRequest) -> Result<Request, Error> {
    if raw.is_null() {
        return Err(Error::Invalid);
    }
    // SAFETY: Caller passes an aligned live descriptor, never a wire pointer.
    let raw = unsafe { &*raw };
    if raw.abi_version != 1 || raw.struct_size < size_of::<CRequest>() as u32 {
        return Err(Error::Contract);
    }
    let action = match raw.kind {
        1 => Action::Submit(Submission {
            operation_id: unsafe { read_bytes(raw.operation_id, io::MAX_OPERATION_BYTES) }?,
            deadline_ms: raw.deadline_ms,
            kind: SubmissionKind::FileRead {
                reference: unsafe { read_reference(raw.reference) }?,
            },
        }),
        2 => Action::Submit(Submission {
            operation_id: unsafe { read_bytes(raw.operation_id, io::MAX_OPERATION_BYTES) }?,
            deadline_ms: raw.deadline_ms,
            kind: SubmissionKind::HttpRequest(HttpRequest {
                endpoint: unsafe { read_bytes(raw.endpoint, io::MAX_ENDPOINT_BYTES) }?,
                method: unsafe { read_text(raw.method, io::MAX_METHOD_BYTES) }?,
                relative_target: unsafe { read_text(raw.relative_target, io::MAX_TARGET_BYTES) }?,
                headers: unsafe { read_headers(raw.headers, raw.header_count) }?,
                body: unsafe { read_bytes(raw.body, io::MAX_PAYLOAD_BYTES) }?,
                credential: unsafe { read_bytes(raw.credential, io::MAX_CREDENTIAL_BYTES) }?,
            }),
        }),
        3 => Action::Poll {
            reference: unsafe { read_reference(raw.reference) }?,
        },
        4 => Action::Read {
            reference: unsafe { read_reference(raw.reference) }?,
            offset: raw.offset,
            limit: raw.limit,
        },
        5 => Action::Finish {
            reference: unsafe { read_reference(raw.reference) }?,
        },
        6 => Action::Cancel {
            reference: unsafe { read_reference(raw.reference) }?,
        },
        7 => Action::QueryOperation {
            operation_id: unsafe { read_bytes(raw.operation_id, io::MAX_OPERATION_BYTES) }?,
        },
        _ => return Err(Error::Invalid),
    };
    Request::new(raw.call_id, action)
}

/// # Safety
/// Descriptor spans, output, and length slot are live, aligned, writable/readable as
/// appropriate and disjoint. Failed calls leave output untouched and length zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp_io_request_encode(
    raw: *const CRequest,
    out: *mut u8,
    capacity: u32,
    length: *mut u32,
) -> u32 {
    if length.is_null() {
        return 16;
    }
    unsafe {
        *length = 0;
    }
    guard(|| {
        if out.is_null() {
            return Err(Error::Invalid);
        }
        let request = unsafe { request(raw) }?;
        let bytes = request.bytes();
        if bytes.len() > capacity as usize {
            return Err(Error::Limit);
        }
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), out, bytes.len());
            *length = bytes.len() as u32;
        }
        Ok(())
    })
}

/// # Safety
/// Input is a readable frame for the stated length. This pure check does not
/// grant admission or contact the host.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp_io_request_validate(bytes: *const u8, length: u32) -> u32 {
    guard(|| {
        if bytes.is_null() || length == 0 {
            return Err(Error::Invalid);
        }
        if length as usize > io::MAX_FRAME_BYTES {
            return Err(Error::Limit);
        }
        Request::decode(unsafe { std::slice::from_raw_parts(bytes, length as usize) })?;
        Ok(())
    })
}

/// # Safety
/// Output is live and writable for at least capacity bytes. Writes exactly 32 bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp_io_schema_digest(out: *mut u8, capacity: u32) -> u32 {
    guard(|| {
        if out.is_null() {
            return Err(Error::Invalid);
        }
        if capacity < 32 {
            return Err(Error::Limit);
        }
        let digest = io::schema_digest();
        unsafe {
            std::ptr::copy_nonoverlapping(digest.as_ptr(), out, 32);
        }
        Ok(())
    })
}

/// # Safety
/// Both frames are readable for the stated lengths; out is a disjoint aligned
/// writable slot. Keep the exact request frame immutable until this call ends.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp_io_response_decode(
    bytes: *const u8,
    length: u32,
    request_frame: *const u8,
    request_length: u32,
    out: *mut *mut c_void,
) -> u32 {
    if out.is_null() {
        return 16;
    }
    unsafe {
        *out = std::ptr::null_mut();
    }
    guard(|| {
        if bytes.is_null() || length == 0 || request_frame.is_null() || request_length == 0 {
            return Err(Error::Invalid);
        }
        if length as usize > io::MAX_FRAME_BYTES || request_length as usize > io::MAX_FRAME_BYTES {
            return Err(Error::Limit);
        }
        // Decode original bytes, not a reconstructed C descriptor: the hash binds
        // the exact frame layout submitted to the host.
        let request = Request::decode(unsafe {
            std::slice::from_raw_parts(request_frame, request_length as usize)
        })?;
        let response =
            request.decode_reply(unsafe { std::slice::from_raw_parts(bytes, length as usize) })?;
        let header_views = response
            .headers
            .iter()
            .map(|h| CHeader {
                name: span(h.name.as_bytes()),
                value: span(&h.value),
            })
            .collect();
        let handle = Box::new(ResponseHandle {
            response,
            header_views,
        });
        unsafe {
            *out = Box::into_raw(handle).cast();
        }
        Ok(())
    })
}

/// # Safety
/// Handle is live and SDK-owned; out is an aligned disjoint view slot. All view
/// spans and header array expire when the handle is freed exactly once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp_io_response_get(
    raw: *const c_void,
    out: *mut CResponseView,
    size: u32,
) -> u32 {
    guard(|| {
        if out.is_null() {
            return Err(Error::Invalid);
        }
        if size < size_of::<CResponseView>() as u32 {
            return Err(Error::Limit);
        }
        unsafe {
            out.write(CResponseView::default());
        }
        if raw.is_null() {
            return Err(Error::Invalid);
        }
        let handle = unsafe { &*raw.cast::<ResponseHandle>() };
        let response = &handle.response;
        unsafe {
            out.write(CResponseView {
                status: response.status as u32,
                reference: span(&response.reference),
                bytes: span(&response.bytes),
                encoded_frame: span(response.encoded_frame()),
                offset: response.offset,
                eof: u32::from(response.eof),
                http_status: u32::from(response.http_status),
                headers: if handle.header_views.is_empty() {
                    std::ptr::null()
                } else {
                    handle.header_views.as_ptr()
                },
                header_count: handle.header_views.len() as u32,
            });
        }
        Ok(())
    })
}

/// # Safety
/// Null or an SDK-created handle freed exactly once after all views expire.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp_io_response_free(raw: *mut c_void) {
    if !raw.is_null() {
        unsafe {
            drop(Box::from_raw(raw.cast::<ResponseHandle>()));
        }
    }
}
