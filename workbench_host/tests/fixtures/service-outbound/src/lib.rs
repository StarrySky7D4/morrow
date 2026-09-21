//! Test-only actual Rust service guest using current core codecs. This is not
//! a public SDK example or a production plugin and carries no native authority.
#![cfg(target_arch = "wasm32")]
use morrow_core::{io, service, service_resources};

#[link(wasm_import_module = "morrow_task_v1")]
unsafe extern "C" {
    fn read_input(output: *mut u8, capacity: u32) -> i32;
    fn complete(input: *const u8, length: u32) -> i32;
}
#[link(wasm_import_module = "morrow_io_v1")]
unsafe extern "C" {
    #[link_name = "call"]
    fn io_call(input: *const u8, length: u32, output: *mut u8, capacity: u32) -> i32;
}
fn run() -> Result<(), ()> {
    let mut input = vec![0u8; service::MAX_FRAME_BYTES];
    // SAFETY: the exclusive owned allocation covers this exact capacity and
    // remains live until the synchronous import copies the request into it.
    let length = unsafe { read_input(input.as_mut_ptr(), input.len() as u32) } as usize;
    if length == 0 || length > input.len() {
        return Err(());
    }
    let request = service::Request::decode(&input[..length]).map_err(|_| ())?;
    let io_request = io::Request::decode(&request.invocation().body).map_err(|_| ())?;
    let io_request = if let Some(directory) =
        service_resources::Directory::from_headers(&request.invocation().headers).map_err(|_| ())?
    {
        let endpoint = directory.endpoints.first().ok_or(())?;
        let io::Action::SubmitHttp(template) = io_request.action() else {
            return Err(());
        };
        let mut submission = template.clone();
        // The caller's body cannot choose an unapproved endpoint or credential.
        // This test guest chooses the first resource explicitly selected by UI.
        submission.endpoint = endpoint.reference.as_bytes().to_vec();
        submission.credential = endpoint.credential.clone();
        io::Request::encode_http_submit(io_request.call_id(), &submission).map_err(|_| ())?
    } else {
        io_request
    };
    let mut output = vec![0u8; io::MAX_FRAME_BYTES];
    // SAFETY: request and output are disjoint, bounded owned allocations. Both
    // stay live for the whole import; the runtime validates their wasm ranges.
    let length = unsafe {
        io_call(
            io_request.bytes().as_ptr(),
            io_request.bytes().len() as u32,
            output.as_mut_ptr(),
            output.len() as u32,
        )
    } as usize;
    if length == 0 || length > output.len() {
        return Err(());
    }
    io::Response::decode_http(&io_request, &output[..length]).map_err(|_| ())?;
    output.truncate(length);
    let response = service::Response::encode(
        &request,
        &service::Reply {
            status: 202,
            headers: vec![],
            body: output,
        },
    )
    .map_err(|_| ())?;
    // SAFETY: the fully initialized bounded encoded response remains allocated
    // until the synchronous import has copied it. No host pointer is retained.
    if unsafe { complete(response.as_ptr(), response.len() as u32) } < 0 {
        return Err(());
    }
    Ok(())
}
// SAFETY: this fixture defines the sole export of this fixed runtime ABI.
#[unsafe(no_mangle)]
pub extern "C" fn morrow_run() -> i32 {
    if run().is_ok() { 0 } else { -1 }
}
