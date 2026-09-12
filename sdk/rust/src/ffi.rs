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
            Reply::ContentCommitted(r) => {
                v.kind = 6;
                v.state = 1;
                receipt(&mut v, r);
            }
            Reply::Content(r) => {
                v.kind = 7;
                v.card_id = span(r.card_id.as_bytes());
                v.revision = r.revision;
                v.offset = r.offset;
                v.total_length = r.total_length;
                v.sha256 = span(&r.sha256);
                v.bytes = span(&r.bytes);
            }
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
            Action::CreateContent { .. }
            | Action::EditContent { .. }
            | Action::ReadContent { .. } => return Err(CodecError::Contract),
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

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct UiNode {
    id: Span,
    parent: Span,
    label: Span,
    text: Span,
    action: Span,
    kind: u32,
    tone: u32,
    enabled: u32,
    checked: u32,
    max_bytes: u32,
}
#[repr(C)]
#[derive(Default)]
pub struct UiEventView {
    view: Span,
    node: Span,
    action: Span,
    text: Span,
    generation: u64,
    revision: u64,
    serial: u64,
    kind: u32,
    checked: u32,
}
/// # Safety
/// All pointers are live, aligned, disjoint and valid for their stated lengths.
/// Nodes and spans are borrowed only during this pure codec call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp_ui_document_encode(
    abi: u32,
    node_size: u32,
    nodes: *const UiNode,
    count: u32,
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
        if abi != 1 || node_size != size_of::<UiNode>() as u32 {
            return Err(CodecError::Contract);
        }
        if count == 0 || count as usize > crate::ui::MAX_NODES {
            return Err(CodecError::Limit);
        }
        if nodes.is_null() || out.is_null() {
            return Err(CodecError::Invalid);
        }
        let nodes = unsafe { std::slice::from_raw_parts(nodes, count as usize) };
        let mut values = Vec::new();
        for n in nodes {
            if n.enabled > 1 || n.checked > 1 {
                return Err(CodecError::Invalid);
            }
            let kind =
                crate::ui::Kind::try_from(u16::try_from(n.kind).map_err(|_| CodecError::Invalid)?)
                    .map_err(|_| CodecError::Invalid)?;
            let tone =
                crate::ui::Tone::try_from(u16::try_from(n.tone).map_err(|_| CodecError::Invalid)?)
                    .map_err(|_| CodecError::Invalid)?;
            values.push(unsafe {
                crate::ui::Node {
                    id: read_text(n.id, 256)?,
                    parent: read_text(n.parent, 256)?,
                    label: read_text(n.label, 512)?,
                    text: read_text(n.text, 4096)?,
                    action: read_text(n.action, 256)?,
                    kind,
                    tone,
                    enabled: n.enabled == 1,
                    checked: n.checked == 1,
                    max_bytes: n.max_bytes,
                }
            });
        }
        let bytes = crate::ui::Document::new(values)?.encode()?;
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
/// Input bytes are readable, and output is a disjoint aligned writable pointer slot.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp_ui_event_decode(
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
        if length as usize > crate::ui::MAX_BYTES {
            return Err(CodecError::Limit);
        }
        if bytes.is_null() {
            return Err(CodecError::Invalid);
        }
        let value = crate::ui::Event::decode(unsafe {
            std::slice::from_raw_parts(bytes, length as usize)
        })?;
        unsafe {
            *out = Box::into_raw(Box::new(value)).cast();
        }
        Ok(())
    })
}
/// # Safety
/// Handle must come from mp_ui_event_decode and remain live. View is aligned,
/// writable and disjoint; returned spans expire when this handle is freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp_ui_event_get(
    handle: *const c_void,
    out: *mut UiEventView,
    size: u32,
) -> u32 {
    if out.is_null() || size != size_of::<UiEventView>() as u32 {
        return 16;
    }
    unsafe {
        *out = UiEventView::default();
    }
    guard(|| {
        if handle.is_null() {
            return Err(CodecError::Invalid);
        }
        let e = unsafe { &*handle.cast::<crate::ui::Event>() };
        unsafe {
            *out = UiEventView {
                view: span(e.view.as_bytes()),
                node: span(e.node.as_bytes()),
                action: span(e.action.as_bytes()),
                text: span(e.text.as_bytes()),
                generation: e.generation,
                revision: e.revision,
                serial: e.serial,
                kind: e.kind as u32,
                checked: e.checked as u32,
            };
        }
        Ok(())
    })
}
/// # Safety
/// Free a handle from mp_ui_event_decode exactly once, after all borrowed spans expire.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp_ui_event_free(handle: *mut c_void) {
    if !handle.is_null() {
        unsafe {
            drop(Box::from_raw(handle.cast::<crate::ui::Event>()));
        }
    }
}

/// # Safety
/// Input is readable; out/count are aligned, writable and disjoint. The returned
/// handle owns decoded values and must be freed after all borrowed spans expire.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp_ui_document_decode(
    bytes: *const u8,
    length: u32,
    out: *mut *mut c_void,
    count: *mut u32,
) -> u32 {
    if out.is_null() || count.is_null() {
        return 16;
    }
    unsafe {
        *out = std::ptr::null_mut();
        *count = 0;
    }
    guard(|| {
        if length as usize > crate::ui::MAX_BYTES {
            return Err(CodecError::Limit);
        }
        if bytes.is_null() {
            return Err(CodecError::Invalid);
        }
        // The public byte-pointer API also accepts unaligned source buffers.
        let mut words = capnp::Word::allocate_zeroed_vec((length as usize).div_ceil(8));
        let aligned = capnp::Word::words_to_bytes_mut(&mut words);
        aligned[..length as usize]
            .copy_from_slice(unsafe { std::slice::from_raw_parts(bytes, length as usize) });
        let value = crate::ui::Document::decode(&aligned[..length as usize])?;
        unsafe {
            *count = value.nodes().len() as u32;
            *out = Box::into_raw(Box::new(value)).cast();
        }
        Ok(())
    })
}
/// # Safety
/// Handle is live and created by mp_ui_document_decode. out is writable,
/// aligned and disjoint. Its spans borrow the handle, not the input buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp_ui_document_node(
    handle: *const c_void,
    index: u32,
    out: *mut UiNode,
    size: u32,
) -> u32 {
    if out.is_null() || size != size_of::<UiNode>() as u32 {
        return 16;
    }
    unsafe {
        *out = UiNode::default();
    }
    guard(|| {
        if handle.is_null() {
            return Err(CodecError::Invalid);
        }
        let d = unsafe { &*handle.cast::<crate::ui::Document>() };
        let n = d.nodes().get(index as usize).ok_or(CodecError::Invalid)?;
        unsafe {
            *out = UiNode {
                id: span(n.id.as_bytes()),
                parent: span(n.parent.as_bytes()),
                label: span(n.label.as_bytes()),
                text: span(n.text.as_bytes()),
                action: span(n.action.as_bytes()),
                kind: n.kind as u32,
                tone: n.tone as u32,
                enabled: n.enabled as u32,
                checked: n.checked as u32,
                max_bytes: n.max_bytes,
            };
        }
        Ok(())
    })
}
/// # Safety
/// Free a decoded document exactly once; no borrowed spans may outlive it.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp_ui_document_free(handle: *mut c_void) {
    if !handle.is_null() {
        unsafe {
            drop(Box::from_raw(handle.cast::<crate::ui::Document>()));
        }
    }
}

#[cfg(test)]
mod ui_tests {
    use super::*;
    #[test]
    fn document_readback_owns_data_and_rejects_invalid_indices_and_frames() {
        let mut n = crate::ui::Node::new("source", "root", crate::ui::Kind::TextInput);
        n.label = "正文".into();
        n.text = "你好🌱".into();
        n.action = "edit".into();
        n.max_bytes = 32;
        let mut bytes = crate::ui::Document::new(vec![
            crate::ui::Node::new("root", "", crate::ui::Kind::Column),
            n,
        ])
        .unwrap()
        .encode()
        .unwrap();
        let mut handle = std::ptr::null_mut();
        let mut count = 99;
        let mut out = UiNode::default();
        unsafe {
            assert_eq!(
                mp_ui_document_decode(bytes.as_ptr(), bytes.len() as u32, &mut handle, &mut count),
                0
            );
            assert_eq!(count, 2);
            let mut prefixed = vec![0];
            prefixed.extend(&bytes);
            let mut unaligned_handle = std::ptr::null_mut();
            let mut unaligned_count = 0;
            assert_eq!(
                mp_ui_document_decode(
                    prefixed.as_ptr().add(1),
                    bytes.len() as u32,
                    &mut unaligned_handle,
                    &mut unaligned_count
                ),
                0
            );
            assert_eq!(unaligned_count, 2);
            mp_ui_document_free(unaligned_handle);
            bytes.fill(0);
            assert_eq!(
                mp_ui_document_node(handle, 1, &mut out, size_of::<UiNode>() as u32),
                0
            );
            assert_eq!(
                std::slice::from_raw_parts(out.text.data, out.text.length as usize),
                "你好🌱".as_bytes()
            );
            assert_eq!(
                mp_ui_document_node(handle, 2, &mut out, size_of::<UiNode>() as u32),
                16
            );
            assert_eq!(out.text.length, 0);
            mp_ui_document_free(handle);
            assert_ne!(
                mp_ui_document_decode(bytes.as_ptr(), 1, &mut handle, &mut count),
                0
            );
            assert!(handle.is_null());
            assert_eq!(count, 0);
            assert_eq!(
                mp_ui_document_decode(bytes.as_ptr(), 65537, &mut handle, &mut count),
                17
            );
            mp_ui_document_free(handle);
        }
    }
    #[test]
    fn ui_encode_bounds_flags_and_failure_atomicity() {
        let node = UiNode {
            id: span(b"root"),
            enabled: 1,
            ..Default::default()
        };
        let mut bytes = [0xabu8; 65536];
        let mut length = 999;
        unsafe {
            assert_eq!(
                mp_ui_document_encode(
                    1,
                    size_of::<UiNode>() as u32,
                    &node,
                    1,
                    bytes.as_mut_ptr(),
                    1,
                    &mut length
                ),
                17
            );
            assert_eq!(length, 0);
            assert!(bytes.iter().all(|b| *b == 0xab));
            assert_eq!(
                mp_ui_document_encode(1, 0, &node, 1, bytes.as_mut_ptr(), 65536, &mut length),
                18
            );
            assert_eq!(
                mp_ui_document_encode(
                    1,
                    size_of::<UiNode>() as u32,
                    std::ptr::null(),
                    129,
                    bytes.as_mut_ptr(),
                    65536,
                    &mut length
                ),
                17
            );
            for bad in [
                UiNode { enabled: 2, ..node },
                UiNode { checked: 2, ..node },
                UiNode {
                    kind: 65536,
                    ..node
                },
                UiNode {
                    tone: 65536,
                    ..node
                },
                UiNode {
                    id: Span {
                        data: std::ptr::null(),
                        length: 1,
                    },
                    ..node
                },
            ] {
                assert_eq!(
                    mp_ui_document_encode(
                        1,
                        size_of::<UiNode>() as u32,
                        &bad,
                        1,
                        bytes.as_mut_ptr(),
                        65536,
                        &mut length
                    ),
                    16
                );
                assert_eq!(length, 0);
            }
            assert_eq!(
                mp_ui_document_encode(
                    1,
                    size_of::<UiNode>() as u32,
                    &node,
                    1,
                    bytes.as_mut_ptr(),
                    65536,
                    &mut length
                ),
                0
            );
        }
        assert_eq!(
            crate::ui::Document::decode(&bytes[..length as usize])
                .unwrap()
                .nodes()
                .len(),
            1
        );
    }
    #[test]
    fn ui_event_ownership_and_exact_u64() {
        let bytes = include_bytes!("../../tests/ui_fixtures/event.capnp");
        let mut handle = std::ptr::null_mut();
        unsafe {
            assert_eq!(
                mp_ui_event_decode(bytes.as_ptr(), bytes.len() as u32, &mut handle),
                0
            );
            assert!(!handle.is_null());
            let mut view = UiEventView::default();
            assert_eq!(
                mp_ui_event_get(handle, &mut view, size_of::<UiEventView>() as u32),
                0
            );
            assert_eq!(view.generation, u64::MAX);
            assert_eq!((view.revision, view.serial), (1, 1));
            assert_eq!(read_text(view.text, 4096).unwrap(), "从 Dart 编辑🌈");
            mp_ui_event_free(handle);
            handle = std::ptr::dangling_mut();
            assert_eq!(mp_ui_event_decode(bytes.as_ptr(), 1, &mut handle), 16);
            assert!(handle.is_null());
            assert_eq!(
                mp_ui_event_get(handle, &mut view, size_of::<UiEventView>() as u32),
                16
            );
            assert_eq!(view.generation, 0);
            assert!(view.text.data.is_null());
            mp_ui_event_free(handle);
        }
    }
}

/// Independent additive ABI: existing request and task structures retain their layouts.
#[repr(C)]
#[derive(Default)]
pub struct CContentRequest {
    abi_version: u32,
    struct_size: u32,
    kind: u32,
    format_version: u32,
    request_id: Span,
    card_id: Span,
    type_id: Span,
    title: Span,
    body: Span,
    preview: Span,
    revision: u64,
    offset: u64,
    length: u32,
}
unsafe fn content_request(raw: *const CContentRequest) -> Result<Request, CodecError> {
    if raw.is_null() {
        return Err(CodecError::Invalid);
    }
    let v = unsafe { &*raw };
    if v.abi_version != 1 || v.struct_size < size_of::<CContentRequest>() as u32 {
        return Err(CodecError::Contract);
    }
    let r = unsafe {
        Request {
            request_id: read_text(v.request_id, 256)?,
            card_id: read_text(v.card_id, 256)?,
            action: match v.kind {
                1 | 2 => {
                    if v.body.length > 32768 {
                        return Err(CodecError::Limit);
                    }
                    if v.body.length != 0 && v.body.data.is_null() {
                        return Err(CodecError::Invalid);
                    }
                    let body = if v.body.length == 0 {
                        Vec::new()
                    } else {
                        std::slice::from_raw_parts(v.body.data, v.body.length as usize).to_vec()
                    };
                    let title = read_text(v.title, 16384)?;
                    if v.kind == 1 {
                        Action::CreateContent {
                            type_id: read_text(v.type_id, 256)?,
                            format_version: v.format_version,
                            title,
                            body,
                        }
                    } else {
                        Action::EditContent {
                            revision: v.revision,
                            title,
                            body,
                            preview: read_text(v.preview, 16384)?,
                        }
                    }
                }
                3 => Action::ReadContent {
                    revision: v.revision,
                    offset: v.offset,
                    length: v.length,
                },
                _ => return Err(CodecError::Invalid),
            },
        }
    };
    r.validate()?;
    Ok(r)
}
/// # Safety
/// All pointers are aligned, disjoint and valid for their declared lengths for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp_content_request_encode(
    raw: *const CContentRequest,
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
        let bytes = unsafe { content_request(raw) }?.encode()?;
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
/// Inputs are readable and aligned; out is a disjoint writable handle slot.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp_content_reply_decode(
    bytes: *const u8,
    length: u32,
    raw: *const CContentRequest,
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
        let request = unsafe { content_request(raw) }?;
        let reply =
            request.decode_reply(unsafe { std::slice::from_raw_parts(bytes, length as usize) })?;
        unsafe {
            *out = Box::into_raw(Box::new(Handle {
                request_id: request.request_id,
                reply,
            }))
            .cast();
        }
        Ok(())
    })
}
/// # Safety
/// raw is a live SDK task and out is a disjoint aligned writable view.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp_task_get_content(
    raw: *const c_void,
    out: *mut CContentRequest,
    size: u32,
) -> u32 {
    guard(|| {
        if raw.is_null() || out.is_null() {
            return Err(CodecError::Invalid);
        }
        if size < size_of::<CContentRequest>() as u32 {
            return Err(CodecError::Limit);
        }
        let task = unsafe { &*raw.cast::<crate::task::Invocation>() };
        let r = task.request().ok_or(CodecError::Invalid)?;
        let mut v = CContentRequest {
            abi_version: 1,
            struct_size: size_of::<CContentRequest>() as u32,
            request_id: span(r.request_id.as_bytes()),
            card_id: span(r.card_id.as_bytes()),
            ..Default::default()
        };
        match &r.action {
            Action::CreateContent {
                type_id,
                format_version,
                title,
                body,
            } => {
                v.kind = 1;
                v.type_id = span(type_id.as_bytes());
                v.format_version = *format_version;
                v.title = span(title.as_bytes());
                v.body = span(body);
            }
            Action::EditContent {
                revision,
                title,
                body,
                preview,
            } => {
                v.kind = 2;
                v.revision = *revision;
                v.title = span(title.as_bytes());
                v.body = span(body);
                v.preview = span(preview.as_bytes());
            }
            Action::ReadContent {
                revision,
                offset,
                length,
            } => {
                v.kind = 3;
                v.revision = *revision;
                v.offset = *offset;
                v.length = *length;
            }
            _ => return Err(CodecError::Contract),
        }
        unsafe {
            out.write(v);
        }
        Ok(())
    })
}

/// # Safety
/// raw is a live SDK task; out is disjoint, aligned and writable. The returned span borrows the task.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp_task_get_command(raw: *const c_void, out: *mut Span, size: u32) -> u32 {
    guard(|| {
        if raw.is_null() || out.is_null() {
            return Err(CodecError::Invalid);
        }
        if size < size_of::<Span>() as u32 {
            return Err(CodecError::Limit);
        }
        let task = unsafe { &*raw.cast::<crate::task::Invocation>() };
        task.request().ok_or(CodecError::Contract)?;
        unsafe {
            out.write(span(task.command_bytes()));
        }
        Ok(())
    })
}
