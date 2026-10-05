//! Native callers supply valid readable/writable memory; pointer checks do not
//! create a sandbox. Successful outputs own every field and retain no pointers.
use crate::{
    Error, Event, EventRef, MAX_ENVELOPE_BYTES, MAX_FIELD_BYTES, SCHEMA_DIGEST, check_lengths,
};
#[repr(C)]
pub struct CEvent {
    pub data_offset: u32,
    pub data_length: u32,
    pub event_offset: u32,
    pub event_length: u32,
    pub id_offset: u32,
    pub id_length: u32,
    pub has_retry: u32,
    pub reserved: u32,
    pub retry: u64,
    pub storage: [u8; MAX_FIELD_BYTES],
}
fn output_valid(out: *mut CEvent, size: u32) -> bool {
    !out.is_null() && out.is_aligned() && size as usize >= std::mem::size_of::<CEvent>()
}
fn fields<'a>(
    data: &'a [u8],
    event: &'a [u8],
    id: &'a [u8],
    has: u32,
    retry: u64,
) -> crate::Result<EventRef<'a>> {
    if has > 1 || (has == 0 && retry != 0) {
        return Err(Error::Invalid);
    }
    check_lengths(data.len(), event.len(), id.len())?;
    Ok(EventRef {
        data: std::str::from_utf8(data).map_err(|_| Error::Utf8)?,
        event: std::str::from_utf8(event).map_err(|_| Error::Utf8)?,
        id: std::str::from_utf8(id).map_err(|_| Error::Utf8)?,
        retry: (has == 1).then_some(retry),
    })
}
fn owned(value: EventRef<'_>) -> CEvent {
    let mut out = CEvent {
        data_offset: 0,
        data_length: value.data.len() as u32,
        event_offset: value.data.len() as u32,
        event_length: value.event.len() as u32,
        id_offset: (value.data.len() + value.event.len()) as u32,
        id_length: value.id.len() as u32,
        has_retry: u32::from(value.retry.is_some()),
        reserved: 0,
        retry: value.retry.unwrap_or(0),
        storage: [0; MAX_FIELD_BYTES],
    };
    let end = value.data.len();
    out.storage[..end].copy_from_slice(value.data.as_bytes());
    let next = end + value.event.len();
    out.storage[end..next].copy_from_slice(value.event.as_bytes());
    out.storage[next..next + value.id.len()].copy_from_slice(value.id.as_bytes());
    out
}
unsafe fn input<'a>(ptr: *const u8, length: u32) -> &'a [u8] {
    if length == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(ptr, length as usize) }
    }
}
fn overlap(a: *const u8, alen: usize, b: *const u8, blen: usize) -> Option<bool> {
    let a = a as usize;
    let b = b as usize;
    Some(a < b.checked_add(blen)? && b < a.checked_add(alen)?)
}
/// # Safety
/// Bytes must be readable for length. Out must be aligned/writable for CEvent;
/// out_size must cover CEvent. Input/output may alias: input is owned before the
/// output write. Every error leaves all output bytes unchanged; success zeroes
/// unused storage and retains no borrowed pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mse_event_v1_decode(
    bytes: *const u8,
    length: u32,
    out: *mut CEvent,
    out_size: u32,
) -> u32 {
    if !output_valid(out, out_size) || bytes.is_null() {
        return Error::Invalid as u32;
    }
    if length == 0 || length as usize > MAX_ENVELOPE_BYTES {
        return Error::Limit as u32;
    }
    match Event::decode(unsafe { input(bytes, length) }) {
        Ok(value) => {
            let copy = owned(value.as_ref());
            unsafe {
                out.write(copy);
            }
            0
        }
        Err(e) => e as u32,
    }
}
/// # Safety
/// Each field pointer must be readable for its length, with null allowed only
/// for zero length. Output/alias/error/ownership contracts are those of decode.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mse_event_v1_set(
    data: *const u8,
    data_length: u32,
    event: *const u8,
    event_length: u32,
    id: *const u8,
    id_length: u32,
    has_retry: u32,
    retry: u64,
    out: *mut CEvent,
    out_size: u32,
) -> u32 {
    if !output_valid(out, out_size)
        || (data.is_null() && data_length != 0)
        || (event.is_null() && event_length != 0)
        || (id.is_null() && id_length != 0)
    {
        return Error::Invalid as u32;
    }
    if let Err(e) = check_lengths(
        data_length as usize,
        event_length as usize,
        id_length as usize,
    ) {
        return e as u32;
    }
    match fields(
        unsafe { input(data, data_length) },
        unsafe { input(event, event_length) },
        unsafe { input(id, id_length) },
        has_retry,
        retry,
    ) {
        Ok(value) => {
            let copy = owned(value);
            unsafe {
                out.write(copy);
            }
            0
        }
        Err(e) => e as u32,
    }
}
fn span(value: &CEvent, offset: u32, length: u32) -> crate::Result<&[u8]> {
    let start = offset as usize;
    let end = start
        .checked_add(length as usize)
        .filter(|end| *end <= MAX_FIELD_BYTES)
        .ok_or(Error::Invalid)?;
    Ok(&value.storage[start..end])
}
/// # Safety
/// Event must be aligned/readable for CEvent and event_size must cover it. Out
/// must be writable for capacity bytes; out_length aligned/writable for u32.
/// Input/output may alias: wire is fully encoded before output writes. The two
/// written outputs must not overlap (checked). Every error preserves both output
/// bytes and length; success writes only the encoded prefix and its length.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mse_event_v1_encode(
    event: *const CEvent,
    event_size: u32,
    out: *mut u8,
    capacity: u32,
    out_length: *mut u32,
) -> u32 {
    if event.is_null()
        || !event.is_aligned()
        || (event_size as usize) < std::mem::size_of::<CEvent>()
        || out.is_null()
        || out_length.is_null()
        || !out_length.is_aligned()
    {
        return Error::Invalid as u32;
    }
    let value = unsafe { &*event };
    if value.reserved != 0 {
        return Error::Invalid as u32;
    }
    let encoded = (|| {
        check_lengths(
            value.data_length as usize,
            value.event_length as usize,
            value.id_length as usize,
        )?;
        fields(
            span(value, value.data_offset, value.data_length)?,
            span(value, value.event_offset, value.event_length)?,
            span(value, value.id_offset, value.id_length)?,
            value.has_retry,
            value.retry,
        )?
        .encode()
    })();
    let bytes = match encoded {
        Ok(v) => v,
        Err(e) => return e as u32,
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
/// Out must be writable for capacity bytes. Success writes exactly32 bytes.
/// Null or insufficient capacity leaves output unchanged; no pointer retained.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mse_event_v1_schema_digest(out: *mut u8, capacity: u32) -> u32 {
    if out.is_null() {
        return Error::Invalid as u32;
    }
    if capacity < 32 {
        return Error::Buffer as u32;
    }
    unsafe {
        std::ptr::copy_nonoverlapping(SCHEMA_DIGEST.as_ptr(), out, 32);
    }
    0
}
#[cfg(test)]
mod tests {
    use super::*;
    fn empty() -> Box<CEvent> {
        Box::new(owned(EventRef {
            data: "",
            event: "",
            id: "",
            retry: None,
        }))
    }
    const SIZE: u32 = std::mem::size_of::<CEvent>() as u32;
    #[test]
    fn owned_aliased_decode_and_set_zero_tail() {
        assert_eq!(SIZE, 65576);
        let mut value = empty();
        let raw = EventRef {
            data: "雪",
            event: "evt",
            id: "id\0",
            retry: Some(u64::MAX),
        }
        .encode()
        .unwrap();
        value.storage[..raw.len()].copy_from_slice(&raw);
        let ptr: *mut CEvent = &mut *value;
        let input = unsafe { (*ptr).storage.as_ptr() };
        assert_eq!(
            unsafe { mse_event_v1_decode(input, raw.len() as u32, ptr, SIZE) },
            0
        );
        assert_eq!(&value.storage[..3], "雪".as_bytes());
        assert_eq!(value.retry, u64::MAX);
        assert!(value.storage[9..].iter().all(|b| *b == 0));
        let ptr: *mut CEvent = &mut *value;
        let input = unsafe { (*ptr).storage.as_ptr() };
        assert_eq!(
            unsafe {
                mse_event_v1_set(
                    input,
                    3,
                    std::ptr::null(),
                    0,
                    std::ptr::null(),
                    0,
                    0,
                    0,
                    ptr,
                    SIZE,
                )
            },
            0
        );
        assert_eq!(&value.storage[..3], "雪".as_bytes());
        assert!(value.storage[3..].iter().all(|b| *b == 0));
    }
    #[test]
    fn rejection_preserves_outputs_and_bounds_before_field_access() {
        let mut value = empty();
        value.storage.fill(0xa5);
        assert_eq!(
            unsafe {
                mse_event_v1_set(
                    std::ptr::dangling(),
                    65536,
                    std::ptr::dangling(),
                    1,
                    std::ptr::null(),
                    0,
                    0,
                    0,
                    &mut *value,
                    SIZE,
                )
            },
            Error::Limit as u32
        );
        assert!(value.storage.iter().all(|b| *b == 0xa5));
        let mut bytes = [0xa5u8; 512];
        let mut length = 0x12345678;
        value.data_offset = u32::MAX;
        value.data_length = 1;
        assert_eq!(
            unsafe { mse_event_v1_encode(&*value, SIZE, bytes.as_mut_ptr(), 512, &mut length) },
            Error::Invalid as u32
        );
        assert_eq!(length, 0x12345678);
        assert!(bytes.iter().all(|b| *b == 0xa5));
        value.data_offset = 0;
        value.data_length = 0;
        assert_eq!(
            unsafe { mse_event_v1_encode(&*value, SIZE, bytes.as_mut_ptr(), 1, &mut length) },
            Error::Buffer as u32
        );
        assert_eq!(length, 0x12345678);
        assert!(bytes.iter().all(|b| *b == 0xa5));
    }
    #[test]
    fn overlapping_encode_outputs_reject_without_write_and_shared_fields_count() {
        let mut value = empty();
        let mut bytes = [0xa5a5a5a5u32; 512];
        let length = bytes.as_mut_ptr();
        assert_eq!(
            unsafe { mse_event_v1_encode(&*value, SIZE, length.cast(), 2048, length) },
            Error::Invalid as u32
        );
        assert!(bytes.iter().all(|v| *v == 0xa5a5a5a5));
        value.data_length = 32768;
        value.event_length = 32768;
        value.id_length = 1;
        assert_eq!(
            unsafe { mse_event_v1_encode(&*value, SIZE, length.cast(), 2048, length) },
            Error::Limit as u32
        );
    }
}
