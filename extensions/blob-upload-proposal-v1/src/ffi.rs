//! Local native owners, also linkable into a Wasm guest. No host imports or grants.
use crate::{BoundInput, Error, Proposal, Result};
use morrow_blob_transfer_v1::Identity;
use std::{
    mem::{align_of, size_of},
    ptr,
};

pub struct MbpProposal(Proposal);
pub struct MbpBoundInput(BoundInput);
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ProposalView {
    pub abi_version: u32,
    pub struct_size: u32,
    pub byte_length: u64,
    pub whole_sha256: [u8; 32],
    pub label: *const u8,
    pub label_length: u32,
    pub reserved: u32,
    pub producer_input: *const u8,
    pub producer_input_length: u32,
    pub reserved2: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct IdentityView {
    pub abi_version: u32,
    pub struct_size: u32,
    pub transfer_epoch: [u8; 32],
    pub object_ref: [u8; 32],
    pub operation_id: [u8; 32],
}
type Region = (usize, usize);
fn status(value: Result<()>) -> u32 {
    value.err().map_or(0, |e| e as u32)
}
fn region(address: usize, length: usize) -> Result<Region> {
    Ok((address, address.checked_add(length).ok_or(Error::Buffer)?))
}
fn disjoint(address: usize, length: usize, regions: &[Region]) -> Result<()> {
    let value = region(address, length)?;
    if regions.iter().any(|r| value.0 < r.1 && r.0 < value.1) {
        Err(Error::Buffer)
    } else {
        Ok(())
    }
}
fn aligned<T>(pointer: *const T) -> Result<()> {
    if pointer.is_null() || !pointer.addr().is_multiple_of(align_of::<T>()) {
        return Err(Error::Buffer);
    }
    region(pointer.addr(), size_of::<T>())?;
    Ok(())
}
fn output<T>(pointer: *mut T, capacity: u32, regions: &[Region]) -> Result<()> {
    if (capacity as usize) < size_of::<T>() {
        return Err(Error::Buffer);
    }
    aligned(pointer)?;
    disjoint(pointer.addr(), size_of::<T>(), regions)
}
unsafe fn input<T: Copy>(pointer: *const T, size: u32) -> Result<T> {
    if size as usize != size_of::<T>() {
        return Err(Error::Contract);
    }
    aligned(pointer)?;
    // SAFETY: caller supplies the complete readable input of the checked size.
    Ok(unsafe { ptr::read(pointer) })
}
unsafe fn bytes<'a>(pointer: *const u8, length: u32, maximum: usize) -> Result<&'a [u8]> {
    if length as usize > maximum {
        return Err(Error::Limit);
    }
    if length == 0 {
        return Ok(&[]);
    }
    if pointer.is_null() {
        return Err(Error::Buffer);
    }
    region(pointer.addr(), length as usize)?;
    // SAFETY: caller retains a live readable buffer for this invocation.
    Ok(unsafe { std::slice::from_raw_parts(pointer, length as usize) })
}
fn proposal_regions(value: &Proposal) -> Result<Vec<Region>> {
    Ok(vec![
        region(ptr::from_ref(value).addr(), size_of::<Proposal>())?,
        region(value.wire.as_ptr().addr(), value.wire.capacity())?,
        region(value.label.as_ptr().addr(), value.label.capacity())?,
        region(value.input.as_ptr().addr(), value.input.capacity())?,
    ])
}
fn bound_regions(value: &MbpBoundInput) -> Result<Vec<Region>> {
    let mut regions = proposal_regions(&value.0.proposal)?;
    regions.push(region(
        ptr::from_ref(value).addr(),
        size_of::<MbpBoundInput>(),
    )?);
    regions.push(region(
        value.0.wire.as_ptr().addr(),
        value.0.wire.capacity(),
    )?);
    Ok(regions)
}
fn view(value: &Proposal) -> ProposalView {
    ProposalView {
        abi_version: 1,
        struct_size: size_of::<ProposalView>() as u32,
        byte_length: value.byte_length(),
        whole_sha256: value.whole_sha256(),
        label: value.label.as_ptr(),
        label_length: value.label.len() as u32,
        reserved: 0,
        producer_input: if value.input.is_empty() {
            ptr::null()
        } else {
            value.input.as_ptr()
        },
        producer_input_length: value.input.len() as u32,
        reserved2: 0,
    }
}
unsafe fn encode(
    wire: &[u8],
    out: *mut u8,
    capacity: u32,
    written: *mut u32,
    regions: &[Region],
) -> Result<()> {
    if out.is_null() || (capacity as usize) < wire.len() {
        return Err(Error::Buffer);
    }
    output(written, size_of::<u32>() as u32, regions)?;
    disjoint(out.addr(), wire.len(), regions)?;
    disjoint(
        written.addr(),
        size_of::<u32>(),
        &[region(out.addr(), wire.len())?],
    )?;
    // SAFETY: caller promises writable disjoint buffers; all ranges were checked.
    unsafe {
        ptr::copy_nonoverlapping(wire.as_ptr(), out, wire.len());
        ptr::write(written, wire.len() as u32);
    }
    Ok(())
}

/// # Safety
/// The complete input view and referenced buffers are readable. The disjoint
/// output slot is writable and does not contain an owner that would be leaked.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbp_proposal_new(
    value: *const ProposalView,
    size: u32,
    out: *mut *mut MbpProposal,
) -> u32 {
    status((|| {
        // SAFETY: caller input obeys the documented live-memory contract.
        let v = unsafe { input(value, size) }?;
        if v.abi_version != 1 || v.struct_size as usize != size_of::<ProposalView>() {
            return Err(Error::Contract);
        }
        if v.reserved != 0 || v.reserved2 != 0 {
            return Err(Error::Invalid);
        }
        output(
            out,
            size_of::<*mut MbpProposal>() as u32,
            &[
                region(value.addr(), size_of::<ProposalView>())?,
                region(v.label.addr(), v.label_length as usize)?,
                region(v.producer_input.addr(), v.producer_input_length as usize)?,
            ],
        )?;
        // SAFETY: checked bounded inputs remain readable through the copy.
        let label =
            std::str::from_utf8(unsafe { bytes(v.label, v.label_length, crate::MAX_LABEL_BYTES) }?)
                .map_err(|_| Error::Invalid)?;
        // SAFETY: same original readable producer input.
        let data = unsafe {
            bytes(
                v.producer_input,
                v.producer_input_length,
                crate::MAX_PRODUCER_INPUT_BYTES,
            )
        }?;
        let value = Proposal::new(label, v.byte_length, v.whole_sha256, data)?;
        // SAFETY: checked disjoint output slot takes one fresh owner on success.
        unsafe {
            ptr::write(out, Box::into_raw(Box::new(MbpProposal(value))));
        }
        Ok(())
    })())
}
/// # Safety
/// Input is readable and the separate output pointer slot is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbp_proposal_decode(
    data: *const u8,
    length: u32,
    out: *mut *mut MbpProposal,
) -> u32 {
    status((|| {
        output(
            out,
            size_of::<*mut MbpProposal>() as u32,
            &[region(data.addr(), length as usize)?],
        )?;
        // SAFETY: caller supplies a live bounded input until decoding finishes.
        let value = Proposal::decode(unsafe { bytes(data, length, crate::MAX_PROPOSAL_BYTES) }?)?;
        // SAFETY: checked separate output slot owns the copied result.
        unsafe {
            ptr::write(out, Box::into_raw(Box::new(MbpProposal(value))));
        }
        Ok(())
    })())
}
/// # Safety
/// Null or one unique live owner from this library; no borrowed views remain in use.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbp_proposal_free(value: *mut MbpProposal) {
    if !value.is_null() {
        // SAFETY: caller transfers the unique original allocation once.
        unsafe {
            drop(Box::from_raw(value));
        }
    }
}
/// # Safety
/// Owner is live; writable output prefix is separate from all its owned storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbp_proposal_view_get(
    value: *const MbpProposal,
    out: *mut ProposalView,
    size: u32,
) -> u32 {
    status((|| {
        aligned(value)?;
        // SAFETY: caller retains the live immutable original owner.
        let value = unsafe { &*value };
        output(out, size, &proposal_regions(&value.0)?)?;
        // SAFETY: checked output borrows read-only owner storage.
        unsafe {
            ptr::write(out, view(&value.0));
        }
        Ok(())
    })())
}
/// # Safety
/// Owner is live; output prefix and length slot are writable and disjoint from
/// each other and every byte owned by the original owner.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbp_proposal_encode(
    value: *const MbpProposal,
    out: *mut u8,
    capacity: u32,
    written: *mut u32,
) -> u32 {
    status((|| {
        aligned(value)?;
        // SAFETY: caller retains a live owner throughout copying.
        let value = unsafe { &*value };
        // SAFETY: encode checks the caller's disjoint writable ranges.
        unsafe {
            encode(
                value.0.wire(),
                out,
                capacity,
                written,
                &proposal_regions(&value.0)?,
            )
        }
    })())
}
/// # Safety
/// Proposal is live; identity input is readable; output slot is separate from
/// input and every proposal allocation. The new envelope confers no authority.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbp_bound_new(
    proposal: *const MbpProposal,
    identity: *const IdentityView,
    size: u32,
    out: *mut *mut MbpBoundInput,
) -> u32 {
    status((|| {
        aligned(proposal)?;
        // SAFETY: caller retains the original proposal and complete identity view.
        let proposal = unsafe { &*proposal };
        let id = unsafe { input(identity, size) }?;
        if id.abi_version != 1 || id.struct_size as usize != size_of::<IdentityView>() {
            return Err(Error::Contract);
        }
        let mut regions = proposal_regions(&proposal.0)?;
        regions.push(region(identity.addr(), size_of::<IdentityView>())?);
        output(out, size_of::<*mut MbpBoundInput>() as u32, &regions)?;
        let value = BoundInput::new(
            &proposal.0,
            Identity {
                transfer_epoch: id.transfer_epoch,
                object_ref: id.object_ref,
                operation_id: id.operation_id,
            },
        )?;
        // SAFETY: output was checked and receives one independent owning copy.
        unsafe {
            ptr::write(out, Box::into_raw(Box::new(MbpBoundInput(value))));
        }
        Ok(())
    })())
}
/// # Safety
/// Input is readable and the separate output pointer slot is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbp_bound_decode(
    data: *const u8,
    length: u32,
    out: *mut *mut MbpBoundInput,
) -> u32 {
    status((|| {
        output(
            out,
            size_of::<*mut MbpBoundInput>() as u32,
            &[region(data.addr(), length as usize)?],
        )?;
        // SAFETY: caller input remains live through bounded decoding and copying.
        let value =
            BoundInput::decode(unsafe { bytes(data, length, crate::MAX_BOUND_INPUT_BYTES) }?)?;
        // SAFETY: output was checked and receives the new owner only on success.
        unsafe {
            ptr::write(out, Box::into_raw(Box::new(MbpBoundInput(value))));
        }
        Ok(())
    })())
}
/// # Safety
/// Null or one unique live owner; all borrows must have ended.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbp_bound_free(value: *mut MbpBoundInput) {
    if !value.is_null() {
        // SAFETY: caller transfers its original allocation once.
        unsafe {
            drop(Box::from_raw(value));
        }
    }
}
/// # Safety
/// Owner is live; the writable view prefix is disjoint from all owner storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbp_bound_proposal_view(
    value: *const MbpBoundInput,
    out: *mut ProposalView,
    size: u32,
) -> u32 {
    status((|| {
        aligned(value)?;
        // SAFETY: caller retains the live original owner.
        let value = unsafe { &*value };
        output(out, size, &bound_regions(value)?)?;
        // SAFETY: output borrows only checked read-only original storage.
        unsafe {
            ptr::write(out, view(value.0.proposal()));
        }
        Ok(())
    })())
}
/// # Safety
/// Owner is live; the writable identity prefix is disjoint from all owner storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbp_bound_identity(
    value: *const MbpBoundInput,
    out: *mut IdentityView,
    size: u32,
) -> u32 {
    status((|| {
        aligned(value)?;
        // SAFETY: caller retains a live original owner.
        let value = unsafe { &*value };
        output(out, size, &bound_regions(value)?)?;
        let id = value.0.identity();
        // SAFETY: checked output receives a copy, not a live capability.
        unsafe {
            ptr::write(
                out,
                IdentityView {
                    abi_version: 1,
                    struct_size: size_of::<IdentityView>() as u32,
                    transfer_epoch: id.transfer_epoch,
                    object_ref: id.object_ref,
                    operation_id: id.operation_id,
                },
            );
        }
        Ok(())
    })())
}
/// # Safety
/// Owner and disjoint output/length buffers obey the same contract as proposal_encode.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbp_bound_encode(
    value: *const MbpBoundInput,
    out: *mut u8,
    capacity: u32,
    written: *mut u32,
) -> u32 {
    status((|| {
        aligned(value)?;
        // SAFETY: caller retains the original owner during copying.
        let value = unsafe { &*value };
        // SAFETY: encode checks all supplied writable ranges against owned storage.
        unsafe {
            encode(
                value.0.wire(),
                out,
                capacity,
                written,
                &bound_regions(value)?,
            )
        }
    })())
}

#[cfg(test)]
mod tests;
