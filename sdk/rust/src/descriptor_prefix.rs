//! Copy a native descriptor only after checking its leading version and size.
#[repr(C)]
#[derive(Clone, Copy)]
struct Header {
    abi_version: u32,
    struct_size: u32,
}
/// # Safety
/// `T` must be a C descriptor beginning with `Header`. The caller provides
/// readable storage for the header and, if its version and size match, a valid
/// initialized descriptor for the duration of the copy.
pub(crate) unsafe fn read<T: Copy>(raw: *const T) -> Option<T> {
    let header = raw.cast::<Header>();
    if raw.is_null() || !header.is_aligned() {
        return None;
    }
    // MSVC's 32-bit stack guarantees four-byte alignment, even for a struct
    // containing u64 fields. Other targets retain the native type alignment.
    #[cfg(not(all(target_arch = "x86", target_env = "msvc", target_os = "windows")))]
    if !raw.is_aligned() {
        return None;
    }
    let prefix = unsafe { header.read() };
    if prefix.abi_version != 1 || (prefix.struct_size as usize) < size_of::<T>() {
        return None;
    }
    Some(unsafe { raw.read_unaligned() })
}
