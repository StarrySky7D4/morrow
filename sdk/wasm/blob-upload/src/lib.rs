//! One static archive and one Rust allocator for the independent C/C++ guests.
//! Reuses the original SDK codecs; does not link the trusted host or Store.
pub use morrow_blob_transfer_v1 as transfer;
pub use morrow_blob_upload_proposal_v1 as proposal;
pub use morrow_blob_upload_request_v1 as upload;
pub use morrow_blob_upload_sample as sample;
pub use morrow_plugin_sdk as task;
use sha2::{Digest, Sha256};

/// Compute one bounded chunk digest. The caller supplies a live readable input
/// and a writable 32-byte output. Input/output may overlap: input is consumed first.
/// Null input is valid only for length zero. No host import or whole-object buffer.
///
/// # Safety
/// Non-null pointers must cover their declared accessible memory regions.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbu_guest_chunk_sha256(
    input: *const u8,
    length: u32,
    output: *mut u8,
) -> u32 {
    if output.is_null() || length > 61440 || (input.is_null() && length != 0) {
        return 1;
    }
    let bytes = if length == 0 {
        &[]
    } else {
        // SAFETY: the caller promises a live readable bounded input.
        unsafe { std::slice::from_raw_parts(input, length as usize) }
    };
    let digest = Sha256::digest(bytes);
    // SAFETY: the caller promises a writable 32-byte output, separate from this digest.
    unsafe { std::ptr::copy_nonoverlapping(digest.as_ptr(), output, 32) };
    0
}
