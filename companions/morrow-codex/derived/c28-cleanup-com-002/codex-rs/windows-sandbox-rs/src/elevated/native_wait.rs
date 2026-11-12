//! Bounded observations on borrowed original native handles. Cooperative
//! callers yield between zero-duration probes; the owner/loan stays in the
//! caller's future. No blocking task, duplicate handle or detached finalizer.
use std::future::Future;
use std::io;
use std::task::{Context, Poll, Waker};
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{GetLastError, HANDLE, WAIT_FAILED, WAIT_TIMEOUT};
use windows_sys::Win32::System::Threading::WaitForSingleObject;

#[derive(Clone, Copy, Debug)]
pub(super) enum NativeWaitMode { Blocking, Cooperative }

pub(super) struct NativeWaitObservation {
    pub(super) return_value: u32,
    pub(super) os_error: Option<i32>,
}

pub(super) fn observe_original(raw: HANDLE, millis: u32) -> NativeWaitObservation {
    let return_value = unsafe { WaitForSingleObject(raw, millis) };
    // GetLastError belongs to this exact wait, before any await or other call.
    let os_error = (return_value == WAIT_FAILED).then(|| unsafe { GetLastError() } as i32);
    NativeWaitObservation { return_value, os_error }
}

pub(super) async fn wait_with(
    mode: NativeWaitMode,
    deadline: Instant,
    mut observe: impl FnMut(u32) -> NativeWaitObservation,
) -> NativeWaitObservation {
    if matches!(mode, NativeWaitMode::Blocking) {
        return observe(super::owned_connect_worker::finite_wait_ms(
            deadline.saturating_duration_since(Instant::now()),
        ));
    }
    loop {
        let observed = observe(0);
        // A zero-duration readiness timeout is not an expired operation. Actual
        // failure, unexpected wait, signal, or the unchanged deadline is final.
        if observed.return_value != WAIT_TIMEOUT || Instant::now() >= deadline {
            return observed;
        }
        tokio::time::sleep(deadline.saturating_duration_since(Instant::now())
            .min(Duration::from_millis(1))).await;
    }
}

// The Blocking branch of shared finalization contains no suspension point.
// Poll it once without creating a runtime or transferring resource ownership.
pub(super) fn run_blocking<T>(future: impl Future<Output = io::Result<T>>) -> io::Result<T> {
    let mut future = std::pin::pin!(future);
    match future.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(result) => result,
        Poll::Pending => Err(io::Error::other("blocking native finalizer unexpectedly suspended")),
    }
}
