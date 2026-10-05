//! C functions use caller-owned results and retain no input or output pointers.
use crate::{
    Error, MAX_ENVELOPE_BYTES, MAX_PAYLOAD_BYTES, Message, MessageKind, MessageRef, SCHEMA_DIGEST,
};
#[repr(C)]
pub struct CMessage {
    pub kind: u32,
    pub has_close_code: u32,
    pub payload_length: u32,
    pub close_code: u16,
    pub reserved: u16,
    pub payload: [u8; MAX_PAYLOAD_BYTES],
}
fn output_valid(out: *mut CMessage, out_size: u32) -> bool {
    !out.is_null() && out.is_aligned() && out_size as usize >= std::mem::size_of::<CMessage>()
}
fn fields(kind: u32, payload: &[u8], has: u32, code: u16) -> crate::Result<MessageRef<'_>> {
    if has > 1 || has == 0 && code != 0 {
        return Err(Error::Invalid);
    }
    let value = MessageRef {
        kind: MessageKind::try_from(kind)?,
        payload,
        close_code: (has == 1).then_some(code),
    };
    value.validate()?;
    Ok(value)
}
fn owned(value: MessageRef<'_>) -> CMessage {
    let mut out = CMessage {
        kind: value.kind as u32,
        has_close_code: u32::from(value.close_code.is_some()),
        payload_length: value.payload.len() as u32,
        close_code: value.close_code.unwrap_or(0),
        reserved: 0,
        payload: [0; MAX_PAYLOAD_BYTES],
    };
    out.payload[..value.payload.len()].copy_from_slice(value.payload);
    out
}
fn overlap(a: *const u8, a_length: usize, b: *const u8, b_length: usize) -> Option<bool> {
    let start_a = a as usize;
    let end_a = start_a.checked_add(a_length)?;
    let start_b = b as usize;
    let end_b = start_b.checked_add(b_length)?;
    Some(start_a < end_b && start_b < end_a)
}
/// # Safety
/// Input must be readable for `length` bytes. Output must be aligned and writable
/// for sizeof(CMessage); out_size must be at least that size. Inputs/output may
/// alias: all input is consumed before writing the owned output. Error changes
/// no output bytes; success retains no pointer and owns all decoded payload bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mws_message_v1_decode(
    bytes: *const u8,
    length: u32,
    out: *mut CMessage,
    out_size: u32,
) -> u32 {
    if !output_valid(out, out_size) || bytes.is_null() {
        return Error::Invalid as u32;
    }
    if length == 0 || length as usize > MAX_ENVELOPE_BYTES {
        return Error::Limit as u32;
    }
    let input = unsafe { std::slice::from_raw_parts(bytes, length as usize) };
    match Message::decode(input) {
        Ok(value) => {
            let copy = owned(value.as_ref());
            unsafe {
                out.write(copy);
            }
            0
        }
        Err(error) => error as u32,
    }
}
/// # Safety
/// Payload must be readable for length bytes; null is permitted only for length0.
/// Output pointer/size and alias contracts are the same as decode. Error leaves
/// output unchanged. Reserved output is zero; tail payload bytes are zeroed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mws_message_v1_set(
    kind: u32,
    payload: *const u8,
    length: u32,
    has_close_code: u32,
    close_code: u16,
    out: *mut CMessage,
    out_size: u32,
) -> u32 {
    if !output_valid(out, out_size) || (payload.is_null() && length != 0) {
        return Error::Invalid as u32;
    }
    if length as usize > MAX_PAYLOAD_BYTES {
        return Error::Limit as u32;
    }
    let input = if length == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(payload, length as usize) }
    };
    match fields(kind, input, has_close_code, close_code) {
        Ok(value) => {
            let copy = owned(value);
            unsafe {
                out.write(copy);
            }
            0
        }
        Err(error) => error as u32,
    }
}
/// # Safety
/// Message must be aligned/readable for sizeof(CMessage), with message_size at
/// least that size. Output must be writable for capacity bytes; out_length must
/// be aligned/writable for a u32. Input/output may alias because encoding consumes
/// the input first. The two written outputs must not overlap (checked). Error
/// leaves BOTH output bytes and output length unchanged. Success writes only the
/// encoded prefix and its length; unused output capacity remains unchanged.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mws_message_v1_encode(
    message: *const CMessage,
    message_size: u32,
    out: *mut u8,
    capacity: u32,
    out_length: *mut u32,
) -> u32 {
    if message.is_null()
        || !message.is_aligned()
        || (message_size as usize) < std::mem::size_of::<CMessage>()
        || out.is_null()
        || out_length.is_null()
        || !out_length.is_aligned()
    {
        return Error::Invalid as u32;
    }
    let message = unsafe { &*message };
    if message.reserved != 0 {
        return Error::Invalid as u32;
    }
    if message.payload_length as usize > MAX_PAYLOAD_BYTES {
        return Error::Limit as u32;
    }
    let result = fields(
        message.kind,
        &message.payload[..message.payload_length as usize],
        message.has_close_code,
        message.close_code,
    )
    .and_then(|value| value.encode());
    let bytes = match result {
        Ok(bytes) => bytes,
        Err(error) => return error as u32,
    };
    if (capacity as usize) < bytes.len() {
        return Error::Buffer as u32;
    }
    if overlap(
        out,
        bytes.len(),
        out_length.cast(),
        std::mem::size_of::<u32>(),
    ) != Some(false)
    {
        return Error::Invalid as u32;
    }
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), out, bytes.len());
        out_length.write(bytes.len() as u32);
    }
    0
}
/// # Safety
/// Output must be writable for capacity bytes. Writes exactly32 bytes only on
/// success; capacity<32 or null leaves output unchanged. No pointer is retained.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mws_message_v1_schema_digest(out: *mut u8, capacity: u32) -> u32 {
    if out.is_null() {
        return Error::Invalid as u32;
    }
    if capacity < 32 {
        return Error::Buffer as u32;
    }
    unsafe {
        std::ptr::copy_nonoverlapping(SCHEMA_DIGEST.as_ptr(), out, SCHEMA_DIGEST.len());
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ffi_rejections_leave_owned_outputs_and_encode_length_unchanged() {
        let mut value = Box::new(CMessage {
            kind: 1,
            has_close_code: 0,
            payload_length: 0,
            close_code: 0,
            reserved: 0,
            payload: [0xa5; MAX_PAYLOAD_BYTES],
        });
        let code = unsafe {
            mws_message_v1_decode(
                std::ptr::null(),
                0,
                &mut *value,
                std::mem::size_of::<CMessage>() as u32,
            )
        };
        assert_eq!(code, Error::Invalid as u32);
        assert!(value.payload.iter().all(|b| *b == 0xa5));
        let mut out = [0xa5u8; 64];
        let mut length = 0x12345678;
        let code = unsafe {
            mws_message_v1_encode(
                &*value,
                std::mem::size_of::<CMessage>() as u32,
                out.as_mut_ptr(),
                out.len() as u32,
                &mut length,
            )
        };
        assert_eq!(code, Error::Buffer as u32);
        assert!(out.iter().all(|b| *b == 0xa5));
        assert_eq!(length, 0x12345678);
    }
    #[test]
    fn null_empty_payload_and_input_output_aliasing_are_defined() {
        let mut value = Box::new(CMessage {
            kind: 0,
            has_close_code: 0,
            payload_length: 0,
            close_code: 0,
            reserved: 0,
            payload: [0; MAX_PAYLOAD_BYTES],
        });
        assert_eq!(
            unsafe {
                mws_message_v1_set(
                    1,
                    std::ptr::null(),
                    0,
                    0,
                    0,
                    &mut *value,
                    std::mem::size_of::<CMessage>() as u32,
                )
            },
            0
        );
        let raw = MessageRef {
            kind: MessageKind::Text,
            payload: b"independent-owned-bytes",
            close_code: None,
        }
        .encode()
        .unwrap();
        value.payload[..raw.len()].copy_from_slice(&raw);
        let ptr: *mut CMessage = &mut *value;
        let input = unsafe { (*ptr).payload.as_ptr() };
        assert_eq!(
            unsafe {
                mws_message_v1_decode(
                    input,
                    raw.len() as u32,
                    ptr,
                    std::mem::size_of::<CMessage>() as u32,
                )
            },
            0
        );
        assert_eq!(
            &value.payload[..value.payload_length as usize],
            b"independent-owned-bytes"
        );
    }
    #[test]
    fn overlapping_encoded_outputs_reject_before_either_write() {
        let value = Box::new(CMessage {
            kind: 1,
            has_close_code: 0,
            payload_length: 0,
            close_code: 0,
            reserved: 0,
            payload: [0; MAX_PAYLOAD_BYTES],
        });
        let mut out = [0xa5a5a5a5u32; 64];
        let length = out.as_mut_ptr();
        let bytes = length.cast();
        assert_eq!(
            unsafe {
                mws_message_v1_encode(
                    &*value,
                    std::mem::size_of::<CMessage>() as u32,
                    bytes,
                    256,
                    length,
                )
            },
            Error::Invalid as u32
        );
        assert!(out.iter().all(|v| *v == 0xa5a5a5a5));
    }
}
