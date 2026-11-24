//! Independent experimental metadata codec. No Store/Core linkage, IO or grants.
//! A decoded identity or digest never supplies source approval or content rights.
use sha2::{Digest, Sha256};
// Reference the transport crate so the combined staticlib includes its C exports.
#[cfg(feature = "c-transport")]
pub use morrow_plugin_sdk as transport;
pub const FEATURE: &str = "changes-metadata-v1";
pub const VERSION: u16 = 1;
pub const MAX_BYTES: usize = 662;
pub const HEADER_BYTES: usize = 150;
pub const MAX_ID_BYTES: usize = 256;
pub const CONTRACT: &[u8] = include_bytes!("../contracts/changes_metadata_v1.wire");
pub const PROFILE_DIGEST: [u8; 32] = [7,252,13,196,196,142,177,127,133,48,146,7,164,65,214,160,127,63,184,224,106,171,94,44,227,55,164,179,201,143,144,137];
const CURSOR_DOMAIN: &[u8] = b"Morrow/changes-metadata/cursor/v1\0";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Error { Invalid = 1, Contract = 2, Correlation = 3, Limit = 4 }

/// Position of an acknowledged card change in the Windows workbench.
/// The original library checks the ACK and current selection when resuming.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct Bookmark {
    pub window_id: [u8; 32],
    pub sequence: u64,
    pub metadata_cursor: [u8; 32],
}

impl Bookmark {
    pub const BYTES: usize = 73;

    fn validate(&self) -> Result<(), Error> {
        if self.sequence == 0 || self.window_id == [0; 32] || self.metadata_cursor == [0; 32] {
            return Err(Error::Invalid);
        }
        Ok(())
    }

    /// Encode the window, frame sequence and metadata cursor after its ACK.
    pub fn encode(&self) -> Result<[u8; Self::BYTES], Error> {
        self.validate()?;
        let mut wire = [0; Self::BYTES];
        wire[0] = 1;
        wire[1..33].copy_from_slice(&self.window_id);
        wire[33..41].copy_from_slice(&self.sequence.to_le_bytes());
        wire[41..].copy_from_slice(&self.metadata_cursor);
        Ok(wire)
    }

    pub fn decode(wire: &[u8]) -> Result<Self, Error> {
        let wire: &[u8; Self::BYTES] = wire.try_into().map_err(|_| Error::Invalid)?;
        if wire[0] != 1 {
            return Err(Error::Contract);
        }
        let value = Self {
            window_id: wire[1..33].try_into().unwrap(),
            sequence: u64::from_le_bytes(wire[33..41].try_into().unwrap()),
            metadata_cursor: wire[41..].try_into().unwrap(),
        };
        value.validate()?;
        Ok(value)
    }
}

/// # Safety
/// `wire` must be readable for `length` bytes and `out` aligned and writable
/// for a Bookmark. Only a successful call writes to the output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mc_metadata_v1_bookmark_decode(
    wire: *const u8,
    length: u32,
    out: *mut Bookmark,
    out_size: u32,
) -> u32 {
    if wire.is_null()
        || length as usize != Bookmark::BYTES
        || out.is_null()
        || (out_size as usize) < std::mem::size_of::<Bookmark>()
    {
        return Error::Invalid as u32;
    }
    let bytes = unsafe { std::slice::from_raw_parts(wire, Bookmark::BYTES) };
    match Bookmark::decode(bytes) {
        Ok(value) => {
            unsafe { out.write(value) };
            0
        }
        Err(error) => error as u32,
    }
}

/// # Safety
/// `value` must be aligned and readable for a Bookmark. `out` must be
/// writable for `capacity` bytes. Success writes exactly Bookmark::BYTES.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mc_metadata_v1_bookmark_encode(
    value: *const Bookmark,
    out: *mut u8,
    capacity: u32,
) -> u32 {
    if value.is_null() || out.is_null() || (capacity as usize) < Bookmark::BYTES {
        return Error::Invalid as u32;
    }
    match unsafe { value.read() }.encode() {
        Ok(wire) => {
            unsafe { std::ptr::copy_nonoverlapping(wire.as_ptr(), out, wire.len()) };
            0
        }
        Err(error) => error as u32,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Metadata<'a> {
    pub scope_digest: [u8; 32],
    pub window_id: [u8; 32],
    pub revision: u64,
    /// SHA256 of the whole Card.encode(), not the body digest.
    pub card_sha256: [u8; 32],
    pub card_id: &'a str,
    pub operation_id: &'a str,
}
fn identity(bytes: &[u8]) -> Result<&str, Error> {
    if bytes.is_empty() || bytes.len() > MAX_ID_BYTES { return Err(Error::Limit); }
    let text = std::str::from_utf8(bytes).map_err(|_| Error::Invalid)?;
    if text.chars().any(|c| c.is_control() || matches!(c, '/' | '\\' | ':')) { return Err(Error::Invalid); }
    Ok(text)
}
/// Strict borrowed decode, bounded by MAX_BYTES and without allocation.
/// The cursor and epoch are required: payload-only acceptance is not public API.
pub fn decode<'a>(bytes: &'a [u8], epoch: &[u8; 32], cursor: &[u8]) -> Result<Metadata<'a>, Error> {
    if bytes.len() < HEADER_BYTES || bytes.len() > MAX_BYTES { return Err(Error::Limit); }
    if &bytes[..8] != b"MRCHG001" || u16::from_le_bytes(bytes[8..10].try_into().unwrap()) != VERSION
        || bytes[10..42] != PROFILE_DIGEST { return Err(Error::Contract); }
    let n = u16::from_le_bytes(bytes[146..148].try_into().unwrap()) as usize;
    let m = u16::from_le_bytes(bytes[148..150].try_into().unwrap()) as usize;
    if !(1..=MAX_ID_BYTES).contains(&n) || !(1..=MAX_ID_BYTES).contains(&m)
        || bytes.len() != HEADER_BYTES + n + m { return Err(Error::Limit); }
    let value = Metadata {
        scope_digest: bytes[42..74].try_into().unwrap(),
        window_id: bytes[74..106].try_into().unwrap(),
        revision: u64::from_le_bytes(bytes[106..114].try_into().unwrap()),
        card_sha256: bytes[114..146].try_into().unwrap(),
        card_id: identity(&bytes[150..150+n])?,
        operation_id: identity(&bytes[150+n..])?,
    };
    if value.scope_digest == [0;32] || value.window_id == [0;32] || value.revision == 0 { return Err(Error::Invalid); }
    if &value.window_id != epoch || cursor.len() != 32 { return Err(Error::Correlation); }
    let mut digest = Sha256::new(); digest.update(CURSOR_DOMAIN); digest.update(bytes);
    if digest.finalize().as_slice() != cursor { return Err(Error::Correlation); }
    Ok(value)
}
/// Owned bounded C result; ID bytes are length-delimited, never NUL-terminated.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct View {
    pub scope_digest: [u8;32], pub window_id: [u8;32], pub card_sha256: [u8;32],
    pub revision: u64, pub card_id_length: u32, pub operation_id_length: u32,
    pub card_id: [u8;256], pub operation_id: [u8;256],
}
/// # Safety
/// Nonnull pointers must describe readable `length`, 32, and `cursor_length`
/// bytes respectively. `out` must be aligned and writable for `out_size` bytes.
/// They may alias; inputs are fully read before writing the owned output.
/// On failure output is unchanged. No pointer is retained or transmitted.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mc_metadata_v1_decode(
    bytes: *const u8, length: u32, epoch: *const u8, cursor: *const u8,
    cursor_length: u32, out: *mut View, out_size: u32,
) -> u32 {
    if bytes.is_null() || epoch.is_null() || cursor.is_null() || out.is_null()
        || out_size < std::mem::size_of::<View>() as u32
        || !(HEADER_BYTES..=MAX_BYTES).contains(&(length as usize)) { return Error::Invalid as u32; }
    if cursor_length != 32 { return Error::Correlation as u32; }
    let bytes = unsafe { std::slice::from_raw_parts(bytes, length as usize) };
    let epoch: &[u8;32] = unsafe { std::slice::from_raw_parts(epoch,32) }.try_into().unwrap();
    let cursor = unsafe { std::slice::from_raw_parts(cursor,32) };
    match decode(bytes, epoch, cursor) {
        Ok(v) => {
            let mut copy = View { scope_digest:v.scope_digest, window_id:v.window_id, card_sha256:v.card_sha256,
                revision:v.revision, card_id_length:v.card_id.len() as u32, operation_id_length:v.operation_id.len() as u32,
                card_id:[0;256],operation_id:[0;256] };
            copy.card_id[..v.card_id.len()].copy_from_slice(v.card_id.as_bytes());
            copy.operation_id[..v.operation_id.len()].copy_from_slice(v.operation_id.as_bytes());
            unsafe { out.write(copy); } 0
        }, Err(e) => e as u32,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn workbench_bookmark_wire_layout_and_invalid_fields() {
        let mut wire = [0; Bookmark::BYTES];
        wire[0] = 1;
        wire[1..33].fill(0x2a);
        wire[33..41].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
        wire[41..].fill(0xd7);
        let value = Bookmark {
            window_id: [0x2a; 32],
            sequence: 0x0807_0605_0403_0201,
            metadata_cursor: [0xd7; 32],
        };
        assert_eq!(Bookmark::decode(&wire), Ok(value));
        assert_eq!(value.encode(), Ok(wire));
        assert_eq!(Bookmark::decode(&wire[..72]), Err(Error::Invalid));
        assert_eq!(Bookmark::decode(&[0; 74]), Err(Error::Invalid));
        for field in [1..33, 33..41, 41..73] {
            let mut invalid = wire;
            invalid[field].fill(0);
            assert_eq!(Bookmark::decode(&invalid), Err(Error::Invalid));
        }
        wire[0] = 2;
        assert_eq!(Bookmark::decode(&wire), Err(Error::Contract));
        assert_eq!(Bookmark { sequence: 0, ..value }.encode(), Err(Error::Invalid));
    }
    #[test] fn contract_is_exact_bytes() { assert_eq!(Sha256::digest(CONTRACT).as_slice(), PROFILE_DIGEST); }
    #[test] fn bounded_ffi_errors_do_not_touch_output() {
        let mut out = [0xa5u8; std::mem::size_of::<View>()];
        let result=unsafe { mc_metadata_v1_decode(std::ptr::null(),0,std::ptr::null(),std::ptr::null(),0,out.as_mut_ptr().cast(),out.len() as u32) };
        assert_ne!(result,0); assert!(out.iter().all(|v| *v==0xa5));
    }
}
#[cfg(test)]
mod conformance {
    use super::*;
    #[test]
    fn shared_vectors_strict_rust_and_c_abi() {
        let data=include_bytes!("../tests/vectors.bin");let mut pos=0;let mut cases=0;
        while pos<data.len() {
            let accept=data[pos]!=0;pos+=1;
            let epoch:&[u8;32]=data[pos..pos+32].try_into().unwrap();pos+=32;
            let cursor_len=u32::from_le_bytes(data[pos..pos+4].try_into().unwrap()) as usize;pos+=4;
            let cursor=&data[pos..pos+32];pos+=32;
            let n=u32::from_le_bytes(data[pos..pos+4].try_into().unwrap()) as usize;pos+=4;
            let payload=&data[pos..pos+n];pos+=n;
            let mut extended=cursor.to_vec();extended.resize(cursor_len,0);
            assert_eq!(decode(payload,epoch,&extended).is_ok(),accept,"Rust case {cases}");
            let mut out=std::mem::MaybeUninit::<View>::uninit();
            let code=unsafe { mc_metadata_v1_decode(payload.as_ptr(),n as u32,epoch.as_ptr(),extended.as_ptr(),cursor_len as u32,out.as_mut_ptr(),std::mem::size_of::<View>() as u32) };
            assert_eq!(code==0,accept,"C ABI case {cases}");cases+=1;
        }
        assert_eq!(cases,209);
    }
}
