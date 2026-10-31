//! Additive adapter for the existing Task envelope, without a base-SDK ABI change.
//! The original serialized invocation supplies the independently decoded task ID.
//! A match correlates data; it does not authenticate its sender or grant disclosure.
#![deny(unsafe_op_in_unsafe_fn)]
use morrow_blob_upload_history_v1::{
    Error, INPUT_TYPE, MAX_ID_BYTES, Packet, Result, ffi::MbhPacket,
};
use morrow_plugin_sdk::{protocol::CodecError, task::Invocation};
use std::{
    mem::{align_of, size_of},
    panic::{AssertUnwindSafe, catch_unwind},
};

pub const MAX_TASK_BYTES: usize = morrow_plugin_sdk::task::MAX_TASK_BYTES;

fn identity(value: &str) -> Result<()> {
    if value.len() > MAX_ID_BYTES {
        return Err(Error::Limit);
    }
    if value.is_empty()
        || value
            .chars()
            .any(|c| c.is_control() || matches!(c, '/' | '\\' | ':'))
    {
        return Err(Error::Invalid);
    }
    Ok(())
}

fn invocation(bytes: &[u8], handler: &str, output_type: &str) -> Result<Invocation> {
    identity(handler)?;
    identity(output_type)?;
    if bytes.is_empty() || bytes.len() > MAX_TASK_BYTES {
        return Err(Error::Limit);
    }
    let task = Invocation::decode(bytes).map_err(|e| match e {
        CodecError::Contract => Error::Contract,
        CodecError::Limit => Error::Limit,
        _ => Error::Invalid,
    })?;
    let input = task.transform().ok_or(Error::Contract)?;
    if input.handler != handler
        || input.input_type != INPUT_TYPE
        || input.output_type != output_type
    {
        return Err(Error::Contract);
    }
    Ok(task)
}

/// Decode the original Task bytes and require the receiving plugin's declared
/// handler/output type. Never derive the expected task ID from the history packet.
pub fn decode_invocation(bytes: &[u8], handler: &str, output_type: &str) -> Result<Packet> {
    let task = invocation(bytes, handler, output_type)?;
    Packet::decode_task(
        &task.transform().ok_or(Error::Contract)?.input,
        task.task_id(),
    )
}

fn region(pointer: usize, length: usize) -> Result<(usize, usize)> {
    Ok((pointer, pointer.checked_add(length).ok_or(Error::Buffer)?))
}

/// Additive C entry point. The base SDK's opaque `mp_task` is not cast or extended.
/// Returned owners use the existing `mbh_packet_view` / `mbh_packet_free` API.
/// Reported failures preserve the output slot and any previously owned packet.
///
/// # Safety
/// Each input addresses its advertised readable bytes for this call. `out` is
/// aligned, exclusively writable, and disjoint from all three input ranges.
/// Expected handler/output type come from the receiving plugin's declaration.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbh_packet_decode_invocation(
    bytes: *const u8,
    length: u32,
    handler: *const u8,
    handler_length: u32,
    output_type: *const u8,
    output_type_length: u32,
    out: *mut *mut MbhPacket,
) -> u32 {
    let result = catch_unwind(AssertUnwindSafe(|| -> Result<u32> {
        if bytes.is_null()
            || handler.is_null()
            || output_type.is_null()
            || out.is_null()
            || !out.addr().is_multiple_of(align_of::<*mut MbhPacket>())
        {
            return Err(Error::Buffer);
        }
        if length == 0
            || length as usize > MAX_TASK_BYTES
            || handler_length as usize > MAX_ID_BYTES
            || output_type_length as usize > MAX_ID_BYTES
        {
            return Err(Error::Limit);
        }
        let inputs = [
            region(bytes.addr(), length as usize)?,
            region(handler.addr(), handler_length as usize)?,
            region(output_type.addr(), output_type_length as usize)?,
        ];
        let output = region(out.addr(), size_of::<*mut MbhPacket>())?;
        if inputs.iter().any(|r| output.0 < r.1 && r.0 < output.1) {
            return Err(Error::Buffer);
        }
        let wire = unsafe { std::slice::from_raw_parts(bytes, length as usize) };
        let handler = std::str::from_utf8(unsafe {
            std::slice::from_raw_parts(handler, handler_length as usize)
        })
        .map_err(|_| Error::Invalid)?;
        let output_type = std::str::from_utf8(unsafe {
            std::slice::from_raw_parts(output_type, output_type_length as usize)
        })
        .map_err(|_| Error::Invalid)?;
        let task = invocation(wire, handler, output_type)?;
        let input = &task.transform().ok_or(Error::Contract)?.input;
        // The same original decoded Task owns both independent inputs until
        // this call returns. The existing codec owns allocation and release.
        Ok(unsafe {
            morrow_blob_upload_history_v1::ffi::mbh_packet_decode_task(
                input.as_ptr(),
                input.len() as u32,
                task.task_id().as_ptr(),
                task.task_id().len() as u32,
                out,
            )
        })
    }));
    match result {
        Ok(Ok(code)) => code,
        Ok(Err(error)) => error as u32,
        Err(_) => Error::Invalid as u32,
    }
}

#[cfg(test)]
mod tests;
