//! One host-selected POST endpoint. No arbitrary URL, caller credentials or retries.
use morrow_plugin_sdk::{
    io,
    service::{Reply, Request},
    service_resources::Directory,
    wasm,
};
fn reply(status: u16, body: &[u8]) -> Reply {
    Reply {
        status,
        headers: vec![],
        body: body.to_vec(),
    }
}
fn forward(request: &Request) -> Result<Reply, ()> {
    let input = request.invocation();
    if input.method != "POST" {
        return Ok(reply(405, b"post-required"));
    }
    let directory = Directory::from_headers(&input.headers).map_err(|_| ())?;
    let Some(directory) = directory.filter(|d| d.endpoints.len() == 1) else {
        return Ok(reply(503, b"one-endpoint-required"));
    };
    let endpoint = &directory.endpoints[0];
    if !endpoint.methods.iter().any(|m| m == "POST") {
        return Ok(reply(403, b"method-denied"));
    }
    if input.body.len() as u64 > endpoint.max_request_bytes {
        return Ok(reply(413, b"request-too-large"));
    }
    let mut operation = String::from("service-http-");
    for b in request.digest() {
        use std::fmt::Write;
        write!(&mut operation, "{b:02x}").map_err(|_| ())?;
    }
    let outbound = io::Request::new(
        request.call_id(),
        io::Action::Submit(io::Submission {
            operation_id: operation.into_bytes(),
            deadline_ms: endpoint.timeout_ms.min(io::MAX_SUBMIT_DEADLINE_MS),
            kind: io::SubmissionKind::HttpRequest(io::HttpRequest {
                endpoint: endpoint.reference.as_bytes().to_vec(),
                credential: endpoint.credential.clone(),
                method: "POST".into(),
                relative_target: "/".into(),
                headers: vec![],
                body: input.body.clone(),
            }),
        }),
    )
    .map_err(|_| ())?;
    let response = wasm::call_io(&outbound).map_err(|_| ())?;
    Ok(match response.status {
        io::Status::Completed => reply(response.http_status, &response.bytes),
        io::Status::OutcomeUnknown => reply(409, b"outcome-unknown"),
        io::Status::Conflict => reply(409, b"operation-conflict"),
        io::Status::Denied | io::Status::Revoked => reply(403, b"outbound-denied"),
        io::Status::Expired => reply(504, b"outbound-expired"),
        io::Status::Quota => reply(429, b"outbound-quota"),
        _ => reply(502, b"outbound-unavailable"),
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn morrow_run() -> i32 {
    let Ok(request) = wasm::read_service_request() else {
        return -1;
    };
    let Ok(reply) = forward(&request) else {
        return -1;
    };
    if wasm::complete_service_response(&request, &reply).is_ok() {
        0
    } else {
        -1
    }
}
