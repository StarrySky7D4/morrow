//! The result-metric hook must preserve terminal exit codes and pipe-failure behavior.

use super::*;
use crate::ipc_framed::ExitPayload;
use crate::ipc_framed::write_frame;
use pretty_assertions::assert_eq;
use std::io::Seek;

#[test]
fn runner_result_reporting_preserves_exit_and_closed_pipe_results() -> Result<()> {
    for exit_code in [Some(0), Some(23), None] {
        let mut frames = tempfile::tempfile()?;
        if let Some(exit_code) = exit_code {
            write_frame(
                &mut frames,
                &FramedMessage {
                    version: IPC_PROTOCOL_VERSION,
                    message: Message::Exit {
                        payload: ExitPayload {
                            exit_code,
                            timed_out: false,
                        },
                    },
                },
            )?;
        }
        frames.rewind()?;
        let (stdout_tx, _stdout_rx) = broadcast::channel(1);
        let (exit_tx, exit_rx) = oneshot::channel();
        let (_outbound, owner) = start_runner_pipe_writer(tempfile::tempfile()?);
        let reader = start_runner_stdout_reader(frames, owner, stdout_tx, /*stderr_tx*/ None, exit_tx);
        assert_eq!(exit_rx.blocking_recv()?, exit_code.unwrap_or(-1));
        reader.join().expect("original terminal reader joined");
    }
    Ok(())
}

// These tests use actual Windows pipe endpoints and deliberately retain the same
// production SpawnedProcess.session and control clones through terminal completion.
// No sandbox account, VM, ACL, network, or forced descendant termination is used.
use std::io::{Read, Write};
use std::os::windows::io::{FromRawHandle, IntoRawHandle};
use std::time::Duration;
use windows_sys::Win32::Foundation::{GetLastError, INVALID_HANDLE_VALUE, ERROR_PIPE_CONNECTED};
use windows_sys::Win32::System::Pipes::{CreatePipe, CreateNamedPipeW, ConnectNamedPipe,
    PIPE_TYPE_BYTE, PIPE_READMODE_BYTE, PIPE_WAIT};
use windows_sys::Win32::Storage::FileSystem::PIPE_ACCESS_INBOUND;

fn anonymous_pipe() -> Result<(File, File)> {
    let mut read = 0;
    let mut write = 0;
    if unsafe { CreatePipe(&mut read, &mut write, std::ptr::null(), 0) } == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(unsafe { (File::from_raw_handle(read as _), File::from_raw_handle(write as _)) })
}

fn legacy_session_fixture(input: File, output: File) -> (SpawnedProcess, RunnerOutbound) {
    let (outbound, owner) = start_runner_pipe_writer(input);
    let (writer_tx, writer_rx) = mpsc::channel(4);
    let stdin = start_runner_stdin_writer(writer_rx, outbound.clone(), false, true);
    let (stdout_tx, stdout_rx) = broadcast::channel(8);
    let (exit_tx, exit_rx) = oneshot::channel();
    let reader = start_runner_stdout_reader(output, owner, stdout_tx, None, exit_tx);
    let workers = observe_runner_workers(stdin, reader);
    let terminator = outbound.clone();
    let session = finish_driver_spawn(ProcessDriver {
        writer_tx, stdout_rx, stderr_rx: None, exit_rx,
        terminator: Some(Box::new(move || {
            let _ = terminator.send(FramedMessage { version: IPC_PROTOCOL_VERSION,
                message: Message::Terminate { payload: EmptyPayload::default() } });
        })),
        writer_handle: Some(workers), resizer: Some(make_runner_resizer(outbound.clone())), tty: true,
    }, true);
    (session, outbound)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn legacy_exit_retires_real_anonymous_pipe_with_session_and_controls_retained() -> Result<()> {
    let (mut guest_input, host_input) = anonymous_pipe()?;
    let (host_output, mut guest_output) = anonymous_pipe()?;
    let (eof_tx, eof_rx) = oneshot::channel();
    let guest = std::thread::spawn(move || -> Result<i32> {
        let first: FramedMessage = crate::ipc_framed::read_frame(&mut guest_input)?.expect("stdin frame");
        assert!(matches!(first.message, Message::Stdin { .. }));
        write_frame(&mut guest_output, &FramedMessage { version: IPC_PROTOCOL_VERSION,
            message: Message::Output { payload: crate::ipc_framed::OutputPayload {
                stream: OutputStream::Stdout, data_b64: encode_bytes(b"tail before Exit"),
            } } })?;
        write_frame(&mut guest_output, &FramedMessage { version: IPC_PROTOCOL_VERSION,
            message: Message::Exit { payload: ExitPayload { exit_code: 256, timed_out: false } } })?;
        guest_output.flush()?;
        // CloseStdin/Terminate is not EOF. There must be no post-terminal frame.
        assert!(crate::ipc_framed::read_frame(&mut guest_input)?.is_none());
        eof_tx.send(()).unwrap();
        Ok(256)
    });
    let (spawned, retained_outbound) = legacy_session_fixture(host_input, host_output);
    let SpawnedProcess { session, mut stdout_rx, stderr_rx: _stderr, exit_rx } = spawned;
    let retained_session = Arc::new(session);
    let retained_writer = retained_session.writer_sender();
    let mut retained_resizer = make_runner_resizer(retained_outbound.clone());
    retained_writer.send(b"input".to_vec()).await?;
    assert_eq!(tokio::time::timeout(Duration::from_secs(3), exit_rx).await??, 256);
    tokio::time::timeout(Duration::from_secs(3), eof_rx).await??;
    tokio::time::timeout(Duration::from_secs(3), retained_writer.closed()).await?;
    assert!(retained_outbound.send(FramedMessage { version: IPC_PROTOCOL_VERSION,
        message: Message::Terminate { payload: EmptyPayload::default() } }).is_err());
    assert!(retained_resizer(TerminalSize { rows: 50, cols: 100 }).is_err());
    assert!(retained_session.has_exited());
    assert_eq!(retained_session.exit_code(), Some(256));
    assert_eq!(stdout_rx.recv().await.unwrap(), b"tail before Exit");
    assert!(tokio::time::timeout(Duration::from_secs(3), stdout_rx.recv()).await?.is_none());
    assert_eq!(tokio::task::spawn_blocking(move || guest.join().unwrap()).await??, 256);
    // session, stdin sender, terminator and resizer ownership are still retained here.
    assert!(Arc::strong_count(&retained_session) >= 1);
    Ok(())
}

fn drain_actual_eof(reader: &mut impl Read) -> bool {
    let mut bytes = [0u8; 8192];
    loop {
        match reader.read(&mut bytes) {
            Ok(0) => return true,
            Ok(_) => {},
            Err(error) if error.raw_os_error() == Some(windows_sys::Win32::Foundation::ERROR_BROKEN_PIPE as i32) => return true,
            Err(_) => return false,
        }
    }
}

const NATIVE_PEER_PIPE: &str = "MORROW_C28_V6_TEST_PIPE";
const NATIVE_PEER_STATUS: &str = "MORROW_C28_V6_TEST_STATUS";

/// Only a parent test invokes this fixture with explicitly owned local pipe names.
#[test]
#[ignore = "native pipe fixture; invoked by legacy_native_exit_after_real_pipe_eof_with_session_retained"]
fn native_legacy_guest_fixture() {
    let pipe = std::env::var_os(NATIVE_PEER_PIPE).expect("parent-owned test pipe");
    let status: i32 = std::env::var(NATIVE_PEER_STATUS).unwrap().parse().unwrap();
    let mut output = std::fs::OpenOptions::new().write(true).open(pipe).unwrap();
    write_frame(&mut output, &FramedMessage { version: IPC_PROTOCOL_VERSION,
        message: Message::Exit { payload: ExitPayload { exit_code: status, timed_out: false } } }).unwrap();
    output.flush().unwrap();
    let mut input = std::io::stdin().lock();
    // Actual ChildStdin anonymous-pipe EOF gates the actual native process exit.
    loop {
        match crate::ipc_framed::read_frame(&mut input) {
            Ok(None) => break,
            Ok(Some(frame)) => assert!(!matches!(frame.message, Message::Terminate { .. }),
                "retirement must never terminate a preserved process"),
            Err(_) => { assert!(drain_actual_eof(&mut input)); break; },
        }
    }
    drop(input);
    drop(output);
    std::process::exit(status);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn legacy_native_exit_after_real_pipe_eof_with_session_retained() -> Result<()> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::process::{Command, Stdio};
    static NEXT_PIPE: AtomicUsize = AtomicUsize::new(0);
    for raw in [256u32, 0xffff_ff00, 0xc000_0142] {
        let status = raw as i32;
        let name = format!(r"\\.\pipe\morrow-c28-v6-{}-{}", std::process::id(), NEXT_PIPE.fetch_add(1, Ordering::SeqCst));
        let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
        let handle = unsafe { CreateNamedPipeW(wide.as_ptr(), PIPE_ACCESS_INBOUND,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT, 1, 4096, 4096, 0, std::ptr::null()) };
        if handle == INVALID_HANDLE_VALUE { return Err(std::io::Error::last_os_error().into()); }
        let output = unsafe { File::from_raw_handle(handle as _) };
        let fixture = format!("{}::native_legacy_guest_fixture", module_path!().split_once("::").unwrap().1);
        let mut child = Command::new(std::env::current_exe()?)
            .args(["--exact", &fixture, "--ignored", "--nocapture"])
            .env(NATIVE_PEER_PIPE, &name).env(NATIVE_PEER_STATUS, status.to_string())
            .stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).spawn()?;
        let input = unsafe { File::from_raw_handle(child.stdin.take().unwrap().into_raw_handle()) };
        let output = tokio::time::timeout(Duration::from_secs(3), tokio::task::spawn_blocking(move || -> Result<File> {
            let connected = unsafe { ConnectNamedPipe(handle, std::ptr::null_mut()) };
            if connected == 0 && unsafe { GetLastError() } != ERROR_PIPE_CONNECTED {
                return Err(std::io::Error::last_os_error().into());
            }
            Ok(output)
        })).await???;
        let (spawned, retained_outbound) = legacy_session_fixture(input, output);
        let SpawnedProcess { session, stdout_rx: _stdout, stderr_rx: _stderr, exit_rx } = spawned;
        let retained_session = Arc::new(session);
        let retained_writer = retained_session.writer_sender();
        let mut retained_resizer = make_runner_resizer(retained_outbound.clone());
        assert_eq!(tokio::time::timeout(Duration::from_secs(3), exit_rx).await??, status);
        tokio::time::timeout(Duration::from_secs(3), retained_writer.closed()).await?;
        let native = tokio::time::timeout(Duration::from_secs(3),
            tokio::task::spawn_blocking(move || child.wait())).await???;
        assert_eq!(native.code().unwrap() as u32, raw);
        assert_eq!(retained_session.exit_code(), Some(status));
        assert!(retained_session.has_exited());
        assert!(retained_resizer(TerminalSize { rows: 40, cols: 100 }).is_err());
        // Controls are kept alive through the real native wait. Nothing is force-killed.
        assert!(retained_outbound.retired());
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn legacy_terminal_retires_original_writer_blocked_in_real_pipe_io() -> Result<()> {
    use std::os::windows::io::AsRawHandle;
    let (mut guest_input, host_input) = anonymous_pipe()?;
    let (host_output, mut guest_output) = anonymous_pipe()?;
    let (eof_tx, eof_rx) = oneshot::channel();
    let guest = std::thread::spawn(move || -> Result<()> {
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        loop {
            let mut available = 0;
            let ok = unsafe { windows_sys::Win32::System::Pipes::PeekNamedPipe(
                guest_input.as_raw_handle() as _, std::ptr::null_mut(), 0,
                std::ptr::null_mut(), &mut available, std::ptr::null_mut()) };
            assert_ne!(ok, 0);
            if available > 0 { break; }
            assert!(std::time::Instant::now() < deadline, "host never entered original pipe write");
            std::thread::sleep(Duration::from_millis(1));
        }
        write_frame(&mut guest_output, &FramedMessage { version: IPC_PROTOCOL_VERSION,
            message: Message::Exit { payload: ExitPayload { exit_code: 256, timed_out: false } } })?;
        guest_output.flush()?;
        // The partially written frame is irrelevant after terminal. Observe the
        // physical pipe EOF instead of mistaking IO cancellation for completion.
        assert!(drain_actual_eof(&mut guest_input));
        eof_tx.send(()).unwrap();
        Ok(())
    });
    let (spawned, retained_outbound) = legacy_session_fixture(host_input, host_output);
    let SpawnedProcess { session, exit_rx, stdout_rx: _stdout, stderr_rx: _stderr } = spawned;
    let retained_sender = session.writer_sender();
    retained_outbound.send(FramedMessage { version: IPC_PROTOCOL_VERSION,
        message: Message::Stdin { payload: StdinPayload { data_b64: encode_bytes(&vec![b'x'; 256 * 1024]) } } })?;
    assert_eq!(tokio::time::timeout(Duration::from_secs(3), exit_rx).await??, 256);
    tokio::time::timeout(Duration::from_secs(3), eof_rx).await??;
    tokio::time::timeout(Duration::from_secs(3), retained_sender.closed()).await?;
    tokio::task::spawn_blocking(move || guest.join().unwrap()).await??;
    assert!(session.has_exited());
    assert_eq!(session.exit_code(), Some(256));
    Ok(())
}
