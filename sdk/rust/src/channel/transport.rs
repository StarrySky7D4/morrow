//! A local callback context is never serialized and grants no authority.
use super::{MAX_WIRE_BYTES, Request, Response};
use std::{ffi::c_void, marker::PhantomData};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct HostV1 {
    pub abi_version: u32,
    pub struct_size: u32,
    pub context: *mut c_void,
    pub call:
        Option<unsafe extern "C" fn(*mut c_void, *const u8, u32, *mut u8, u32, *mut u32) -> u32>,
}
pub struct Client<'a> {
    host: HostV1,
    lifetime: PhantomData<&'a HostV1>,
    local: PhantomData<*mut ()>,
}
fn buffer() -> Result<Vec<u8>, crate::Error> {
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(MAX_WIRE_BYTES)
        .map_err(|_| crate::Error::NoMemory)?;
    bytes.resize(MAX_WIRE_BYTES, 0);
    Ok(bytes)
}
impl<'a> Client<'a> {
    /// # Safety
    /// Callback/context remain valid for this client's lifetime. Callback respects
    /// bounds, retains no pointers, cannot unwind/reenter, and checks host binding.
    pub unsafe fn from_host(host: &'a HostV1) -> Result<Self, crate::Error> {
        if host.abi_version != 1 || host.struct_size < size_of::<HostV1>() as u32 {
            return Err(crate::Error::AbiMismatch);
        }
        if host.call.is_none() {
            return Err(crate::Error::InvalidArgument);
        }
        Ok(Self {
            host: *host,
            lifetime: PhantomData,
            local: PhantomData,
        })
    }
    /// One submission; failure never proves rollback. Callers explicitly inspect
    /// Unknown, ClosingUnconfirmed, Accepted and resource_reclaimed.
    pub fn call(&mut self, request: &Request) -> Result<Response, crate::Error> {
        let encoded = request
            .encode()
            .map_err(|_| crate::Error::InvalidArgument)?;
        let mut input = buffer()?;
        input[..encoded.len()].copy_from_slice(&encoded);
        let mut output = buffer()?;
        let mut written = 0;
        let code = unsafe {
            self.host.call.unwrap()(
                self.host.context,
                input.as_ptr(),
                encoded.len() as u32,
                output.as_mut_ptr(),
                MAX_WIRE_BYTES as u32,
                &mut written,
            )
        };
        if code != 0 {
            return Err(crate::Error::TransportFailure);
        }
        if written == 0 || written as usize > MAX_WIRE_BYTES {
            return Err(crate::Error::BadReply);
        }
        let response =
            Response::decode(&output[..written as usize]).map_err(|_| crate::Error::BadReply)?;
        response
            .validate_for(request)
            .map_err(|_| crate::Error::BadReply)?;
        Ok(response)
    }
}
#[cfg(all(target_arch = "wasm32", feature = "wasm-guest"))]
#[link(wasm_import_module = "morrow_channel_v1")]
unsafe extern "C" {
    #[link_name = "call"]
    fn channel_call(input: *const u8, length: u32, output: *mut u8, capacity: u32) -> i32;
}
#[cfg(all(target_arch = "wasm32", feature = "wasm-guest"))]
pub fn call_wasm(request: &Request) -> Result<Response, crate::Error> {
    let input = request
        .encode()
        .map_err(|_| crate::Error::InvalidArgument)?;
    let mut output = buffer()?;
    let written = unsafe {
        channel_call(
            input.as_ptr(),
            input.len() as u32,
            output.as_mut_ptr(),
            MAX_WIRE_BYTES as u32,
        )
    };
    if written <= 0 || written as usize > MAX_WIRE_BYTES {
        return Err(crate::Error::TransportFailure);
    }
    let response =
        Response::decode(&output[..written as usize]).map_err(|_| crate::Error::BadReply)?;
    response
        .validate_for(request)
        .map_err(|_| crate::Error::BadReply)?;
    Ok(response)
}
