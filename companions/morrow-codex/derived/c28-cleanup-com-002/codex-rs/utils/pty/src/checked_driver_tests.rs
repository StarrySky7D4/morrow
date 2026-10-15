use super::*;
use std::sync::Arc;

fn unverified_driver() -> ProcessDriver {
    let (writer_tx, _writer_rx) = mpsc::channel(1);
    let (_stdout_tx, stdout_rx) = broadcast::channel(1);
    let (_exit_tx, exit_rx) = oneshot::channel();
    ProcessDriver {
        writer_tx,
        stdout_rx,
        stderr_rx: None,
        exit_rx,
        terminator: None,
        writer_handle: None,
        // Legacy callbacks cannot establish checked OS acknowledgement.
        resizer: Some(Box::new(|_| Ok(()))),
        tty: true,
    }
}
struct UnsupportedControls;
impl CheckedDriverControls for UnsupportedControls {
    fn capabilities(&self) -> CheckedControlCapabilities {
        CheckedControlCapabilities::default()
    }
    fn close_stdin(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = io::Result<()>> + Send + '_>> {
        Box::pin(async {
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "no actual driver",
            ))
        })
    }
    fn resize(
        &self,
        _: TerminalSize,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = io::Result<()>> + Send + '_>> {
        Box::pin(async {
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "no actual driver",
            ))
        })
    }
}
#[tokio::test]
async fn unverified_resizer_keeps_checked_capabilities_false() {
    let spawned = spawn_from_driver(unverified_driver());
    assert_eq!(
        spawned.session.checked_control_capabilities(),
        CheckedControlCapabilities::default()
    );
    assert_eq!(
        spawned
            .session
            .resize_checked(TerminalSize::default())
            .await
            .unwrap_err()
            .kind(),
        io::ErrorKind::Unsupported
    );
}
#[tokio::test]
async fn explicit_unsupported_adapter_never_promotes_capabilities() {
    let spawned =
        spawn_from_driver_with_checked_controls(unverified_driver(), Arc::new(UnsupportedControls));
    assert_eq!(
        spawned.session.checked_control_capabilities(),
        CheckedControlCapabilities::default()
    );
    assert_eq!(
        spawned
            .session
            .close_stdin_checked()
            .await
            .unwrap_err()
            .kind(),
        io::ErrorKind::Unsupported
    );
}

struct StickyUnknownControls;
impl CheckedDriverControls for StickyUnknownControls {
    fn capabilities(&self) -> CheckedControlCapabilities {
        CheckedControlCapabilities::default()
    }
    fn close_stdin(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = io::Result<()>> + Send + '_>> {
        Box::pin(async { Ok(()) })
    }
    fn resize(
        &self,
        _: TerminalSize,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = io::Result<()>> + Send + '_>> {
        Box::pin(async { Ok(()) })
    }
    fn wait_ready_to_finalize(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
        Box::pin(async {})
    }
    fn finalize_checked_owner(
        &self,
        _: std::time::Instant,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = io::Result<CheckedOwnerFinalization>> + Send + '_,
        >,
    > {
        Box::pin(async {
            Ok(CheckedOwnerFinalization::PhysicallyReapedWithStickyUnknown {
                exit_code: Some(0),
            })
        })
    }
}

#[tokio::test]
async fn sticky_unknown_finalization_never_promotes_to_clean_exit() {
    let (writer_tx, _writer_rx) = mpsc::channel(1);
    let (_stdout_tx, stdout_rx) = broadcast::channel(1);
    let (exit_tx, exit_rx) = oneshot::channel();
    let _ = exit_tx.send(0);
    let driver = ProcessDriver {
        writer_tx,
        stdout_rx,
        stderr_rx: None,
        exit_rx,
        terminator: None,
        writer_handle: None,
        resizer: None,
        tty: false,
    };
    let spawned =
        spawn_from_driver_with_checked_controls(driver, Arc::new(StickyUnknownControls));
    assert_eq!(
        spawned
            .session
            .finalize_checked_owner(
                std::time::Instant::now() + std::time::Duration::from_secs(1)
            )
            .await
            .unwrap(),
        CheckedOwnerFinalization::PhysicallyReapedWithStickyUnknown {
            exit_code: Some(0),
        }
    );
    assert!(spawned.exit_rx.await.is_err());
    assert!(!spawned.session.has_exited());
    assert_eq!(spawned.session.exit_code(), None);
}

struct ContendedSharedFinalizerControls {
    calls: std::sync::atomic::AtomicUsize,
    first_deadline: std::sync::Mutex<Option<std::time::Instant>>,
    gate: tokio::sync::Notify,
    finished: std::sync::atomic::AtomicBool,
}

impl CheckedDriverControls for ContendedSharedFinalizerControls {
    fn capabilities(&self) -> CheckedControlCapabilities {
        CheckedControlCapabilities::default()
    }
    fn close_stdin(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = io::Result<()>> + Send + '_>> {
        Box::pin(async { Ok(()) })
    }
    fn resize(
        &self,
        _: TerminalSize,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = io::Result<()>> + Send + '_>> {
        Box::pin(async { Ok(()) })
    }
    fn wait_ready_to_finalize(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
        Box::pin(async {})
    }
    fn finalize_checked_owner(
        &self,
        deadline: std::time::Instant,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = io::Result<CheckedOwnerFinalization>> + Send + '_,
        >,
    > {
        Box::pin(async move {
            let call = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if call == 0 {
                // Simulate manual finalizer holding the finalization claim until notified.
                self.gate.notified().await;
                self.finished
                    .store(true, std::sync::atomic::Ordering::SeqCst);
                return Ok(CheckedOwnerFinalization::CleanRelease { exit_code: 17 });
            }
            {
                let mut seen = self.first_deadline.lock().unwrap();
                if let Some(prev) = *seen {
                    // Automatic ProcessDriver completion must preserve its single original finite deadline.
                    assert_eq!(prev, deadline);
                } else {
                    *seen = Some(deadline);
                }
            }
            if !self.finished.load(std::sync::atomic::Ordering::SeqCst) {
                return Err(io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "temporary finalizer contention",
                ));
            }
            Ok(CheckedOwnerFinalization::CleanRelease { exit_code: 17 })
        })
    }
}

#[tokio::test]
async fn automatic_completion_shares_finalization_outcome_with_concurrent_manual_finalizer() {
    let (writer_tx, _writer_rx) = mpsc::channel(1);
    let (_stdout_tx, stdout_rx) = broadcast::channel(1);
    let (exit_tx, exit_rx) = oneshot::channel();
    let _ = exit_tx.send(17);
    let driver = ProcessDriver {
        writer_tx,
        stdout_rx,
        stderr_rx: None,
        exit_rx,
        terminator: None,
        writer_handle: None,
        resizer: None,
        tty: false,
    };
    let controls = Arc::new(ContendedSharedFinalizerControls {
        calls: std::sync::atomic::AtomicUsize::new(0),
        first_deadline: std::sync::Mutex::new(None),
        gate: tokio::sync::Notify::new(),
        finished: std::sync::atomic::AtomicBool::new(false),
    });
    let spawned = spawn_from_driver_with_checked_controls(driver, controls.clone());

    let manual_deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    let manual_fut = spawned.session.finalize_checked_owner(manual_deadline);
    tokio::pin!(manual_fut);

    assert!(tokio::time::timeout(std::time::Duration::from_millis(20), &mut manual_fut)
        .await
        .is_err());

    controls.gate.notify_waiters();
    let manual_outcome = manual_fut.await.unwrap();
    assert_eq!(
        manual_outcome,
        CheckedOwnerFinalization::CleanRelease { exit_code: 17 }
    );

    let auto_code = tokio::time::timeout(std::time::Duration::from_secs(1), spawned.exit_rx)
        .await
        .expect("automatic completion should finish within deadline")
        .expect("automatic completion should publish exit code");
    assert_eq!(auto_code, 17);
    assert!(spawned.session.has_exited());
    assert_eq!(spawned.session.exit_code(), Some(17));
}

