use morrow_agent_session_exec_v1_r2::{
    Action, Outcome, Reply, Request,
    authority::{Admission, Capabilities, SessionExecHost},
    hash,
};
use morrow_core::{
    dispatch::{Connection, HostRuntime},
    store::{EventBudget, Store},
    task::{Invocation, Transform},
};
use morrow_plugin_runtime::{Cancellation, Fault, Limits, Runner};
use std::path::PathBuf;

struct Host {
    _temp: tempfile::TempDir,
    runtime: HostRuntime,
    connection: Connection,
    host: SessionExecHost,
    admission: Admission,
    count: usize,
}
impl Host {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let mut runtime = HostRuntime::new(
            Store::open(
                &temp.path().join("real-wasm.sqlite"),
                EventBudget::default(),
            )
            .unwrap(),
        )
        .unwrap();
        let connection = runtime.connect().unwrap();
        let host = SessionExecHost::new(&mut runtime).unwrap();
        let caps = Capabilities {
            session_read: true,
            session_write: true,
            propose: false,
            execute: false,
            retire: false,
        };
        let admission = host
            .admit(
                &runtime,
                &connection,
                caps,
                caps,
                vec!["guest-session".into(), "guest-session-child".into()],
                "session-only".into(),
                100,
                1,
            )
            .unwrap();
        Self {
            _temp: temp,
            runtime,
            connection,
            host,
            admission,
            count: 0,
        }
    }
    fn call(&mut self, bytes: &[u8]) -> Result<Vec<u8>, ()> {
        self.count += 1;
        self.host
            .dispatch(
                &mut self.runtime,
                &self.connection,
                &self.admission,
                bytes,
                || 1,
            )
            .map_err(|_| ())
    }
    fn snapshot(&mut self, session_id: &str) -> Outcome {
        let request = Request::new(
            "verify-host-state",
            Action::Snapshot {
                session_id: session_id.into(),
                after: 0,
                limit: 16,
            },
        )
        .unwrap();
        let raw = self.call(request.raw()).unwrap();
        Reply::decode_for(&request, &raw).unwrap().outcome
    }
}
fn wasm() -> Vec<u8> {
    let path = PathBuf::from(
        std::env::var_os("MORROW_CODEX_R2_GUEST").expect("explicit compiled guest path required"),
    );
    assert!(path.is_absolute());
    std::fs::read(path).unwrap()
}
fn limits() -> Limits {
    Limits {
        fuel: 100_000_000,
        memory_bytes: 16 * 1024 * 1024,
        host_calls: 7,
    }
}
fn invocation() -> Invocation {
    let mut config = 1u64.to_le_bytes().to_vec();
    config.extend_from_slice(&[7; 16]);
    config.extend_from_slice(b"guest-session");
    Invocation::new_transform(
        "host-bound-task",
        Transform {
            handler: "codex.session.continue".into(),
            input_type: "codex.session.config.v1".into(),
            output_type: "codex.session.receipt.v1".into(),
            input: config,
        },
    )
    .unwrap()
}
#[test]
fn compiled_rust_wasm_guest_runs_seven_calls_on_original_sqlite_host() {
    let module = wasm();
    let runner = Runner::new_agent_session_exec_task(&module, limits()).unwrap();
    let task = invocation();
    let mut host = Host::new();
    let result = runner.run_agent_session_exec_task(
        task.bytes(),
        &mut |b| host.call(b),
        Cancellation::default(),
    );
    assert_eq!(result.report.outcome, Ok(0), "{result:?}");
    assert_eq!(result.report.host_calls, 7);
    assert_eq!(host.count, 7);
    let output = task
        .verify_output(result.completion.as_ref().unwrap())
        .unwrap();
    let mut expected = 1u64.to_le_bytes().to_vec();
    expected.extend_from_slice(&hash(b"sealed-state\0\xff"));
    expected.extend_from_slice(&1u64.to_le_bytes());
    assert_eq!(output.bytes, expected);
    match host.snapshot("guest-session-child") {
        Outcome::Snapshot(s) => {
            assert_eq!(s.checkpoint, b"sealed-state\0\xff");
            assert_eq!(s.info.parent_tail, 1);
            assert!(s.events.is_empty());
        }
        other => panic!("{other:?}"),
    }
}
#[test]
fn actual_rust_guest_is_rejected_by_all_original_factories() {
    let module = wasm();
    for result in [
        Runner::new(&module, limits()),
        Runner::new_task(&module, limits()),
        Runner::new_io_task(&module, limits()),
        Runner::new_directory_task(&module, limits()),
    ] {
        assert!(matches!(result, Err(Fault::UnsupportedAbi)));
    }
}
#[test]
fn real_wasm_revocation_stops_before_followup_mutations() {
    let runner = Runner::new_agent_session_exec_task(&wasm(), limits()).unwrap();
    let task = invocation();
    let mut host = Host::new();
    let result = runner.run_agent_session_exec_task(
        task.bytes(),
        &mut |b| {
            let reply = host.call(b)?;
            if host.count == 1 {
                host.host.revoke(&host.admission).unwrap();
            }
            Ok(reply)
        },
        Cancellation::default(),
    );
    assert_eq!(result.report.outcome, Err(Fault::TaskProtocol));
    assert!(result.completion.is_none());
    assert_eq!(host.count, 2);
    assert_eq!(result.report.host_calls, 2);
}
#[test]
fn real_wasm_mismatched_reply_does_not_retry_or_complete() {
    let runner = Runner::new_agent_session_exec_task(&wasm(), limits()).unwrap();
    let task = invocation();
    let mut host = Host::new();
    let result = runner.run_agent_session_exec_task(
        task.bytes(),
        &mut |b| {
            let request = Request::decode(b).unwrap();
            let mut reply = Reply::decode_for(&request, &host.call(b)?).unwrap();
            reply.generation = 2;
            reply.encode().map_err(|_| ())
        },
        Cancellation::default(),
    );
    assert_eq!(result.report.outcome, Err(Fault::TaskProtocol));
    assert!(result.completion.is_none());
    assert_eq!(host.count, 1);
    assert_eq!(result.report.host_calls, 1);
    assert!(
        matches!(host.snapshot("guest-session"),Outcome::Snapshot(s) if s.info.epoch==0 && s.info.tail==0)
    );
}
#[test]
fn real_wasm_lost_reply_retains_committed_effect_without_replay() {
    let runner = Runner::new_agent_session_exec_task(&wasm(), limits()).unwrap();
    let task = invocation();
    let mut host = Host::new();
    let result = runner.run_agent_session_exec_task(
        task.bytes(),
        &mut |b| {
            let reply = host.call(b)?;
            if host.count == 3 { Err(()) } else { Ok(reply) }
        },
        Cancellation::default(),
    );
    assert!(result.report.outcome.is_err());
    assert!(result.completion.is_none());
    assert_eq!(host.count, 3);
    assert_eq!(result.report.host_calls, 3);
    assert!(
        matches!(host.snapshot("guest-session"),Outcome::Snapshot(s) if s.info.tail==1 && s.events.len()==1)
    );
}
#[test]
fn real_wasm_host_call_budget_blocks_extra_dispatch() {
    let runner = Runner::new_agent_session_exec_task(
        &wasm(),
        Limits {
            host_calls: 2,
            ..limits()
        },
    )
    .unwrap();
    let task = invocation();
    let mut host = Host::new();
    let result = runner.run_agent_session_exec_task(
        task.bytes(),
        &mut |b| host.call(b),
        Cancellation::default(),
    );
    assert_eq!(result.report.outcome, Err(Fault::Limits));
    assert!(result.completion.is_none());
    assert_eq!(host.count, 2);
    assert_eq!(result.report.host_calls, 2);
    assert!(
        matches!(host.snapshot("guest-session"),Outcome::Snapshot(s) if s.info.epoch==1 && s.info.tail==0)
    );
}
