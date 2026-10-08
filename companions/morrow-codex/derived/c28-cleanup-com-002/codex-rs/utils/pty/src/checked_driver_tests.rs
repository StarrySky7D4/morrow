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
