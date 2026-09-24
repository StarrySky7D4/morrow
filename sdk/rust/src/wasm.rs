//! Experimental wasm32 guest import adapter; no WASI or host identity parameters.
use crate::HostV1;
use std::ffi::c_void;
#[link(wasm_import_module = "morrow_v1")]
unsafe extern "C" {
    fn exchange(input: *const u8, length: u32, output: *mut u8, capacity: u32) -> i32;
}
unsafe extern "C" fn adapter(
    _: *mut c_void,
    input: *const u8,
    length: u32,
    output: *mut u8,
    capacity: u32,
    written: *mut u32,
) -> u32 {
    // SAFETY: Client supplies live disjoint bounded buffers. Import is synchronous
    // and the selected backend validates guest offsets before reading or submitting.
    let result = unsafe { exchange(input, length, output, capacity) };
    if result <= 0 {
        return 1;
    }
    unsafe {
        *written = result as u32;
    }
    0
}
/// Host descriptor for the fixed import, with no caller-selected context/identity.
pub fn host() -> HostV1 {
    HostV1 {
        abi_version: 1,
        struct_size: size_of::<HostV1>() as u32,
        context: std::ptr::null_mut(),
        exchange: Some(adapter),
    }
}

#[link(wasm_import_module = "morrow_task_v1")]
unsafe extern "C" {
    fn read_input(output: *mut u8, capacity: u32) -> i32;
    fn complete(input: *const u8, length: u32) -> i32;
}
/// Read once from the host-bound invocation. Requires guest ABI v2.
pub fn read_task() -> Result<crate::task::Invocation, crate::Error> {
    let mut bytes = vec![0; crate::task::MAX_TASK_BYTES];
    // SAFETY: the owned buffer spans the advertised capacity; the backend validates offsets.
    let size = unsafe { read_input(bytes.as_mut_ptr(), bytes.len() as u32) };
    if size <= 0 || size as usize > bytes.len() {
        return Err(crate::Error::TransportFailure);
    }
    crate::task::Invocation::decode(&bytes[..size as usize]).map_err(|_| crate::Error::BadReply)
}
/// Submit one correlated completion. The host independently checks actual response provenance.
pub fn complete_task(task: &crate::task::Invocation, response: &[u8]) -> Result<(), crate::Error> {
    let bytes = task
        .completion(response)
        .map_err(|_| crate::Error::BadReply)?;
    // SAFETY: completion bytes remain live for the synchronous import and contain no pointers.
    if unsafe { complete(bytes.as_ptr(), bytes.len() as u32) } != 0 {
        return Err(crate::Error::TransportFailure);
    }
    Ok(())
}

/// Return correlated computed data, never a core receipt or direct content write.
pub fn complete_output(task: &crate::task::Invocation, output: &[u8]) -> Result<(), crate::Error> {
    let bytes = task.output(output).map_err(|_| crate::Error::BadReply)?;
    // SAFETY: owned completion remains live for the synchronous bounded import.
    if unsafe { complete(bytes.as_ptr(), bytes.len() as u32) } != 0 {
        return Err(crate::Error::TransportFailure);
    }
    Ok(())
}

/// Complete execution with a plugin-reported business failure. This does not retry the task.
pub fn complete_failure(
    task: &crate::task::Invocation,
    code: crate::task::FailureCode,
    message: &str,
) -> Result<(), crate::Error> {
    let bytes = task
        .failure(code, message)
        .map_err(|_| crate::Error::BadReply)?;
    // SAFETY: the owned bounded completion remains live for this synchronous import.
    if unsafe { complete(bytes.as_ptr(), bytes.len() as u32) } != 0 {
        return Err(crate::Error::TransportFailure);
    }
    Ok(())
}

#[link(wasm_import_module = "morrow_dependency_v1")]
unsafe extern "C" {
    #[link_name = "call"]
    fn dependency_call(input: *const u8, length: u32, output: *mut u8, capacity: u32) -> i32;
}
/// Explicit dependency transport. Requires the host's dependency task mode and a declared,
/// locked slot; this call does not choose the provider, grant scope, or commit content.
pub fn call_dependency(
    request: &crate::dependency_call::Request,
) -> Result<crate::dependency_call::Output, crate::Error> {
    let input = request.bytes();
    if input.is_empty() || input.len() > 128 * 1024 {
        return Err(crate::Error::Limit);
    }
    let mut response = vec![0; 128 * 1024];
    // SAFETY: owned disjoint buffers remain live for the synchronous import. No memory view
    // crosses a host boundary; the backend validates full ranges before routing the request.
    let size = unsafe {
        dependency_call(
            input.as_ptr(),
            input.len() as u32,
            response.as_mut_ptr(),
            response.len() as u32,
        )
    };
    if size <= 0 || size as usize > response.len() {
        return Err(crate::Error::TransportFailure);
    }
    request
        .verify_response(&response[..size as usize])
        .map_err(|_| crate::Error::BadReply)
}

#[link(wasm_import_module = "morrow_io_v1")]
unsafe extern "C" {
    #[link_name = "call"]
    fn io_call(input: *const u8, length: u32, output: *mut u8, capacity: u32) -> i32;
}

fn io_buffer() -> Result<Vec<u8>, crate::Error> {
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(crate::io::MAX_FRAME_BYTES)
        .map_err(|_| crate::Error::NoMemory)?;
    bytes.resize(crate::io::MAX_FRAME_BYTES, 0);
    Ok(bytes)
}

/// Read the IO-frame task mode's host-bound request once. This is separate from
/// `read_task`: a managed IO invocation contains an IO Request, not Invocation.
/// Decoding a resource reference grants no authority; the current host decides.
pub fn read_io_request() -> Result<crate::io::Request, crate::Error> {
    let mut bytes = io_buffer()?;
    // SAFETY: the live owned buffer covers the advertised capacity. The runtime
    // bounds-checks the guest memory before writing and enforces read-once.
    let size = unsafe { read_input(bytes.as_mut_ptr(), bytes.len() as u32) };
    if size <= 0 || size as usize > bytes.len() {
        return Err(crate::Error::TransportFailure);
    }
    crate::io::Request::decode(&bytes[..size as usize]).map_err(|_| crate::Error::BadReply)
}

/// Make exactly one IO call with the immutable original frame and verify the
/// response's schema, call ID and frame digest. This function never retries.
/// Transport/bad-reply errors do not prove rollback; retain operation identity.
/// A verified response can still report Denied, Unsupported or OutcomeUnknown.
pub fn call_io(request: &crate::io::Request) -> Result<crate::io::Response, crate::Error> {
    let input = request.bytes();
    let mut response = io_buffer()?;
    // SAFETY: owned disjoint buffers stay live throughout the synchronous import.
    // No host address, socket or file handle crosses the guest memory boundary.
    let size = unsafe {
        io_call(
            input.as_ptr(),
            input.len() as u32,
            response.as_mut_ptr(),
            response.len() as u32,
        )
    };
    if size <= 0 || size as usize > response.len() {
        return Err(crate::Error::TransportFailure);
    }
    request
        .decode_reply(&response[..size as usize])
        .map_err(|_| crate::Error::BadReply)
}

/// Complete the IO-frame mode with the exact verified host reply, without
/// synthesizing a receipt from exposed data fields. The host checks provenance
/// independently. This is not `complete_output` for a pure transformation.
pub fn complete_io_response(response: &crate::io::Response) -> Result<(), crate::Error> {
    let bytes = response.encoded_frame();
    // SAFETY: immutable reply storage covers the length and lives through the
    // import. The runtime enforces completion once and actual reply provenance.
    if unsafe { complete(bytes.as_ptr(), bytes.len() as u32) } != 0 {
        return Err(crate::Error::TransportFailure);
    }
    Ok(())
}
