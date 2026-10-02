//! A local callback context is never serialized and grants no authority.
use super::{MAX_WIRE_BYTES, Request, Response};
use sha2::{Digest, Sha256};
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
    output: Vec<u8>,
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
            // Keep construction and invalid-request errors allocation-free.
            output: Vec::new(),
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
        // Freeze/hash the actual owned input before submitting. The input and
        // reusable output are separate allocations and never alias.
        let digest = Sha256::digest(&encoded).into();
        if self.output.is_empty() {
            self.output = buffer()?;
        }
        let mut written = 0;
        let code = unsafe {
            self.host.call.unwrap()(
                self.host.context,
                encoded.as_ptr(),
                encoded.len() as u32,
                self.output.as_mut_ptr(),
                MAX_WIRE_BYTES as u32,
                &mut written,
            )
        };
        let result = (|| {
            if code != 0 {
                return Err(crate::Error::TransportFailure);
            }
            if written == 0 || written as usize > MAX_WIRE_BYTES {
                return Err(crate::Error::BadReply);
            }
            let response = Response::decode(&self.output[..written as usize])
                .map_err(|_| crate::Error::BadReply)?;
            response
                .validate_for_digest(request, &digest)
                .map_err(|_| crate::Error::BadReply)?;
            Ok(response)
        })();
        // Native callbacks may scratch anywhere in capacity, including after
        // the written prefix and on failure. Always clear the entire buffer.
        self.output.fill(0);
        result
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
    let digest = Sha256::digest(&input).into();
    // Preserve the one-shot API's invalid-request-before-allocation priority.
    WasmClient::new()?.call_encoded(request, &input, &digest)
}

/// Bounded reusable output for the trusted prefix-writing Wasm import.
/// Every call is one submission and returns a fully owned response.
#[cfg(all(target_arch = "wasm32", feature = "wasm-guest"))]
pub struct WasmClient {
    output: Vec<u8>,
}
#[cfg(all(target_arch = "wasm32", feature = "wasm-guest"))]
impl WasmClient {
    pub fn new() -> Result<Self, crate::Error> {
        Ok(Self { output: buffer()? })
    }
    pub fn call(&mut self, request: &Request) -> Result<Response, crate::Error> {
        let input = request
            .encode()
            .map_err(|_| crate::Error::InvalidArgument)?;
        let digest = Sha256::digest(&input).into();
        self.call_encoded(request, &input, &digest)
    }
    fn call_encoded(
        &mut self,
        request: &Request,
        input: &[u8],
        digest: &[u8; 32],
    ) -> Result<Response, crate::Error> {
        let written = unsafe {
            channel_call(
                input.as_ptr(),
                input.len() as u32,
                self.output.as_mut_ptr(),
                MAX_WIRE_BYTES as u32,
            )
        };
        if written <= 0 || written as usize > MAX_WIRE_BYTES {
            // No valid prefix is available on transport failure.
            self.output.fill(0);
            return Err(crate::Error::TransportFailure);
        }
        let written = written as usize;
        let result = (|| {
            let response =
                Response::decode(&self.output[..written]).map_err(|_| crate::Error::BadReply)?;
            response
                .validate_for_digest(request, digest)
                .map_err(|_| crate::Error::BadReply)?;
            Ok(response)
        })();
        // The import writes only this validated-length prefix; decode has copied
        // every returned span before reusable storage is cleared.
        self.output[..written].fill(0);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channel::{Action, Status};

    struct State {
        reply: Vec<u8>,
        code: u32,
        written: u32,
        calls: usize,
    }
    unsafe extern "C" fn scratch(
        context: *mut c_void,
        _input: *const u8,
        _length: u32,
        output: *mut u8,
        capacity: u32,
        written: *mut u32,
    ) -> u32 {
        let state = unsafe { &mut *context.cast::<State>() };
        state.calls += 1;
        let output = unsafe { std::slice::from_raw_parts_mut(output, capacity as usize) };
        assert!(output.iter().all(|byte| *byte == 0));
        output.fill(0xa5);
        output[..state.reply.len()].copy_from_slice(&state.reply);
        unsafe { *written = state.written };
        state.code
    }
    fn request() -> Request {
        Request {
            call_id: [1; 32],
            reference: [2; 32],
            source_epoch: [3; 32],
            action: Action::Query,
        }
    }
    fn reply(request: &Request) -> Vec<u8> {
        Response {
            call_id: request.call_id,
            request_sha256: request.digest().unwrap(),
            reference: request.reference,
            source_epoch: request.source_epoch,
            status: Status::Ready,
            frame: None,
            last_acked: 0,
            accepted_sequence: 0,
            resource_reclaimed: false,
        }
        .encode()
        .unwrap()
    }
    #[test]
    fn native_output_is_cleared_immediately_even_after_full_capacity_scratch() {
        let request = request();
        let good = reply(&request);
        let mut noncanonical = good.clone();
        noncanonical.push(0);
        let mut uncorrelated = Response::decode(&good).unwrap();
        uncorrelated.call_id = [4; 32];
        let bad_identity = uncorrelated.encode().unwrap();
        let mut receive = request.clone();
        receive.action = Action::Receive {
            last_acked: 0,
            credit_bytes: 1,
        };
        // Ready is well-formed, but not an admissible Receive response.
        let wrong_action = reply(&receive);
        let mut state = State {
            reply: good.clone(),
            code: 0,
            written: good.len() as u32,
            calls: 0,
        };
        let host = HostV1 {
            abi_version: 1,
            struct_size: size_of::<HostV1>() as u32,
            context: (&mut state as *mut State).cast(),
            call: Some(scratch),
        };
        let mut client = unsafe { Client::from_host(&host) }.unwrap();
        assert!(client.output.is_empty());
        let first = client.call(&request).unwrap();
        let pointer = client.output.as_ptr();
        let capacity = client.output.capacity();
        assert_eq!(client.output.len(), MAX_WIRE_BYTES);
        assert!(client.output.iter().all(|byte| *byte == 0));
        for (encoded, code, written, expected, submitted) in [
            (good.clone(), 9, 0, crate::Error::TransportFailure, &request),
            (
                good.clone(),
                9,
                MAX_WIRE_BYTES as u32 + 1,
                crate::Error::TransportFailure,
                &request,
            ),
            (good.clone(), 0, 0, crate::Error::BadReply, &request),
            (
                good.clone(),
                0,
                MAX_WIRE_BYTES as u32 + 1,
                crate::Error::BadReply,
                &request,
            ),
            (good.clone(), 0, 1, crate::Error::BadReply, &request),
            (vec![0xff; 8], 0, 8, crate::Error::BadReply, &request),
            (
                noncanonical.clone(),
                0,
                noncanonical.len() as u32,
                crate::Error::BadReply,
                &request,
            ),
            (
                bad_identity.clone(),
                0,
                bad_identity.len() as u32,
                crate::Error::BadReply,
                &request,
            ),
            (
                wrong_action.clone(),
                0,
                wrong_action.len() as u32,
                crate::Error::BadReply,
                &receive,
            ),
        ] {
            state.reply = encoded;
            state.code = code;
            state.written = written;
            assert_eq!(client.call(submitted), Err(expected));
            assert_eq!(client.output.as_ptr(), pointer);
            assert_eq!(client.output.capacity(), capacity);
            assert_eq!(client.output.len(), MAX_WIRE_BYTES);
            assert!(client.output.iter().all(|byte| *byte == 0));
        }
        assert_eq!(state.calls, 10);
        assert_eq!(first, Response::decode(&good).unwrap());
        state.reply = good.clone();
        state.code = 0;
        state.written = good.len() as u32;
        assert_eq!(client.call(&request).unwrap(), first);
        assert_eq!(state.calls, 11);
    }
}
