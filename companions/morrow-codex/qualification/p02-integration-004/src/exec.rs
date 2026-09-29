//! Actual upstream dispatcher interception, with no successful process substitute.
use crate::deny;
use codex_exec_server::{Environment, ExecBackend, ExecBackendFuture, ExecParams, ExecServerError};
use kit::fake::{FakeHost, FixtureIdentity, hello};
use morrow_agent_host_contract::{self as kit, agent_host_capnp as wire};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy)]
enum Mode {
    Disconnected,
    ProposeOnly,
}
pub(crate) struct Boundary {
    mode: Mode,
    calls: Mutex<Vec<Value>>,
}

pub(crate) fn context() -> (Arc<Boundary>, Arc<deny::DenyCapabilities>, Environment) {
    let backend = Arc::new(Boundary {
        mode: Mode::Disconnected,
        calls: Mutex::new(vec![]),
    });
    let denied = Arc::new(deny::DenyCapabilities::default());
    let env =
        Environment::with_injected_capabilities(backend.clone(), denied.clone(), denied.clone());
    (backend, denied, env)
}

pub(crate) fn calls(backend: &Boundary) -> Vec<Value> {
    backend.calls.lock().unwrap().clone()
}
struct Disconnected;
impl kit::Transport for Disconnected {
    fn exchange(&mut self, request: &[u8]) -> Result<Vec<u8>, kit::Error> {
        kit::decode(request)?;
        Err(kit::Error::Invalid)
    }
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|v| format!("{v:02x}")).collect()
}
impl ExecBackend for Boundary {
    fn start(&self, params: ExecParams) -> ExecBackendFuture<'_> {
        Box::pin(async move {
            let payload = serde_json::to_vec(&params).unwrap();
            let identity = FixtureIdentity::default();
            let mut msg = kit::frame(2, &identity.session_id, identity.instance_epoch);
            let mut tool = msg.get_root::<wire::frame::Builder>().unwrap().init_tool();
            tool.set_operation_id("fixture-exec-operation");
            let mut intent = tool.init_propose();
            intent.set_root_operation("fixture-exec-root");
            intent.set_attempt_id("fixture-exec-attempt");
            intent.set_input_digest(&kit::digest(&payload));
            intent.set_tool_schema_digest(&kit::digest(b"qualification-only:ExecParams-json"));
            intent.set_executor_artifact(&identity.artifact_digest);
            let mut source = intent.init_source();
            source.set_namespace("fixture");
            source.set_id("source");
            source.set_revision(9007199254740993);
            source.set_byte_length(0);
            source.set_sha256(&kit::digest(b"fixture-reference"));
            let bytes = kit::encode(&msg).unwrap();
            self.calls.lock().unwrap().push(json!({"method":"ExecBackend::start", "params":params,"params_sha256":hex(&kit::digest(&payload)),"proposal_bytes":bytes.len(),"proposal_sha256":hex(&kit::digest(&bytes))}));
            match self.mode {
                Mode::Disconnected => {
                    let error = kit::exchange_checked(&mut Disconnected, &bytes)
                        .err()
                        .expect("disconnected transport");
                    Err(ExecServerError::Protocol(format!(
                        "qualification-disconnected:{error}"
                    )))
                }
                Mode::ProposeOnly => {
                    let mut host = FakeHost::new(identity.clone(), vec![]).unwrap();
                    kit::exchange_checked(&mut host, &hello(&identity, 1)).unwrap();
                    let response = kit::exchange_checked(&mut host, &bytes).unwrap();
                    let frame = response.get_root::<wire::frame::Reader>().unwrap();
                    let wire::frame::Which::Reply(reply) = frame.which().unwrap() else {
                        panic!("reply required")
                    };
                    let reply = reply.unwrap();
                    assert!(reply.get_qualification_only());
                    let wire::reply::Which::Tool(receipt) = reply.which().unwrap() else {
                        panic!("tool receipt required")
                    };
                    let receipt = receipt.unwrap();
                    assert_eq!(receipt.get_state().unwrap(), kit::ToolState::Proposed);
                    assert!(!receipt.get_execute());
                    // Never manufacture a StartedExecProcess from a proposal receipt.
                    Err(ExecServerError::Protocol("qualification-unsupported:003 has no argv execution, PTY or process IO contract; requires M-06 (related M-02)".into()))
                }
            }
        })
    }
}
pub(crate) async fn qualify() -> Vec<Value> {
    let mut cases = vec![];
    for (mode, tty, expected) in [
        (Mode::Disconnected, false, "qualification-disconnected"),
        (Mode::Disconnected, true, "qualification-disconnected"),
        (Mode::ProposeOnly, false, "qualification-unsupported"),
    ] {
        let backend = Arc::new(Boundary {
            mode,
            calls: Mutex::new(vec![]),
        });
        let denied = Arc::new(deny::DenyCapabilities::default());
        let environment = Environment::with_injected_capabilities(
            backend.clone(),
            denied.clone(),
            denied.clone(),
        );
        assert!(!environment.is_remote());
        let error = codex_core::morrow_p02_qualification::dispatch_refusal_probe(&environment, tty)
            .await
            .expect_err("must not start any process");
        let calls = backend.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        let params = &calls[0]["params"];
        assert_eq!(
            params["argv"],
            json!(["morrow-qualification-never-launch", "fixture-argument"])
        );
        assert_eq!(params["tty"], tty);
        assert!(params.get("shellSnapshot").is_none());
        assert_eq!(params["env"], json!({}));
        assert!(error.contains(expected), "{error}");
        assert!(denied.0.lock().unwrap().is_empty());
        cases.push(json!({"case":expected,"tty":tty,"calls":*calls,"error":error,"assertions": if matches!(mode, Mode::ProposeOnly) { 11 } else { 8 },"filesystem_http_adapter_calls":[]}));
    }
    cases
}
