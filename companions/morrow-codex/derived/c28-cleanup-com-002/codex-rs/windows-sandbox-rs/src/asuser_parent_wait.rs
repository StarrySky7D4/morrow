//! Wait once on an original borrowed child handle, without extending its deadline.
//! No handle ownership, duplicate, reopen, termination, close, or retry is added.
use std::{io, os::windows::io::{AsRawHandle, BorrowedHandle}, time::Instant};

#[link(name = "kernel32")]
unsafe extern "system" {
    fn WaitForSingleObject(handle: *mut std::ffi::c_void, milliseconds: u32) -> u32;
}

fn expired() -> io::Error {
    io::Error::new(io::ErrorKind::TimedOut, "A1 original child native lifetime budget")
}

fn wait_once(
    deadline: Instant,
    before: Instant,
    native: impl FnOnce(u32) -> io::Result<bool>,
    after: impl FnOnce() -> Instant,
) -> io::Result<()> {
    let remaining = deadline.checked_duration_since(before)
        .filter(|duration| !duration.is_zero()).ok_or_else(expired)?;
    // Never round up past the original budget or pass INFINITE to Windows.
    let milliseconds = remaining.as_millis().min(u128::from(u32::MAX - 1)) as u32;
    // Preserve an actual native error even if the clock subsequently expires.
    if !native(milliseconds)? || after() >= deadline { return Err(expired()); }
    Ok(())
}

pub(super) fn wait_original(handle: BorrowedHandle<'_>, deadline: Instant) -> io::Result<()> {
    wait_once(deadline, Instant::now(), |milliseconds| {
        // The original owner retains the handle for this entire borrow.
        match unsafe { WaitForSingleObject(handle.as_raw_handle(), milliseconds) } {
            0 => Ok(true),
            258 => Ok(false),
            u32::MAX => Err(io::Error::last_os_error()),
            _ => Err(io::Error::new(io::ErrorKind::InvalidData,
                "unexpected original child wait result")),
        }
    }, Instant::now)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, time::Duration};

    #[test]
    fn expired_budget_never_calls_native_or_starts_another_wait() {
        let now = Instant::now();
        for before in [now, now + Duration::from_millis(1)] {
            let error = wait_once(now, before, |_| panic!("late native call"),
                || panic!("late clock query")).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        }
    }

    #[test]
    fn process_and_thread_waits_share_one_absolute_budget() {
        let start = Instant::now();
        let deadline = start + Duration::from_millis(100);
        let calls = Cell::new(0);
        wait_once(deadline, start, |ms| {
            calls.set(calls.get() + 1); assert_eq!(ms, 100); Ok(true)
        }, || start + Duration::from_millis(71)).unwrap();
        wait_once(deadline, start + Duration::from_millis(71), |ms| {
            calls.set(calls.get() + 1); assert_eq!(ms, 29); Ok(true)
        }, || start + Duration::from_millis(92)).unwrap();
        assert_eq!(calls.get(), 2);
    }

    #[test]
    fn a_pending_handle_cannot_be_promoted_to_signal_or_retried() {
        let start = Instant::now(); let calls = Cell::new(0);
        let error = wait_once(start + Duration::from_secs(1), start, |_| {
            calls.set(calls.get() + 1); Ok(false)
        }, || panic!("no new observation after timeout")).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn native_failure_preserves_its_original_code_without_a_second_call() {
        let start = Instant::now(); let calls = Cell::new(0);
        let error = wait_once(start + Duration::from_secs(1), start, |_| {
            calls.set(calls.get() + 1); Err(io::Error::from_raw_os_error(6))
        }, || panic!("cannot replace native error with later clock evidence")).unwrap_err();
        assert_eq!(error.raw_os_error(), Some(6)); assert_eq!(calls.get(), 1);
    }

    #[test]
    fn signal_at_or_after_deadline_does_not_authorize_retirement() {
        let start = Instant::now(); let deadline = start + Duration::from_millis(10);
        for after in [deadline, deadline + Duration::from_millis(1)] {
            assert_eq!(wait_once(deadline, start, |_| Ok(true), || after)
                .unwrap_err().kind(), io::ErrorKind::TimedOut);
        }
    }

    #[test]
    fn fractional_budget_is_not_rounded_up() {
        let start = Instant::now();
        wait_once(start + Duration::from_micros(900), start, |ms| {
            assert_eq!(ms, 0); Ok(true)
        }, || start + Duration::from_micros(1)).unwrap();
    }

    #[test]
    fn large_budget_never_becomes_infinite_or_wraps() {
        let start = Instant::now();
        let deadline = start + Duration::from_millis(u64::from(u32::MAX) + 100);
        wait_once(deadline, start, |ms| {
            assert_eq!(ms, u32::MAX - 1); Ok(true)
        }, || start).unwrap();
    }
}
