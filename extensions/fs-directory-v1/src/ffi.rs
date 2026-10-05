//! Native owning C ABI. Valid readable/writable aligned pointers remain the
//! caller's responsibility; these checks are not a memory sandbox or OOM recovery.
use crate::{
    DirectoryEntry, DirectoryState, EntryKind, Error, FsDirectoryPage, MAX_ENTRIES_PER_PAGE,
    MAX_ENVELOPE_BYTES, MAX_NAME_BYTES_PER_PAGE, NameEncoding, Result, SCHEMA_DIGEST, StateLimits,
    StateSnapshot,
};
use std::{
    mem::{align_of, size_of},
    ptr, slice,
};
pub const ABI_VERSION: u32 = 1;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct EntryView {
    pub entry_id: [u8; 32],
    pub name: *const u8,
    pub name_length: u32,
    pub encoding: u32,
    pub kind: u32,
    pub has_logical_length: u32,
    pub logical_length: u64,
    pub reserved: u64,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PageView {
    pub abi_version: u32,
    pub struct_size: u32,
    pub selection_epoch: [u8; 32],
    pub page_sequence: u64,
    pub entry_count: u32,
    pub terminal: u32,
    pub entries: *const EntryView,
    pub reserved: u64,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Limits {
    pub abi_version: u32,
    pub struct_size: u32,
    pub max_entries: u32,
    pub max_pages: u32,
    pub max_name_bytes: u64,
    pub max_wire_bytes: u64,
    pub reserved: u64,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Snapshot {
    pub abi_version: u32,
    pub struct_size: u32,
    pub accepted_pages: u64,
    pub next_sequence: u64,
    pub admitted_wire_bytes: u64,
    pub accepted_name_bytes: u64,
    pub accepted_entries: u32,
    pub resident_ids: u32,
    pub terminal: u32,
    pub released: u32,
    pub reserved: u64,
}
pub struct Page {
    value: FsDirectoryPage,
    views: Vec<EntryView>,
}
pub struct State {
    value: DirectoryState,
}
fn region(p: *const u8, n: usize) -> Result<(usize, usize)> {
    let a = p as usize;
    let b = a.checked_add(n).ok_or(Error::Invalid)?;
    if n != 0 && p.is_null() {
        return Err(Error::Invalid);
    }
    Ok((a, b))
}
fn overlap(a: (usize, usize), b: (usize, usize)) -> bool {
    a.0 < a.1 && b.0 < b.1 && a.0 < b.1 && b.0 < a.1
}
fn checked<T>(p: *const T) -> Result<()> {
    region(p.cast(), size_of::<T>())?;
    if !(p as usize).is_multiple_of(align_of::<T>()) {
        return Err(Error::Invalid);
    }
    Ok(())
}
fn status(r: Result<()>) -> u32 {
    r.err().map_or(0, |e| e as u32)
}
impl Page {
    fn new(value: FsDirectoryPage) -> Box<Self> {
        let views = value
            .entries
            .iter()
            .map(|e| EntryView {
                entry_id: e.entry_id,
                name: e.name.as_ptr(),
                name_length: e.name.len() as u32,
                encoding: e.encoding as u32,
                kind: e.kind as u32,
                has_logical_length: u32::from(e.logical_length.is_some()),
                logical_length: e.logical_length.unwrap_or(0),
                reserved: 0,
            })
            .collect();
        Box::new(Self { value, views })
    }
    fn view(&self) -> PageView {
        PageView {
            abi_version: ABI_VERSION,
            struct_size: size_of::<PageView>() as u32,
            selection_epoch: self.value.selection_epoch,
            page_sequence: self.value.page_sequence,
            entry_count: self.views.len() as u32,
            terminal: u32::from(self.value.terminal),
            entries: if self.views.is_empty() {
                ptr::null()
            } else {
                self.views.as_ptr()
            },
            reserved: 0,
        }
    }
    fn disjoint(&self, p: *const u8, n: usize) -> Result<()> {
        let r = region(p, n)?;
        let own = region((self as *const Self).cast(), size_of::<Self>())?;
        if overlap(r, own)
            || overlap(
                r,
                region(
                    self.views.as_ptr().cast(),
                    self.views.len() * size_of::<EntryView>(),
                )?,
            )
        {
            return Err(Error::Invalid);
        }
        for e in &self.value.entries {
            if overlap(r, region(e.name.as_ptr(), e.name.len())?) {
                return Err(Error::Invalid);
            }
        }
        Ok(())
    }
}
unsafe fn page<'a>(p: *const Page) -> Result<&'a Page> {
    checked(p)?;
    Ok(unsafe { &*p })
}
unsafe fn state<'a>(p: *mut State) -> Result<&'a mut State> {
    checked(p)?;
    Ok(unsafe { &mut *p })
}
unsafe fn bytes<'a>(p: *const u8, n: u32, max: usize) -> Result<&'a [u8]> {
    if n as usize > max {
        return Err(Error::Limit);
    }
    region(p, n as usize)?;
    if n == 0 {
        Ok(&[])
    } else {
        Ok(unsafe { slice::from_raw_parts(p, n as usize) })
    }
}
unsafe fn output<T>(p: *mut T) -> Result<()> {
    checked(p)
}
unsafe fn from_view(p: *const PageView, n: u32) -> Result<FsDirectoryPage> {
    if n as usize != size_of::<PageView>() {
        return Err(Error::Invalid);
    }
    checked(p)?;
    let v = unsafe { ptr::read(p) };
    if v.abi_version != ABI_VERSION || v.struct_size != n || v.reserved != 0 || v.terminal > 1 {
        return Err(Error::Invalid);
    }
    if v.entry_count as usize > MAX_ENTRIES_PER_PAGE {
        return Err(Error::Limit);
    }
    let len = v.entry_count as usize;
    region(v.entries.cast(), len * size_of::<EntryView>())?;
    if len != 0 {
        checked(v.entries)?;
    }
    let views = if len == 0 {
        &[][..]
    } else {
        unsafe { slice::from_raw_parts(v.entries, len) }
    };
    // First bound every length and the total before allocating any entry/name.
    let mut total = 0usize;
    for e in views {
        total = total
            .checked_add(e.name_length as usize)
            .ok_or(Error::Limit)?;
        if total > MAX_NAME_BYTES_PER_PAGE {
            return Err(Error::Limit);
        }
        if e.reserved != 0
            || e.has_logical_length > 1
            || e.has_logical_length == 0 && e.logical_length != 0
        {
            return Err(Error::Invalid);
        }
        NameEncoding::try_from(e.encoding)?;
        EntryKind::try_from(e.kind)?;
        region(e.name, e.name_length as usize)?;
    }
    let mut entries = Vec::with_capacity(len);
    for e in views {
        entries.push(DirectoryEntry {
            entry_id: e.entry_id,
            name: unsafe { bytes(e.name, e.name_length, MAX_NAME_BYTES_PER_PAGE)? }.to_vec(),
            encoding: NameEncoding::try_from(e.encoding)?,
            kind: EntryKind::try_from(e.kind)?,
            logical_length: if e.has_logical_length == 1 {
                Some(e.logical_length)
            } else {
                None
            },
        });
    }
    let p = FsDirectoryPage {
        selection_epoch: v.selection_epoch,
        page_sequence: v.page_sequence,
        entries,
        terminal: v.terminal == 1,
    };
    p.validate()?;
    Ok(p)
}
/// Successful creation replaces only the pointer slot; caller frees any old owner.
/// # Safety
/// Pointers must satisfy the native C header validity, alignment, exclusive-access
/// and ownership contract. Opaque handles must be live SDK allocations; frees
/// consume one owner. Output memory must not overlap live owner storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mfd_page_v1_create(
    input: *const PageView,
    input_size: u32,
    out: *mut *mut Page,
) -> u32 {
    status((|| {
        unsafe {
            output(out)?;
        }
        let p = Page::new(unsafe { from_view(input, input_size)? });
        unsafe {
            ptr::write(out, Box::into_raw(p));
        }
        Ok(())
    })())
}
/// # Safety
/// Pointers must satisfy the native C header validity, alignment, exclusive-access
/// and ownership contract. Opaque handles must be live SDK allocations; frees
/// consume one owner. Output memory must not overlap live owner storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mfd_page_v1_decode(
    input: *const u8,
    length: u32,
    out: *mut *mut Page,
) -> u32 {
    status((|| {
        unsafe {
            output(out)?;
        }
        let p = Page::new(FsDirectoryPage::decode(unsafe {
            bytes(input, length, MAX_ENVELOPE_BYTES)?
        })?);
        unsafe {
            ptr::write(out, Box::into_raw(p));
        }
        Ok(())
    })())
}
/// # Safety
/// Pointers must satisfy the native C header validity, alignment, exclusive-access
/// and ownership contract. Opaque handles must be live SDK allocations; frees
/// consume one owner. Output memory must not overlap live owner storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mfd_page_v1_encode(
    input: *const Page,
    out: *mut u8,
    capacity: u32,
    out_length: *mut u32,
) -> u32 {
    status((|| {
        unsafe {
            output(out_length)?;
        }
        let p = unsafe { page(input)? };
        let wire = p.value.encode()?;
        if wire.len() > capacity as usize {
            return Err(Error::Buffer);
        }
        let r = region(out, wire.len())?;
        if overlap(r, region(out_length.cast(), size_of::<u32>())?) {
            return Err(Error::Invalid);
        }
        p.disjoint(out, wire.len())?;
        p.disjoint(out_length.cast(), size_of::<u32>())?;
        unsafe {
            ptr::copy_nonoverlapping(wire.as_ptr(), out, wire.len());
            ptr::write(out_length, wire.len() as u32);
        }
        Ok(())
    })())
}
/// # Safety
/// Pointers must satisfy the native C header validity, alignment, exclusive-access
/// and ownership contract. Opaque handles must be live SDK allocations; frees
/// consume one owner. Output memory must not overlap live owner storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mfd_page_v1_view(
    input: *const Page,
    out: *mut PageView,
    out_size: u32,
) -> u32 {
    status((|| {
        if out_size as usize != size_of::<PageView>() {
            return Err(Error::Invalid);
        }
        unsafe {
            output(out)?;
        }
        let p = unsafe { page(input)? };
        p.disjoint(out.cast(), size_of::<PageView>())?;
        unsafe {
            ptr::write(out, p.view());
        }
        Ok(())
    })())
}
/// # Safety
/// Pointers must satisfy the native C header validity, alignment, exclusive-access
/// and ownership contract. Opaque handles must be live SDK allocations; frees
/// consume one owner. Output memory must not overlap live owner storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mfd_page_v1_entry(
    input: *const Page,
    index: u32,
    out: *mut EntryView,
    out_size: u32,
) -> u32 {
    status((|| {
        if out_size as usize != size_of::<EntryView>() {
            return Err(Error::Invalid);
        }
        unsafe {
            output(out)?;
        }
        let p = unsafe { page(input)? };
        let v = *p.views.get(index as usize).ok_or(Error::Invalid)?;
        p.disjoint(out.cast(), size_of::<EntryView>())?;
        unsafe {
            ptr::write(out, v);
        }
        Ok(())
    })())
}
/// # Safety
/// Pointers must satisfy the native C header validity, alignment, exclusive-access
/// and ownership contract. Opaque handles must be live SDK allocations; frees
/// consume one owner. Output memory must not overlap live owner storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mfd_page_v1_free(p: *mut Page) {
    if !p.is_null() {
        drop(unsafe { Box::from_raw(p) });
    }
}
/// # Safety
/// Pointers must satisfy the native C header validity, alignment, exclusive-access
/// and ownership contract. Opaque handles must be live SDK allocations; frees
/// consume one owner. Output memory must not overlap live owner storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mfd_directory_v1_schema_digest(out: *mut u8, length: u32) -> u32 {
    status((|| {
        if length != 32 {
            return Err(Error::Invalid);
        }
        region(out, 32)?;
        unsafe {
            ptr::copy_nonoverlapping(SCHEMA_DIGEST.as_ptr(), out, 32);
        }
        Ok(())
    })())
}
unsafe fn limits(p: *const Limits, n: u32) -> Result<StateLimits> {
    if n as usize != size_of::<Limits>() {
        return Err(Error::Invalid);
    }
    checked(p)?;
    let l = unsafe { ptr::read(p) };
    if l.abi_version != ABI_VERSION || l.struct_size != n || l.reserved != 0 {
        return Err(Error::Invalid);
    }
    let l = StateLimits {
        max_entries: l.max_entries as usize,
        max_pages: l.max_pages as u64,
        max_name_bytes: l.max_name_bytes,
        max_wire_bytes: l.max_wire_bytes,
    };
    l.validate()?;
    Ok(l)
}
fn snapshot(s: StateSnapshot) -> Snapshot {
    Snapshot {
        abi_version: ABI_VERSION,
        struct_size: size_of::<Snapshot>() as u32,
        accepted_pages: s.accepted_pages,
        next_sequence: s.next_sequence,
        admitted_wire_bytes: s.admitted_wire_bytes,
        accepted_name_bytes: s.accepted_name_bytes,
        accepted_entries: s.accepted_entries as u32,
        resident_ids: s.resident_ids as u32,
        terminal: u32::from(s.terminal),
        released: u32::from(s.released),
        reserved: 0,
    }
}
/// # Safety
/// Pointers must satisfy the native C header validity, alignment, exclusive-access
/// and ownership contract. Opaque handles must be live SDK allocations; frees
/// consume one owner. Output memory must not overlap live owner storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mfd_state_v1_new(
    epoch: *const u8,
    limits_ptr: *const Limits,
    limits_size: u32,
    out: *mut *mut State,
) -> u32 {
    status((|| {
        unsafe {
            output(out)?;
        }
        let epoch: [u8; 32] = unsafe { bytes(epoch, 32, 32)? }
            .try_into()
            .map_err(|_| Error::Invalid)?;
        let value = DirectoryState::new(epoch, unsafe { limits(limits_ptr, limits_size)? })?;
        unsafe {
            ptr::write(out, Box::into_raw(Box::new(State { value })));
        }
        Ok(())
    })())
}
/// Failed admitted calls retain wire cost while leaving semantic state and out unchanged.
/// # Safety
/// Pointers must satisfy the native C header validity, alignment, exclusive-access
/// and ownership contract. Opaque handles must be live SDK allocations; frees
/// consume one owner. Output memory must not overlap live owner storage.
/// Wire must be readable and disjoint from the state allocation (checked before
/// forming any slice/exclusive reference); wire and output slot may alias.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mfd_state_v1_admit(
    input: *mut State,
    wire: *const u8,
    length: u32,
    out: *mut *mut Page,
) -> u32 {
    status((|| {
        unsafe {
            output(out)?;
        }
        if overlap(
            region(input.cast(), size_of::<State>())?,
            region(out.cast(), size_of::<*mut Page>())?,
        ) {
            return Err(Error::Invalid);
        }
        if length as usize > MAX_ENVELOPE_BYTES {
            return Err(Error::Limit);
        }
        if overlap(
            region(input.cast(), size_of::<State>())?,
            region(wire, length as usize)?,
        ) {
            return Err(Error::Invalid);
        }
        // Refuse overlap before constructing any reference into opaque storage.
        let data = unsafe { bytes(wire, length, MAX_ENVELOPE_BYTES)? };
        let s = unsafe { state(input)? };
        let owned = Page::new(s.value.admit(data)?);
        unsafe {
            ptr::write(out, Box::into_raw(owned));
        }
        Ok(())
    })())
}
/// # Safety
/// Pointers must satisfy the native C header validity, alignment, exclusive-access
/// and ownership contract. Opaque handles must be live SDK allocations; frees
/// consume one owner. Output memory must not overlap live owner storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mfd_state_v1_accept_page(input: *mut State, p: *const Page) -> u32 {
    status((|| {
        let p = unsafe { page(p)? };
        p.disjoint(input.cast(), size_of::<State>())?;
        unsafe { state(input)? }.value.accept_page(&p.value)
    })())
}
/// # Safety
/// Pointers must satisfy the native C header validity, alignment, exclusive-access
/// and ownership contract. Opaque handles must be live SDK allocations; frees
/// consume one owner. Output memory must not overlap live owner storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mfd_state_v1_snapshot(
    input: *const State,
    out: *mut Snapshot,
    out_size: u32,
) -> u32 {
    status((|| {
        if out_size as usize != size_of::<Snapshot>() {
            return Err(Error::Invalid);
        }
        checked(input)?;
        unsafe {
            output(out)?;
        }
        if overlap(
            region(input.cast(), size_of::<State>())?,
            region(out.cast(), size_of::<Snapshot>())?,
        ) {
            return Err(Error::Invalid);
        }
        let s = unsafe { &*input };
        unsafe {
            ptr::write(out, snapshot(s.value.snapshot()));
        }
        Ok(())
    })())
}
/// # Safety
/// Pointers must satisfy the native C header validity, alignment, exclusive-access
/// and ownership contract. Opaque handles must be live SDK allocations; frees
/// consume one owner. Output memory must not overlap live owner storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mfd_state_v1_release(input: *mut State) -> u32 {
    status((|| {
        unsafe { state(input)? }.value.release();
        Ok(())
    })())
}
/// # Safety
/// Pointers must satisfy the native C header validity, alignment, exclusive-access
/// and ownership contract. Opaque handles must be live SDK allocations; frees
/// consume one owner. Output memory must not overlap live owner storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mfd_state_v1_free(input: *mut State) {
    if !input.is_null() {
        drop(unsafe { Box::from_raw(input) });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_page_ownership_error_sentinels_alias_and_checked_views() {
        unsafe {
            let wire = crate::tests::page(1, 1, false).encode().unwrap();
            let mut owner = ptr::null_mut();
            assert_eq!(
                mfd_page_v1_decode(wire.as_ptr(), wire.len() as u32, &mut owner),
                0
            );
            let mut view: PageView = std::mem::zeroed();
            assert_eq!(
                mfd_page_v1_view(owner, &mut view, size_of::<PageView>() as u32),
                0
            );
            assert_eq!(view.entry_count, 1);
            let old = owner;
            assert_eq!(
                mfd_page_v1_decode([0u8; 8].as_ptr(), 8, &mut owner),
                Error::Invalid as u32
            );
            assert_eq!(owner, old);
            let mut output = vec![0xa5; MAX_ENVELOPE_BYTES];
            let before = output.clone();
            let mut n = 999;
            assert_eq!(
                mfd_page_v1_encode(owner, output.as_mut_ptr(), 1, &mut n),
                Error::Buffer as u32
            );
            assert_eq!(output, before);
            assert_eq!(n, 999);
            assert_eq!(
                mfd_page_v1_encode(
                    owner,
                    output.as_mut_ptr(),
                    output.len() as u32,
                    output.as_mut_ptr().cast()
                ),
                Error::Invalid as u32
            );
            assert_eq!(output, before);
            assert_eq!(
                mfd_page_v1_entry(owner, 1, ptr::null_mut(), size_of::<EntryView>() as u32),
                Error::Invalid as u32
            );
            assert_eq!(
                mfd_page_v1_encode(owner, output.as_mut_ptr(), output.len() as u32, &mut n),
                0
            );
            assert_eq!(&output[..n as usize], wire);
            // Wire input and output pointer slot may alias: decoded names are owned first.
            let mut aliased = vec![0u64; wire.len().div_ceil(8)];
            ptr::copy_nonoverlapping(wire.as_ptr(), aliased.as_mut_ptr().cast(), wire.len());
            assert_eq!(
                mfd_page_v1_decode(
                    aliased.as_ptr().cast(),
                    wire.len() as u32,
                    aliased.as_mut_ptr().cast()
                ),
                0
            );
            let copy = ptr::read(aliased.as_ptr().cast::<*mut Page>());
            assert_eq!((*copy).value, (*owner).value);
            mfd_page_v1_free(copy);
            mfd_page_v1_free(owner);
        }
    }
    #[test]
    fn native_state_cost_semantic_atomicity_release_and_owned_admission() {
        unsafe {
            let l = Limits {
                abi_version: 1,
                struct_size: size_of::<Limits>() as u32,
                max_entries: 1024,
                max_pages: 1024,
                max_name_bytes: 1048576,
                max_wire_bytes: 1048576,
                reserved: 0,
            };
            let mut s = ptr::null_mut();
            let epoch = [1u8; 32];
            assert_eq!(
                mfd_state_v1_new(epoch.as_ptr(), &l, size_of::<Limits>() as u32, &mut s),
                0
            );
            let wire = crate::tests::page(1, 1, false).encode().unwrap();
            let mut p = ptr::null_mut();
            assert_eq!(
                mfd_state_v1_admit(s, wire.as_ptr(), wire.len() as u32, &mut p),
                0
            );
            let before = (*s).value.snapshot();
            let old = p;
            assert_eq!(
                mfd_state_v1_admit(s, wire.as_ptr(), wire.len() as u32, &mut p),
                Error::Sequence as u32
            );
            assert_eq!(p, old);
            let after = (*s).value.snapshot();
            assert_eq!(after.accepted_pages, before.accepted_pages);
            assert_eq!(
                after.admitted_wire_bytes,
                before.admitted_wire_bytes + wire.len() as u64
            );
            assert_eq!(mfd_state_v1_release(s), 0);
            let retained = (*s).value.snapshot();
            assert_eq!(retained.resident_ids, 0);
            assert_eq!(retained.admitted_wire_bytes, after.admitted_wire_bytes);
            assert_eq!(
                mfd_state_v1_admit(s, wire.as_ptr(), wire.len() as u32, &mut p),
                Error::Released as u32
            );
            mfd_page_v1_free(p);
            mfd_state_v1_free(s);
        }
    }
    #[test]
    fn state_wire_overlap_refused_before_reference_and_without_admission() {
        unsafe {
            let s = Box::into_raw(Box::new(State {
                value: DirectoryState::new([1; 32], StateLimits::default()).unwrap(),
            }));
            let before = (*s).value.snapshot();
            let mut out = std::ptr::without_provenance_mut::<Page>(1);
            let sentinel = out;
            assert_eq!(
                mfd_state_v1_admit(s, s.cast(), size_of::<State>() as u32, &mut out),
                Error::Invalid as u32
            );
            assert_eq!(out, sentinel);
            assert_eq!((*s).value.snapshot(), before);
            mfd_state_v1_free(s);
        }
    }
}
