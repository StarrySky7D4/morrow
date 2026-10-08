//! One real R2 proposal; host approval and execution remain separate native operations.
#![deny(unsafe_code)]
#[cfg(target_arch = "wasm32")]
#[allow(unsafe_code)]
mod ffi;

#[cfg(target_arch = "wasm32")]
struct Capture {
    import: ffi::Import,
    request: Option<Vec<u8>>,
    reply: Option<Vec<u8>>,
}
#[cfg(target_arch = "wasm32")]
impl morrow_codex_session_exec_client_r2::Transport for Capture {
    fn exchange_once(&mut self, request: &[u8]) -> Result<Vec<u8>, ()> {
        if self.request.is_some() {
            return Err(());
        }
        self.request = Some(request.to_vec());
        let reply = morrow_codex_session_exec_client_r2::Transport::exchange_once(
            &mut self.import,
            request,
        )?;
        self.reply = Some(reply.clone());
        Ok(reply)
    }
}

#[cfg(target_arch = "wasm32")]
fn run() -> Result<(), ()> {
    use morrow_codex_session_exec_client_r2::{
        Budget, Capabilities, Client, Scope,
        protocol::{Action, MAX_FRAME_BYTES, Reply, Request, ToolPhase},
    };
    use morrow_plugin_sdk::wasm;
    let task = wasm::read_task().map_err(|_| ())?;
    let transform = task.transform().ok_or(())?;
    if transform.handler != "codex.session.propose"
        || transform.input_type != "codex.session.proposal.v1"
        || transform.output_type != "codex.session.proposal.receipt.v1"
    {
        return Err(());
    }
    let input = Request::decode(&transform.input).map_err(|_| ())?;
    let Action::Propose { session_id, intent } = input.action() else {
        return Err(());
    };
    let nonce: [u8; 16] = input.digest()[..16].try_into().map_err(|_| ())?;
    let scope = Scope {
        capabilities: Capabilities {
            read: true,
            propose: true,
            ..Default::default()
        },
        sessions: vec![session_id.clone()],
        operations: vec![intent.operation_id.clone()],
        execution_domain: intent.execution_domain.clone(),
    };
    let transport = Capture {
        import: ffi::Import,
        request: None,
        reply: None,
    };
    let mut client = Client::new(
        transport,
        input.generation(),
        nonce,
        scope,
        Budget {
            calls: 1,
            request_bytes: MAX_FRAME_BYTES,
            reply_bytes: MAX_FRAME_BYTES,
        },
    )
    .map_err(|_| ())?;
    let proposed = client.propose(session_id, intent.clone()).map_err(|_| ())?;
    if proposed.phase != ToolPhase::Proposed {
        return Err(());
    }
    let capture = client.into_transport();
    let actual_request = Request::decode(capture.request.as_ref().ok_or(())?).map_err(|_| ())?;
    let actual_reply = capture.reply.ok_or(())?;
    Reply::decode_for(&actual_request, &actual_reply).map_err(|_| ())?;
    // Only actual bytes supplied by the original host become a task output.
    wasm::complete_output(&task, &actual_reply).map_err(|_| ())
}

#[cfg(target_arch = "wasm32")]
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn morrow_run() -> i32 {
    match run() {
        Ok(()) => 0,
        Err(()) => -1,
    }
}
