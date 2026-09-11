//! One allocator for C/C++ and Rust within a guest. Never host memory or capabilities.
use std::alloc::{Layout, alloc, dealloc};
const ALIGN: usize = 16;
const MAX_ALLOCATION: usize = 64 * 1024 * 1024;
fn allocate(size: u32, alignment: u32) -> *mut u8 {
    let size = (size as usize).max(1);
    let alignment = alignment as usize;
    if size > MAX_ALLOCATION || !alignment.is_power_of_two() || alignment > 65536 {
        return std::ptr::null_mut();
    }
    let alignment = alignment.max(ALIGN);
    let layout = Layout::from_size_align(size + alignment, alignment).expect("bounded layout");
    // SAFETY: valid nonzero layout; fixed header immediately precedes aligned payload.
    unsafe {
        let base = alloc(layout);
        if base.is_null() {
            return base;
        }
        let payload = base.add(alignment);
        let header = payload.sub(ALIGN).cast::<usize>();
        header.write(size);
        header.add(1).write(alignment);
        payload
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn mp_guest_malloc(size: u32) -> *mut u8 {
    allocate(size, ALIGN as u32)
}
#[unsafe(no_mangle)]
pub extern "C" fn mp_guest_memalign(alignment: u32, size: u32) -> *mut u8 {
    allocate(size, alignment)
}
/// # Safety
/// p is null or a live pointer from this guest allocator, never freed twice.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp_guest_free(p: *mut u8) {
    if p.is_null() {
        return;
    }
    unsafe {
        let header = p.sub(ALIGN).cast::<usize>();
        let size = header.read();
        let alignment = header.add(1).read();
        dealloc(
            p.sub(alignment),
            Layout::from_size_align(size + alignment, alignment).expect("original layout"),
        );
    }
}
/// # Safety
/// p is null or a live pointer from this allocator; no overlapping active borrows.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp_guest_realloc(p: *mut u8, size: u32) -> *mut u8 {
    if p.is_null() {
        return mp_guest_malloc(size);
    }
    if size == 0 {
        unsafe {
            mp_guest_free(p);
        }
        return std::ptr::null_mut();
    }
    let replacement = mp_guest_malloc(size);
    if replacement.is_null() {
        return replacement;
    }
    unsafe {
        let old = p.sub(ALIGN).cast::<usize>().read();
        std::ptr::copy_nonoverlapping(p, replacement, old.min(size as usize));
        mp_guest_free(p);
    }
    replacement
}
#[unsafe(no_mangle)]
pub extern "C" fn mp_guest_calloc(count: u32, size: u32) -> *mut u8 {
    let Some(size) = count.checked_mul(size) else {
        return std::ptr::null_mut();
    };
    let p = mp_guest_malloc(size);
    if !p.is_null() {
        unsafe {
            std::ptr::write_bytes(p, 0, size as usize);
        }
    }
    p
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn aligned_zero_overflow_and_failed_realloc_preserve_original() {
        unsafe {
            let p = mp_guest_calloc(7, 3);
            assert!(!p.is_null());
            assert_eq!(p as usize % 16, 0);
            assert_eq!(std::slice::from_raw_parts(p, 21), [0; 21]);
            p.write(42);
            assert!(mp_guest_realloc(p, u32::MAX).is_null());
            assert_eq!(p.read(), 42);
            let q = mp_guest_realloc(p, 128);
            assert!(!q.is_null());
            assert_eq!(q.read(), 42);
            let q = mp_guest_realloc(q, 1);
            assert_eq!(q.read(), 42);
            assert!(mp_guest_realloc(q, 0).is_null());
            assert!(mp_guest_calloc(u32::MAX, 2).is_null());
            let z = mp_guest_malloc(0);
            assert!(!z.is_null());
            mp_guest_free(z);
            mp_guest_free(std::ptr::null_mut());
        }
    }
}

#[cfg(test)]
#[test]
fn over_aligned_allocations_share_free_and_reallocation() {
    unsafe {
        for alignment in [16, 64, 256, 4096, 65536] {
            let p = mp_guest_memalign(alignment, 513);
            assert!(!p.is_null());
            assert_eq!(p as usize % alignment as usize, 0);
            p.write(73);
            let q = mp_guest_realloc(p, 1024);
            assert_eq!(q.read(), 73);
            mp_guest_free(q);
        }
        assert!(mp_guest_memalign(3, 64).is_null());
        assert!(mp_guest_memalign(0, 64).is_null());
        assert!(mp_guest_memalign(131072, 64).is_null());
    }
}
