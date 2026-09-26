//! Local C service codec ABI. Handles own all spans; no authority or listener.
use crate::{
    io,
    service::{self, Error, Header, Reply, Request, Response},
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

#[repr(C)]
#[derive(Default)]
pub struct CRequestView {
    call_id: u64,
    service: Span,
    handler: Span,
    principal: Span,
    method: Span,
    target: Span,
    body: Span,
    headers: *const CHeader,
    header_count: u32,
}
#[repr(C)]
pub struct CReply {
    abi_version: u32,
    struct_size: u32,
    status: u32,
    body: Span,
    headers: *const CHeader,
    header_count: u32,
}
struct RequestHandle {
    request: Request,
    header_views: Vec<CHeader>,
}
/// # Safety
/// Input is readable for length; out is an aligned, disjoint writable slot.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp_service_request_decode(
    bytes: *const u8,
    length: u32,
    out: *mut *mut c_void,
) -> u32 {
    if out.is_null() {
        return 16;
    }
    unsafe {
        *out = std::ptr::null_mut();
    }
    guard(|| {
        if bytes.is_null() || length == 0 {
            return Err(Error::Invalid);
        }
        if length as usize > service::MAX_FRAME_BYTES {
            return Err(Error::Limit);
        }
        let request =
            Request::decode(unsafe { std::slice::from_raw_parts(bytes, length as usize) })?;
        let header_views = request
            .invocation()
            .headers
            .iter()
            .map(|h| CHeader {
                name: span(h.name.as_bytes()),
                value: span(&h.value),
            })
            .collect();
        let handle = Box::new(RequestHandle {
            request,
            header_views,
        });
        unsafe {
            *out = Box::into_raw(handle).cast();
        }
        Ok(())
    })
}
/// # Safety
/// Handle is live and SDK-owned; out is an aligned disjoint writable view.
/// Spans and headers expire when the handle is freed, not when input is freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp_service_request_get(
    raw: *const c_void,
    out: *mut CRequestView,
    size: u32,
) -> u32 {
    guard(|| {
        if out.is_null() {
            return Err(Error::Invalid);
        }
        if size < size_of::<CRequestView>() as u32 {
            return Err(Error::Limit);
        }
        unsafe {
            out.write(CRequestView::default());
        }
        if raw.is_null() {
            return Err(Error::Invalid);
        }
        let handle = unsafe { &*raw.cast::<RequestHandle>() };
        let v = handle.request.invocation();
        unsafe {
            out.write(CRequestView {
                call_id: handle.request.call_id(),
                service: span(v.service.as_bytes()),
                handler: span(v.handler.as_bytes()),
                principal: span(v.principal.as_bytes()),
                method: span(v.method.as_bytes()),
                target: span(v.target.as_bytes()),
                body: span(&v.body),
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
/// Request is live and SDK-owned; descriptor and its spans/headers are readable,
/// aligned and disjoint from writable output and length. Failure leaves output
/// untouched and length zero. The original request's exact digest is retained.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp_service_response_encode(
    raw: *const c_void,
    reply: *const CReply,
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
        if raw.is_null() || reply.is_null() || out.is_null() {
            return Err(Error::Invalid);
        }
        let handle = unsafe { &*raw.cast::<RequestHandle>() };
        let reply = unsafe { &*reply };
        if reply.abi_version != 1 || reply.struct_size < size_of::<CReply>() as u32 {
            return Err(Error::Contract);
        }
        let reply = Reply {
            status: reply.status.try_into().map_err(|_| Error::Invalid)?,
            body: unsafe { read_bytes(reply.body, service::MAX_BODY_BYTES) }?,
            headers: unsafe { read_headers(reply.headers, reply.header_count) }?,
        };
        let bytes = Response::encode(&handle.request, &reply)?;
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
/// Null or an SDK-created handle, freed exactly once after all views expire.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp_service_request_free(raw: *mut c_void) {
    if !raw.is_null() {
        unsafe {
            drop(Box::from_raw(raw.cast::<RequestHandle>()));
        }
    }
}
