//! Pure discovery scheduling regression. No OS command or PTY is created.
use std::sync::Arc;
use std::sync::mpsc as thread_channel;
use std::time::Duration;
use std::time::Instant;

use tokio::sync::Mutex;
use tokio::sync::mpsc;
use tokio::sync::oneshot;

use super::CheckedControlCapabilities;
use super::ProcessDriver;
use super::ProcessHandle;
use super::spawn_from_driver;

fn generic_handle() -> Arc<ProcessHandle> {
    let (writer_tx, _input) = mpsc::channel(1);
    let (_, stdout_rx) = tokio::sync::broadcast::channel(1);
    let (_, exit_rx) = oneshot::channel();
    Arc::new(
        spawn_from_driver(ProcessDriver {
            writer_tx,
            stdout_rx,
            stderr_rx: None,
            exit_rx,
            terminator: None,
            writer_handle: None,
            resizer: None,
            tty: false,
        })
        .session,
    )
}

// A finite independent watchdog releases the lock even under the old blocking
// implementation. That implementation fails the latency assertion, not a hang.
fn hold_native_mutex(
    handle: Arc<ProcessHandle>,
) -> (thread_channel::Sender<()>, std::thread::JoinHandle<()>) {
    let (locked_tx, locked_rx) = thread_channel::channel();
    let (release_tx, release_rx) = thread_channel::channel();
    let thread = std::thread::spawn(move || {
        let _guard = handle._pty_handles.lock().unwrap();
        locked_tx.send(()).unwrap();
        let _ = release_rx.recv_timeout(Duration::from_secs(2));
    });
    locked_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    (release_tx, thread)
}

#[tokio::test(flavor = "current_thread")]
async fn discovery_does_not_wait_on_a_busy_native_resize_mutex() {
    let handle = generic_handle();
    let (release, thread) = hold_native_mutex(handle.clone());
    let started = Instant::now();
    let capabilities = handle.checked_control_capabilities();
    assert!(
        started.elapsed() < Duration::from_millis(200),
        "read-only discovery waited for a native operation"
    );
    assert_eq!(capabilities, CheckedControlCapabilities::default());
    release.send(()).unwrap();
    thread.join().unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn busy_discovery_releases_an_async_map_gate_and_keeps_the_worker_live() {
    let handle = generic_handle();
    let (release, thread) = hold_native_mutex(handle.clone());
    // This gate models the caller's process-map guard; it does not construct a
    // LocalProcess or claim that an OS/process-map integration test has run.
    let map = Arc::new(Mutex::new(()));
    let guard = map.lock().await;
    let waiting_map = map.clone();
    let (progress_tx, progress_rx) = oneshot::channel();
    let waiter = tokio::spawn(async move {
        let _guard = waiting_map.lock().await;
        tokio::time::sleep(Duration::from_millis(10)).await;
        progress_tx.send(()).unwrap();
    });
    tokio::task::yield_now().await;
    let started = Instant::now();
    for _ in 0..16 {
        assert_eq!(
            handle.checked_control_capabilities(),
            CheckedControlCapabilities::default()
        );
    }
    drop(guard);
    tokio::time::timeout(Duration::from_millis(200), progress_rx)
        .await
        .unwrap()
        .unwrap();
    assert!(
        started.elapsed() < Duration::from_millis(200),
        "native mutex blocked discovery and delayed the async map gate/worker"
    );
    release.send(()).unwrap();
    thread.join().unwrap();
    waiter.await.unwrap();
}
