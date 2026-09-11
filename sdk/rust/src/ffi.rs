//! Reviewed local C codec ABI. Opaque reply pointers never represent host capabilities.
use crate::protocol::{Action, CodecError, Reply, Request};
use std::{ffi::c_void, panic::catch_unwind};
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Span {
    data: *const u8,
    length: u32,
}
#[repr(C)]
pub struct CRequest {
    abi_version: u32,
    struct_size: u32,
    kind: u32,
    request_id: Span,
    card_id: Span,
    title: Span,
    operation_id: Span,
    attachment_id: Span,
    revision: u64,
    offset: u64,
    length: u32,
}
#[repr(C)]
#[derive(Default)]
pub struct View {
    kind: u32,
    failure: u32,
    state: u32,
    format_version: u32,
    revision: u64,
    offset: u64,
    total_length: u64,
    request_id: Span,
    card_id: Span,
    operation_id: Span,
    event_id: Span,
    type_id: Span,
    title: Span,
    preview: Span,
    attachment_id: Span,
    sha256: Span,
    bytes: Span,
}
struct Handle {
    request_id: String,
    reply: Reply,
}
fn span(v: &[u8]) -> Span {
    Span {
        data: v.as_ptr(),
        length: v.len() as u32,
    }
}
fn guard(f: impl FnOnce() -> Result<(), CodecError> + std::panic::UnwindSafe) -> u32 {
    match catch_unwind(f) {
        Ok(Ok(())) => 0,
        Ok(Err(CodecError::Invalid)) => 16,
        Ok(Err(CodecError::Limit)) => 17,
        Ok(Err(CodecError::Contract)) => 18,
        Ok(Err(CodecError::Correlation)) => 19,
        Err(_) => 20,
    }
}
unsafe fn read_text(s: Span, limit: usize) -> Result<String, CodecError> {
    if s.length as usize > limit {
        return Err(CodecError::Limit);
    }
    if s.length == 0 {
        return Ok(String::new());
    }
    if s.data.is_null() {
        return Err(CodecError::Invalid);
    }
    // SAFETY: foreign caller promises readable span for its stated bounded length.
    let bytes = unsafe { std::slice::from_raw_parts(s.data, s.length as usize) };
    Ok(std::str::from_utf8(bytes)
        .map_err(|_| CodecError::Invalid)?
        .to_owned())
}
unsafe fn request(raw: *const CRequest) -> Result<Request, CodecError> {
    if raw.is_null() {
        return Err(CodecError::Invalid);
    }
    // SAFETY: caller passes a live aligned CRequest, not a wire pointer.
    let r = unsafe { &*raw };
    if r.abi_version != 1 || r.struct_size < size_of::<CRequest>() as u32 {
        return Err(CodecError::Contract);
    }
    // SAFETY: spans are bounded/checked before use; lifetime is the current call.
    let result = unsafe {
        Request {
            request_id: read_text(r.request_id, 256)?,
            card_id: read_text(r.card_id, 256)?,
            action: match r.kind {
                1 => Action::Rename {
                    revision: r.revision,
                    title: read_text(r.title, 16384)?,
                },
                2 => Action::ReadSummary,
                3 => Action::QueryOperation {
                    operation_id: read_text(r.operation_id, 256)?,
                },
                4 => Action::ReadAttachment {
                    attachment_id: read_text(r.attachment_id, 256)?,
                    revision: r.revision,
                    offset: r.offset,
                    length: r.length,
                },
                _ => return Err(CodecError::Invalid),
            },
        }
    };
    result.validate()?;
    Ok(result)
}
/// # Safety
/// Foreign pointers are aligned, disjoint and valid for the declared sizes for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp_request_encode(
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
            return Err(CodecError::Invalid);
        }
        let bytes = unsafe { request(raw) }?.encode()?;
        if bytes.len() > capacity as usize {
            return Err(CodecError::Limit);
        }
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), out, bytes.len());
            *length = bytes.len() as u32;
        }
        Ok(())
    })
}
/// # Safety
/// Inputs are live and readable for their lengths; out is a disjoint writable handle slot.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp_reply_decode(
    bytes: *const u8,
    length: u32,
    raw: *const CRequest,
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
            return Err(CodecError::Invalid);
        }
        if length as usize > crate::MAX_MESSAGE_BYTES {
            return Err(CodecError::Limit);
        }
        let request = unsafe { request(raw) }?;
        let reply =
            request.decode_reply(unsafe { std::slice::from_raw_parts(bytes, length as usize) })?;
        let handle = Box::new(Handle {
            request_id: request.request_id,
            reply,
        });
        unsafe {
            *out = Box::into_raw(handle).cast();
        }
        Ok(())
    })
}
fn receipt(view: &mut View, r: &crate::protocol::Receipt) {
    view.card_id = span(r.card_id.as_bytes());
    view.operation_id = span(r.operation_id.as_bytes());
    view.event_id = span(r.event_id.as_bytes());
    view.revision = r.revision;
    view.sha256 = span(&r.sha256);
}
/// # Safety
/// Pointer is a live SDK-created reply; view is disjoint/aligned and writable for size bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp_reply_get(raw: *const c_void, out: *mut View, size: u32) -> u32 {
    guard(|| {
        if raw.is_null() || out.is_null() {
            return Err(CodecError::Invalid);
        }
        if size < size_of::<View>() as u32 {
            return Err(CodecError::Limit);
        }
        let h = unsafe { &*raw.cast::<Handle>() };
        let mut v = View {
            request_id: span(h.request_id.as_bytes()),
            ..Default::default()
        };
        match &h.reply {
            Reply::Renamed(r) => {
                v.kind = 1;
                v.state = 1;
                receipt(&mut v, r);
            }
            Reply::Summary(r) => {
                v.kind = 2;
                v.card_id = span(r.card_id.as_bytes());
                v.type_id = span(r.type_id.as_bytes());
                v.title = span(r.title.as_bytes());
                v.preview = span(r.preview.as_bytes());
                v.revision = r.revision;
                v.format_version = r.format_version;
            }
            Reply::Rejected(f) => {
                v.kind = 3;
                v.failure = *f as u32;
            }
            Reply::OperationResult {
                card_id,
                operation_id,
                result,
            } => {
                v.kind = 4;
                v.card_id = span(card_id.as_bytes());
                v.operation_id = span(operation_id.as_bytes());
                if let Some(r) = result {
                    v.state = 1;
                    receipt(&mut v, r);
                }
            }
            Reply::Attachment(r) => {
                v.kind = 5;
                v.card_id = span(r.card_id.as_bytes());
                v.attachment_id = span(r.attachment_id.as_bytes());
                v.revision = r.revision;
                v.offset = r.offset;
                v.total_length = r.total_length;
                v.sha256 = span(&r.sha256);
                v.bytes = span(&r.bytes);
            }
        };
        unsafe {
            out.write(v);
        }
        Ok(())
    })
}
/// # Safety
/// A nonnull pointer must be an SDK-created handle, freed exactly once and no longer borrowed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp_reply_free(raw: *mut c_void) {
    if !raw.is_null() {
        unsafe {
            drop(Box::from_raw(raw.cast::<Handle>()));
        }
    }
}
