//! Pure adapter tests: these do not qualify an OS process or a sandbox.
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use pretty_assertions::assert_eq;
use tokio::sync::watch;

use super::ExecProcess;
use super::ExecProcessEventReceiver;
use super::ExecProcessFuture;
use super::ProcessControlCapabilities;
use super::ProcessControlOutcome;
use crate::ProcessId;
use crate::protocol::ProcessSignal;
use crate::protocol::ReadResponse;
use crate::protocol::WriteResponse;
use crate::protocol::WriteStatus;

struct ControlledMock {
    id: ProcessId,
    effects: AtomicUsize,
}

impl ExecProcess for ControlledMock {
    fn process_id(&self) -> &ProcessId {
        &self.id
    }
    fn subscribe_wake(&self) -> watch::Receiver<u64> {
        watch::channel(0).1
    }
    fn subscribe_events(&self) -> ExecProcessEventReceiver {
        ExecProcessEventReceiver::empty()
    }
    fn read(
        &self,
        _: Option<u64>,
        _: Option<usize>,
        _: Option<u64>,
    ) -> ExecProcessFuture<'_, ReadResponse> {
        Box::pin(async { panic!("read is not used by the control test") })
    }
    fn write(&self, _: Vec<u8>) -> ExecProcessFuture<'_, WriteResponse> {
        Box::pin(async {
            Ok(WriteResponse {
                status: WriteStatus::Accepted,
            })
        })
    }
    fn signal(&self, _: ProcessSignal) -> ExecProcessFuture<'_, ()> {
        Box::pin(async { Ok(()) })
    }
    fn terminate(&self) -> ExecProcessFuture<'_, ()> {
        Box::pin(async { Ok(()) })
    }
    fn checked_control_capabilities(&self) -> ExecProcessFuture<'_, ProcessControlCapabilities> {
        Box::pin(async {
            Ok(ProcessControlCapabilities {
                close_input: true,
                resize_pty: true,
            })
        })
    }
    fn close_input_checked(&self) -> ExecProcessFuture<'_, ProcessControlOutcome> {
        Box::pin(async {
            self.effects.fetch_add(1, Ordering::SeqCst);
            Ok(ProcessControlOutcome::Applied)
        })
    }
    fn resize_checked(&self, _: u16, _: u16) -> ExecProcessFuture<'_, ProcessControlOutcome> {
        Box::pin(async {
            self.effects.fetch_add(1, Ordering::SeqCst);
            Ok(ProcessControlOutcome::Applied)
        })
    }
}

// An existing adapter delegates the pre-existing methods only. It does not gain
// new control authority simply because its inner process supports the controls.
struct ExistingDecorator(Arc<dyn ExecProcess>);
impl ExecProcess for ExistingDecorator {
    fn process_id(&self) -> &ProcessId {
        self.0.process_id()
    }
    fn subscribe_wake(&self) -> watch::Receiver<u64> {
        self.0.subscribe_wake()
    }
    fn subscribe_events(&self) -> ExecProcessEventReceiver {
        self.0.subscribe_events()
    }
    fn read(
        &self,
        seq: Option<u64>,
        bytes: Option<usize>,
        wait: Option<u64>,
    ) -> ExecProcessFuture<'_, ReadResponse> {
        self.0.read(seq, bytes, wait)
    }
    fn write(&self, bytes: Vec<u8>) -> ExecProcessFuture<'_, WriteResponse> {
        self.0.write(bytes)
    }
    fn signal(&self, signal: ProcessSignal) -> ExecProcessFuture<'_, ()> {
        self.0.signal(signal)
    }
    fn terminate(&self) -> ExecProcessFuture<'_, ()> {
        self.0.terminate()
    }
}

#[tokio::test]
async fn existing_decorator_does_not_implicitly_gain_checked_controls() {
    let inner = Arc::new(ControlledMock {
        id: ProcessId::from("mock"),
        effects: AtomicUsize::new(0),
    });
    let process: Arc<dyn ExecProcess> = inner.clone();
    let decorated = ExistingDecorator(process);
    assert_eq!(
        decorated.checked_control_capabilities().await.unwrap(),
        ProcessControlCapabilities::default()
    );
    assert_eq!(
        decorated.close_input_checked().await.unwrap(),
        ProcessControlOutcome::Unsupported
    );
    assert_eq!(
        decorated.resize_checked(24, 80).await.unwrap(),
        ProcessControlOutcome::Unsupported
    );
    assert_eq!(inner.effects.load(Ordering::SeqCst), 0);
    assert_eq!(
        decorated.write(b"original write".to_vec()).await.unwrap(),
        WriteResponse {
            status: WriteStatus::Accepted
        }
    );
}

#[tokio::test]
async fn explicit_process_controls_are_object_safe_and_keep_the_same_process() {
    let inner = Arc::new(ControlledMock {
        id: ProcessId::from("mock"),
        effects: AtomicUsize::new(0),
    });
    let process: Arc<dyn ExecProcess> = inner.clone();
    assert_eq!(process.process_id(), &ProcessId::from("mock"));
    assert_eq!(
        process.checked_control_capabilities().await.unwrap(),
        ProcessControlCapabilities {
            close_input: true,
            resize_pty: true
        }
    );
    assert_eq!(
        process.close_input_checked().await.unwrap(),
        ProcessControlOutcome::Applied
    );
    assert_eq!(
        process.resize_checked(24, 80).await.unwrap(),
        ProcessControlOutcome::Applied
    );
    assert_eq!(inner.effects.load(Ordering::SeqCst), 2);
}
