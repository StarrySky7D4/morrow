//! Exercise the actual CLI stream loop with a real managed worker retaining the
//! original audited library. No mocked finish or synthetic Busy response.
use super::serve;
use morrow_core::{
    io::{HttpSubmission, Request},
    plugin_package::{
        Package,
        io::{self, IoCapability},
    },
};
use morrow_plugin_runtime::io_jobs::{BrokerRouter, JobLimits, RouteContext, RouterFault};
use morrow_workbench_host::{
    Workbench, host_capnp as wire,
    io_tasks::{PreparedJob, StartOptions, StoragePhase},
    protocol,
};
use std::{
    collections::BTreeSet,
    io::{self as stdio, Cursor, Read, Write},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};
const WAIT: Duration = Duration::from_secs(10);

struct HeldRouter {
    entered: mpsc::SyncSender<()>,
    release: mpsc::Receiver<()>,
    calls: Arc<AtomicUsize>,
}
impl BrokerRouter for HeldRouter {
    fn route(
        &mut self,
        _: &mut RouteContext<'_>,
        _: u32,
        _: &Request,
    ) -> Result<Vec<u8>, RouterFault> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.entered.send(()).map_err(|_| RouterFault::Unknown)?;
        self.release
            .recv_timeout(WAIT)
            .map_err(|_| RouterFault::Unknown)?;
        Err(RouterFault::Denied)
    }
}
fn start_held(root: &std::path::Path) -> (Workbench, mpsc::SyncSender<()>, Arc<AtomicUsize>) {
    let mut host = Workbench::open_managed(root, None).unwrap();
    let wasm = wat::parse_str(
        r#"(module
      (import "morrow_task_v1" "read_input" (func $read (param i32 i32) (result i32)))
      (import "morrow_task_v1" "complete" (func $done (param i32 i32) (result i32)))
      (import "morrow_io_v1" "call" (func $io (param i32 i32 i32 i32) (result i32)))
      (memory (export "memory") 4)
      (func (export "morrow_run") (result i32) (local $n i32)
        i32.const 0 i32.const 131072 call $read local.set $n
        i32.const 131072 i32.const 0 local.get $n i32.const 131072 i32.const 131072
        call $io call $done drop i32.const 0))"#,
    )
    .unwrap();
    let id = "test.cli-shutdown";
    let mut manifest = Package::manifest_for_task(id, "1.0.0", &wasm, vec![]);
    manifest.required_features.push(io::FEATURE.into());
    manifest.io_declaration = Some(io::declaration(
        vec![IoCapability::HttpRequest],
        vec!["io.invoke".into()],
    ));
    let package = Package::build(manifest, &wasm).unwrap();
    let path = root.join("incoming.mplugin");
    std::fs::write(&path, package.archive()).unwrap();
    let revision = host.catalog_page("", None).unwrap().revision;
    host.import_plugin(&path, &package.digest(), revision)
        .unwrap();
    let revision = host.catalog_page("", None).unwrap().revision;
    host.configure_external_io(id, &package.digest(), revision, &["http-request".into()])
        .unwrap();
    let revision = host.catalog_page("", None).unwrap().revision;
    host.configure_external(id, &package.digest(), revision, &[], true)
        .unwrap();
    let revision = host.catalog_page("", None).unwrap().revision;
    let (entered_tx, entered_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    let calls = Arc::new(AtomicUsize::new(0));
    let routed = calls.clone();
    host.start_io(
        StartOptions {
            package_id: id.into(),
            digest: package.digest(),
            revision,
            capabilities: BTreeSet::from([IoCapability::HttpRequest]),
            lifetime: WAIT,
            limits: JobLimits::new(1, 1024 * 1024, 4 * 1024 * 1024).unwrap(),
        },
        |_| {
            let request = Request::encode_http_submit(
                1,
                &HttpSubmission {
                    operation_id: b"cli-shutdown-held".to_vec(),
                    deadline_ms: 0,
                    endpoint: vec![b'1'; 64],
                    method: "GET".into(),
                    relative_target: "/".into(),
                    headers: vec![],
                    body: vec![],
                    credential: vec![],
                },
            )?;
            Ok(PreparedJob {
                input: request.bytes().to_vec(),
                router: Box::new(HeldRouter {
                    entered: entered_tx,
                    release: release_rx,
                    calls: routed,
                }),
                timeout: WAIT,
            })
        },
    )
    .unwrap();
    entered_rx.recv_timeout(WAIT).unwrap();
    (host, release_tx, calls)
}
struct Input {
    bytes: Cursor<Vec<u8>>,
    fail: bool,
    entered: Option<mpsc::SyncSender<()>>,
}
impl Read for Input {
    fn read(&mut self, out: &mut [u8]) -> stdio::Result<usize> {
        if let Some(entered) = self.entered.take() {
            entered.send(()).unwrap();
        }
        if self.fail {
            Err(stdio::Error::other("controlled input failure"))
        } else {
            self.bytes.read(out)
        }
    }
}
struct Output {
    fail_write: bool,
    fail_flush: bool,
    bytes: Vec<u8>,
}
impl Write for Output {
    fn write(&mut self, bytes: &[u8]) -> stdio::Result<usize> {
        if self.fail_write {
            return Err(stdio::ErrorKind::BrokenPipe.into());
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> stdio::Result<()> {
        if self.fail_flush {
            Err(stdio::ErrorKind::BrokenPipe.into())
        } else {
            Ok(())
        }
    }
}
fn state_frame() -> Vec<u8> {
    let mut message = capnp::message::Builder::new_default();
    let mut request = message.init_root::<wire::request::Builder>();
    request.set_version(1);
    request.set_digest(&protocol::digest());
    request.set_action(wire::Action::PluginState);
    let payload = capnp::serialize::write_message_to_words(&message);
    let mut bytes = (payload.len() as u32).to_le_bytes().to_vec();
    bytes.extend(payload);
    bytes
}
fn assert_drains(
    bytes: Vec<u8>,
    fail_read: bool,
    fail_write: bool,
    fail_flush: bool,
    expected_success: bool,
) {
    let dir = tempfile::tempdir().unwrap();
    let (mut host, release, calls) = start_held(dir.path());
    let (entered_tx, entered_rx) = mpsc::sync_channel(1);
    let returned = Arc::new(AtomicBool::new(false));
    let returned_check = returned.clone();
    let root = dir.path().to_path_buf();
    let observer = thread::spawn(move || {
        entered_rx.recv_timeout(WAIT).unwrap();
        thread::sleep(Duration::from_millis(100));
        assert!(
            !returned_check.load(Ordering::SeqCst),
            "stream close returned before its worker exited"
        );
        assert!(matches!(
            morrow_audit::library::Registry::open(&root),
            Err(morrow_audit::library::Error::Busy)
        ));
        assert!(matches!(
            morrow_audit::session::Session::open(
                &root.join("workbench.db"),
                Default::default(),
                morrow_audit::session::OpenMode::Existing
            ),
            Err(morrow_audit::session::SessionError::Busy)
        ));
        release.send(()).unwrap();
    });
    let mut input = Input {
        bytes: Cursor::new(bytes),
        fail: fail_read,
        entered: Some(entered_tx),
    };
    let mut output = Output {
        fail_write,
        fail_flush,
        bytes: vec![],
    };
    let result = serve(&mut host, &mut input, &mut output);
    returned.store(true, Ordering::SeqCst);
    observer.join().unwrap();
    assert_eq!(result.is_ok(), expected_success, "{result:?}");
    let status = host.io_status();
    assert_eq!(status.storage, StoragePhase::Reclaimed);
    let exit = status.exit.expect("an actual joined worker exit");
    assert!(exit.disconnect.is_ok() && exit.maintenance.is_ok());
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "shutdown must not replay the operation"
    );
    drop(host);
    let mut reopened = Workbench::open_managed(dir.path(), None).unwrap();
    reopened.finish().unwrap();
}
#[test]
fn eof_and_explicit_terminator_wait_for_actual_worker_return() {
    assert_drains(vec![], false, false, false, true);
    assert_drains(vec![0; 4], false, false, false, true);
}
#[test]
fn truncated_headers_bodies_and_input_errors_still_drain_original_owner() {
    assert_drains(vec![1, 2], false, false, false, false);
    assert_drains(vec![8, 0, 0, 0, 1, 2], false, false, false, false);
    assert_drains(vec![], true, false, false, false);
    assert_drains(
        (128u32 * 1024 + 1).to_le_bytes().to_vec(),
        false,
        false,
        false,
        false,
    );
}
#[test]
fn broken_output_write_and_flush_still_drain_original_owner() {
    assert_drains(state_frame(), false, true, false, false);
    assert_drains(state_frame(), false, false, true, false);
}
