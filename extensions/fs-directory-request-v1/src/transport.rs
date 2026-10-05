//! One bounded call, verified against the original request; never retries.
use crate::{Error,Request,Response,Result,MAX_RESPONSE_BYTES};
pub fn call_once_with(request:&Request,call:impl FnOnce(&[u8],&mut [u8])->i32)->Result<Response> {
    Response::decode_for(&exchange_once_with(request,call)?,request)
}
pub(crate) fn exchange_once_with(request:&Request,call:impl FnOnce(&[u8],&mut [u8])->i32)->Result<Vec<u8>> {
    let mut output=vec![0u8;MAX_RESPONSE_BYTES];
    let size=call(request.wire(),&mut output);
    if size<=0||size as usize>output.len(){return Err(Error::OutcomeUnknown);}
    output.truncate(size as usize);Ok(output)
}
#[cfg(target_arch="wasm32")]
#[link(wasm_import_module="morrow_fs_directory_v1")]
unsafe extern "C" {#[link_name="call"] fn directory_call(input:*const u8,length:u32,output:*mut u8,capacity:u32)->i32;}
#[cfg(target_arch="wasm32")]
pub fn call_once(request:&Request)->Result<Response> {
    Response::decode_for(&exchange_once(request)?,request)
}
#[cfg(target_arch="wasm32")]
pub(crate) fn exchange_once(request:&Request)->Result<Vec<u8>> {
    exchange_once_with(request,|input,output|{
        // SAFETY: owned bounded buffers remain live for the one synchronous import.
        unsafe{directory_call(input.as_ptr(),input.len() as u32,output.as_mut_ptr(),output.len() as u32)}
    })
}
#[cfg(not(target_arch="wasm32"))]
pub fn call_once(_request:&Request)->Result<Response> {Err(Error::Unsupported)}
#[cfg(not(target_arch="wasm32"))]
pub(crate) fn exchange_once(_request:&Request)->Result<Vec<u8>> {Err(Error::Unsupported)}
