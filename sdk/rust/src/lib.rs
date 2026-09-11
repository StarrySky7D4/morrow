//! Guest transport and typed protocol. No trusted core linkage or loader.
mod ffi;
pub mod protocol;
#[cfg(all(target_arch = "wasm32", feature = "wasm-guest"))]
pub mod wasm;
#[cfg(any(test, all(target_arch = "wasm32", feature = "wasm-c")))]
mod wasm_alloc;
#[allow(clippy::all)]
pub mod runtime_capnp {
    include!(concat!(env!("OUT_DIR"), "/runtime_capnp.rs"));
}
use std::{ffi::c_void, marker::PhantomData};
pub const MAX_MESSAGE_BYTES: usize = 65536;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct HostV1 {
    pub abi_version: u32,
    pub struct_size: u32,
    pub context: *mut c_void,
    pub exchange:
        Option<unsafe extern "C" fn(*mut c_void, *const u8, u32, *mut u8, u32, *mut u32) -> u32>,
}
#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    InvalidArgument,
    AbiMismatch,
    Limit,
    NoMemory,
    TransportFailure,
    BadReply,
}
/// Session is neither Send nor Sync. No pointers or Rust ABI values cross IPC.
pub struct Client<'a> {
    host: HostV1,
    lifetime: PhantomData<&'a HostV1>,
    local: PhantomData<*mut ()>,
}
impl<'a> Client<'a> {
    /// # Safety
    /// Callback/context remain valid for this client's lifetime; callback honors
    /// buffer bounds, does not retain pointers or unwind/re-enter, and validates
    /// the actual bound channel. This API provides no isolation from native code.
    pub unsafe fn from_host(host: &'a HostV1) -> Result<Self, Error> {
        if host.abi_version != 1 || host.struct_size < (size_of::<HostV1>() as u32) {
            return Err(Error::AbiMismatch);
        }
        if host.exchange.is_none() {
            return Err(Error::InvalidArgument);
        }
        Ok(Self {
            host: *host,
            lifetime: PhantomData,
            local: PhantomData,
        })
    }
    /// Returned bytes still require fixed-schema decoding and request correlation.
    /// Transport failure never proves rollback and is never retried here.
    pub fn exchange(&mut self, request: &[u8]) -> Result<Vec<u8>, Error> {
        if request.is_empty() {
            return Err(Error::InvalidArgument);
        }
        if request.len() > MAX_MESSAGE_BYTES {
            return Err(Error::Limit);
        }
        let mut fixed = Vec::new();
        fixed
            .try_reserve_exact(request.len())
            .map_err(|_| Error::NoMemory)?;
        fixed.extend_from_slice(request);
        let mut output = Vec::new();
        output
            .try_reserve_exact(MAX_MESSAGE_BYTES)
            .map_err(|_| Error::NoMemory)?;
        output.resize(MAX_MESSAGE_BYTES, 0);
        let mut written = 0;
        // SAFETY: constructor contract; both owned buffers stay alive and exclusive.
        let status = unsafe {
            self.host.exchange.unwrap()(
                self.host.context,
                fixed.as_ptr(),
                fixed.len() as u32,
                output.as_mut_ptr(),
                MAX_MESSAGE_BYTES as u32,
                &mut written,
            )
        };
        if status != 0 {
            return Err(Error::TransportFailure);
        }
        if written == 0 || written as usize > MAX_MESSAGE_BYTES {
            return Err(Error::BadReply);
        }
        output.truncate(written as usize);
        Ok(output)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    struct State {
        calls: u32,
        status: u32,
        length: Option<u32>,
    }
    unsafe extern "C" fn exchange(
        ctx: *mut c_void,
        request: *const u8,
        length: u32,
        output: *mut u8,
        capacity: u32,
        written: *mut u32,
    ) -> u32 {
        assert_eq!(capacity, MAX_MESSAGE_BYTES as u32);
        // SAFETY: synthetic test adapter uses live disjoint buffers/context.
        unsafe {
            let s = &mut *ctx.cast::<State>();
            s.calls += 1;
            std::ptr::copy_nonoverlapping(request, output, length as usize);
            *written = s.length.unwrap_or(length);
            s.status
        }
    }
    fn host(s: &mut State) -> HostV1 {
        HostV1 {
            abi_version: 1,
            struct_size: size_of::<HostV1>() as u32,
            context: (s as *mut State).cast(),
            exchange: Some(exchange),
        }
    }
    #[test]
    fn exact_binary_reply_and_one_submission() {
        let mut s = State {
            calls: 0,
            status: 0,
            length: None,
        };
        let h = host(&mut s);
        let mut c = unsafe { Client::from_host(&h) }.unwrap();
        assert_eq!(c.exchange(&[0, 255, 7]).unwrap(), [0, 255, 7]);
        assert_eq!(s.calls, 1);
    }
    #[test]
    fn invalid_and_oversized_input_do_not_submit() {
        let mut s = State {
            calls: 0,
            status: 0,
            length: None,
        };
        let h = host(&mut s);
        let mut c = unsafe { Client::from_host(&h) }.unwrap();
        assert_eq!(c.exchange(&[]), Err(Error::InvalidArgument));
        assert_eq!(
            c.exchange(&vec![0; MAX_MESSAGE_BYTES + 1]),
            Err(Error::Limit)
        );
        assert_eq!(s.calls, 0);
    }
    #[test]
    fn failure_never_retries_or_exposes_partial_bytes() {
        let mut s = State {
            calls: 0,
            status: 99,
            length: None,
        };
        let h = host(&mut s);
        let mut c = unsafe { Client::from_host(&h) }.unwrap();
        assert_eq!(c.exchange(&[9]), Err(Error::TransportFailure));
        assert_eq!(s.calls, 1);
    }
    #[test]
    fn impossible_reply_lengths_rejected() {
        for length in [0, MAX_MESSAGE_BYTES as u32 + 1] {
            let mut s = State {
                calls: 0,
                status: 0,
                length: Some(length),
            };
            let h = host(&mut s);
            let mut c = unsafe { Client::from_host(&h) }.unwrap();
            assert_eq!(c.exchange(&[9]), Err(Error::BadReply));
            assert_eq!(s.calls, 1);
        }
    }
    #[test]
    fn unsupported_abi_and_missing_callback_rejected() {
        let mut s = State {
            calls: 0,
            status: 0,
            length: None,
        };
        let mut h = host(&mut s);
        h.abi_version = 2;
        assert!(matches!(
            unsafe { Client::from_host(&h) },
            Err(Error::AbiMismatch)
        ));
        h.abi_version = 1;
        h.exchange = None;
        assert!(matches!(
            unsafe { Client::from_host(&h) },
            Err(Error::InvalidArgument)
        ));
        assert_eq!(s.calls, 0);
    }
}
