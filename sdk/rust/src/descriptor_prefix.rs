//! Native sized descriptors are inspected without forming a full reference.
#[repr(C)]
#[derive(Clone, Copy)]
struct Header {
    abi_version: u32,
    struct_size: u32,
}
/// # Safety
/// Non-null, aligned inputs must provide eight readable prefix bytes. A passing
/// size advertises readable storage for the full T; invalid addresses cannot be
/// made safe by this check. No full-sized pointer dereference occurs here.
pub(crate) unsafe fn matches<T>(raw: *const T) -> bool {
    if raw.is_null() || !raw.is_aligned() || !raw.cast::<Header>().is_aligned() {
        return false;
    }
    let prefix = unsafe { raw.cast::<Header>().read() };
    prefix.abi_version == 1 && prefix.struct_size as usize >= size_of::<T>()
}
