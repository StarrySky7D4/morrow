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
#[derive(Default)]
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

#[repr(C)]
pub struct TaskView {
    task_id: Span,
    command: Span,
    request: CRequest,
}
/// # Safety
/// Readable input and disjoint writable handle slot. Successful opaque handle is freed once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp_task_decode(
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
            return Err(CodecError::Invalid);
        }
        if length as usize > crate::task::MAX_TASK_BYTES {
            return Err(CodecError::Limit);
        }
        let task = crate::task::Invocation::decode(unsafe {
            std::slice::from_raw_parts(bytes, length as usize)
        })?;
        unsafe {
            *out = Box::into_raw(Box::new(task)).cast();
        }
        Ok(())
    })
}
/// # Safety
/// Valid SDK task; aligned disjoint writable view. All returned spans expire on task free.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp_task_get(raw: *const c_void, out: *mut TaskView, size: u32) -> u32 {
    guard(|| {
        if raw.is_null() || out.is_null() {
            return Err(CodecError::Invalid);
        }
        if size < size_of::<TaskView>() as u32 {
            return Err(CodecError::Limit);
        }
        let task = unsafe { &*raw.cast::<crate::task::Invocation>() };
        let r = task.request().ok_or(CodecError::Invalid)?;
        let mut request = CRequest {
            abi_version: 1,
            struct_size: size_of::<CRequest>() as u32,
            request_id: span(r.request_id.as_bytes()),
            card_id: span(r.card_id.as_bytes()),
            ..Default::default()
        };
        match &r.action {
            Action::Rename { revision, title } => {
                request.kind = 1;
                request.revision = *revision;
                request.title = span(title.as_bytes());
            }
            Action::ReadSummary => request.kind = 2,
            Action::QueryOperation { operation_id } => {
                request.kind = 3;
                request.operation_id = span(operation_id.as_bytes());
            }
            Action::ReadAttachment {
                attachment_id,
                revision,
                offset,
                length,
            } => {
                request.kind = 4;
                request.attachment_id = span(attachment_id.as_bytes());
                request.revision = *revision;
                request.offset = *offset;
                request.length = *length;
            }
        };
        unsafe {
            out.write(TaskView {
                task_id: span(task.task_id().as_bytes()),
                command: span(task.command_bytes()),
                request,
            });
        }
        Ok(())
    })
}
/// # Safety
/// SDK task and readable response; disjoint output/length slots with stated capacity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp_task_complete(
    raw: *const c_void,
    response: *const u8,
    response_length: u32,
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
        if raw.is_null() || response.is_null() || out.is_null() {
            return Err(CodecError::Invalid);
        }
        if response_length as usize > crate::MAX_MESSAGE_BYTES {
            return Err(CodecError::Limit);
        }
        let task = unsafe { &*raw.cast::<crate::task::Invocation>() };
        let bytes = task.completion(unsafe {
            std::slice::from_raw_parts(response, response_length as usize)
        })?;
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
/// Null or live SDK task, freed exactly once after all borrowed views expire.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp_task_free(raw: *mut c_void) {
    if !raw.is_null() {
        unsafe {
            drop(Box::from_raw(raw.cast::<crate::task::Invocation>()));
        }
    }
}

#[repr(C)]
pub struct TransformView {
    handler: Span,
    input_type: Span,
    output_type: Span,
    input: Span,
}
/// # Safety
/// SDK task and aligned disjoint writable view. Returned spans expire when the task is freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp_task_get_transform(
    raw: *const c_void,
    out: *mut TransformView,
    size: u32,
) -> u32 {
    guard(|| {
        if raw.is_null() || out.is_null() {
            return Err(CodecError::Invalid);
        }
        if size < size_of::<TransformView>() as u32 {
            return Err(CodecError::Limit);
        }
        let task = unsafe { &*raw.cast::<crate::task::Invocation>() };
        let t = task.transform().ok_or(CodecError::Invalid)?;
        unsafe {
            out.write(TransformView {
                handler: span(t.handler.as_bytes()),
                input_type: span(t.input_type.as_bytes()),
                output_type: span(t.output_type.as_bytes()),
                input: span(&t.input),
            });
        }
        Ok(())
    })
}
/// # Safety
/// SDK task; readable bytes (null allowed only for zero length); disjoint writable output/control slots.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp_task_output(
    raw: *const c_void,
    bytes: *const u8,
    size: u32,
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
        if raw.is_null() || out.is_null() || (bytes.is_null() && size != 0) {
            return Err(CodecError::Invalid);
        }
        if size as usize > crate::task::MAX_VALUE_BYTES {
            return Err(CodecError::Limit);
        }
        let task = unsafe { &*raw.cast::<crate::task::Invocation>() };
        let input = if size == 0 {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(bytes, size as usize) }
        };
        let result = task.output(input)?;
        if result.len() > capacity as usize {
            return Err(CodecError::Limit);
        }
        unsafe {
            std::ptr::copy_nonoverlapping(result.as_ptr(), out, result.len());
            *length = result.len() as u32;
        }
        Ok(())
    })
}

/// # Safety
/// Live SDK task; readable message; disjoint writable output and aligned length slot.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp_task_fail(
    raw: *const c_void,
    code: u32,
    message: *const u8,
    size: u32,
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
        if raw.is_null() || out.is_null() || message.is_null() || size == 0 {
            return Err(CodecError::Invalid);
        }
        if size as usize > crate::task::MAX_FAILURE_MESSAGE_BYTES {
            return Err(CodecError::Limit);
        }
        let code = match code {
            0 => crate::task::FailureCode::InvalidInput,
            1 => crate::task::FailureCode::UnsupportedInput,
            2 => crate::task::FailureCode::ResourceLimit,
            3 => crate::task::FailureCode::Failed,
            _ => return Err(CodecError::Invalid),
        };
        let task = unsafe { &*raw.cast::<crate::task::Invocation>() };
        let message =
            std::str::from_utf8(unsafe { std::slice::from_raw_parts(message, size as usize) })
                .map_err(|_| CodecError::Invalid)?;
        let result = task.failure(code, message)?;
        if result.len() > capacity as usize {
            return Err(CodecError::Limit);
        }
        unsafe {
            std::ptr::copy_nonoverlapping(result.as_ptr(), out, result.len());
            *length = result.len() as u32;
        }
        Ok(())
    })
}

#[cfg(test)]
mod task_failure_tests {
    use super::*;
    #[test]
    fn c_failure_api_rejects_invalid_codes_utf8_lengths_and_short_buffers() {
        let mut m = capnp::message::Builder::new_default();
        let mut r = m.init_root::<crate::task_capnp::invocation::Builder>();
        r.set_version(crate::contract::TASK_VERSION);
        r.set_schema_digest(&crate::contract::TASK_DIGEST);
        r.set_task_id("task");
        r.set_kind(crate::task_capnp::Kind::Transform);
        let mut t = r.init_transform();
        t.set_handler("validate");
        t.set_input_type("bytes");
        t.set_output_type("bytes");
        t.set_input(&[]);
        let task =
            crate::task::Invocation::decode(&capnp::serialize::write_message_to_words(&m)).unwrap();
        let ptr = (&task as *const crate::task::Invocation).cast::<c_void>();
        let mut output = vec![0xa5; crate::task::MAX_TASK_BYTES];
        for code in 0..4 {
            let mut length = 0;
            let status = unsafe {
                mp_task_fail(
                    ptr,
                    code,
                    b"failure".as_ptr(),
                    7,
                    output.as_mut_ptr(),
                    output.len() as u32,
                    &mut length,
                )
            };
            assert_eq!(status, 0);
            assert!(length > 0);
        }
        for (code, bytes, capacity) in [
            (99, b"failure".as_slice(), 131072),
            (0, &[0xff][..], 131072),
            (0, &[][..], 131072),
            (0, b"failure".as_slice(), 1),
        ] {
            output.fill(0xa5);
            let mut length = 999;
            let status = unsafe {
                mp_task_fail(
                    ptr,
                    code,
                    bytes.as_ptr(),
                    bytes.len() as u32,
                    output.as_mut_ptr(),
                    capacity,
                    &mut length,
                )
            };
            assert_ne!(status, 0);
            assert_eq!(length, 0);
            assert!(output.iter().all(|b| *b == 0xa5));
        }
        let too_long = vec![b'x'; 1025];
        let mut length = 999;
        assert_ne!(
            unsafe {
                mp_task_fail(
                    ptr,
                    0,
                    too_long.as_ptr(),
                    1025,
                    output.as_mut_ptr(),
                    output.len() as u32,
                    &mut length,
                )
            },
            0
        );
        assert_eq!(length, 0);
    }
}
