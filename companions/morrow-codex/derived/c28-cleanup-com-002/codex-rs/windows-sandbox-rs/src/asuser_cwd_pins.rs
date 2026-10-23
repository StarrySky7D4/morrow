//! Original startup-directory pins. No Drop closes or retry after uncertainty.
use std::{
    fs::File,
    io,
    mem::ManuallyDrop,
    os::windows::{fs::MetadataExt, io::FromRawHandle},
    path::PathBuf,
    ptr,
    time::Instant,
};
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::CreateFileW;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Empty,
    Opening,
    Held,
    Rejected,
    Closing,
    Closed,
    Unknown,
    CloseFailed(i32),
}
#[derive(Clone, Copy)]
struct Pin {
    raw: HANDLE,
    state: State,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Empty,
    Opening,
    Ready,
    Refused,
    Closing,
    Closed,
    Unknown,
}

pub(crate) struct CwdPins {
    pins: [Pin; 32],
    count: usize,
    phase: Phase,
}
fn rejected() -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        "A1 original cwd pins rejected",
    )
}

impl CwdPins {
    pub(crate) fn new() -> Self {
        Self {
            pins: [Pin {
                raw: 0,
                state: State::Empty,
            }; 32],
            count: 0,
            phase: Phase::Empty,
        }
    }
    fn deadline(&mut self, deadline: Instant) -> io::Result<()> {
        if Instant::now() >= deadline {
            self.phase = Phase::Unknown;
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "A1 original cwd pin budget expired",
            ));
        }
        Ok(())
    }
    /// Installed in Attempt.command before the first native allocation. Each
    /// result is captured in its original slot before metadata or other fallible work.
    pub(crate) fn open_once(&mut self, paths: &[PathBuf], deadline: Instant) -> io::Result<()> {
        if self.phase != Phase::Empty || paths.is_empty() || paths.len() > self.pins.len() {
            return Err(rejected());
        }
        self.count = paths.len();
        self.phase = Phase::Opening;
        for (index, path) in paths.iter().enumerate() {
            self.deadline(deadline)?;
            let Some(path) = path.to_str().filter(|v| !v.contains('\0')) else {
                self.phase = Phase::Refused;
                return Err(rejected());
            };
            let wide: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
            self.pins[index].state = State::Opening;
            // GENERIC_READ, FILE_SHARE_READ, OPEN_EXISTING,
            // BACKUP_SEMANTICS | OPEN_REPARSE_POINT. Never follow reparses.
            let raw =
                unsafe { CreateFileW(wide.as_ptr(), 0x80000000, 1, ptr::null(), 3, 0x02200000, 0) };
            let error = if raw == INVALID_HANDLE_VALUE || raw == 0 {
                unsafe { GetLastError() }
            } else {
                0
            };
            self.pins[index].raw = raw;
            if raw == INVALID_HANDLE_VALUE && error != 0 {
                self.pins[index].state = State::Rejected;
                self.phase = Phase::Refused;
                return Err(io::Error::from_raw_os_error(error as i32));
            }
            if raw == 0 || raw == INVALID_HANDLE_VALUE {
                self.pins[index].state = State::Unknown;
                self.phase = Phase::Unknown;
                return Err(rejected());
            }
            self.pins[index].state = State::Held;
            self.deadline(deadline)?;
            // Borrow only: the exact retained slot remains the sole close owner.
            let file = ManuallyDrop::new(unsafe { File::from_raw_handle(raw as _) });
            let metadata = match file.metadata() {
                Ok(value) => value,
                Err(error) => {
                    self.phase = Phase::Refused;
                    return Err(error);
                }
            };
            if !metadata.is_dir() || metadata.file_attributes() & 0x400 != 0 {
                self.phase = Phase::Refused;
                return Err(rejected());
            }
            self.deadline(deadline)?;
        }
        self.phase = Phase::Ready;
        Ok(())
    }
    pub(crate) fn ready(&self) -> bool {
        self.phase == Phase::Ready && self.count > 0
    }
    pub(crate) fn checked_closed(&self) -> bool {
        self.phase == Phase::Closed && self.count > 0 && !self.uncertain()
    }
    pub(crate) fn outstanding(&self) -> bool {
        !matches!(self.phase, Phase::Empty | Phase::Closed)
    }
    pub(crate) fn uncertain(&self) -> bool {
        matches!(self.phase, Phase::Opening | Phase::Closing | Phase::Unknown)
            || self.pins.iter().any(|p| {
                matches!(
                    p.state,
                    State::Opening | State::Closing | State::Unknown | State::CloseFailed(_)
                )
            })
    }
    pub(crate) fn close_once(&mut self, deadline: Instant) -> io::Result<()> {
        self.close_with(deadline, |raw| {
            if unsafe { CloseHandle(raw) } != 0 {
                Ok(())
            } else {
                Err(io::Error::last_os_error())
            }
        })
    }
    fn close_with(
        &mut self,
        deadline: Instant,
        mut close: impl FnMut(HANDLE) -> io::Result<()>,
    ) -> io::Result<()> {
        if !matches!(self.phase, Phase::Ready | Phase::Refused) || self.uncertain() {
            return Err(rejected());
        }
        self.phase = Phase::Closing;
        for index in (0..self.count).rev() {
            self.deadline(deadline)?;
            if matches!(self.pins[index].state, State::Empty | State::Rejected) {
                continue;
            }
            if self.pins[index].state != State::Held {
                self.phase = Phase::Unknown;
                return Err(rejected());
            }
            self.pins[index].state = State::Closing;
            if let Err(error) = close(self.pins[index].raw) {
                self.pins[index].state = State::CloseFailed(error.raw_os_error().unwrap_or(0));
                self.phase = Phase::Unknown;
                return Err(error);
            }
            self.pins[index].raw = 0;
            self.pins[index].state = State::Closed;
            self.deadline(deadline)?;
        }
        self.phase = Phase::Closed;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    fn budget() -> Instant {
        Instant::now() + Duration::from_secs(10)
    }
    fn model() -> CwdPins {
        let mut value = CwdPins::new();
        value.count = 2;
        value.phase = Phase::Ready;
        value.pins[0] = Pin {
            raw: 101,
            state: State::Held,
        };
        value.pins[1] = Pin {
            raw: 103,
            state: State::Held,
        };
        value
    }
    #[test]
    fn close_failure_preserves_exact_originals_and_never_retries() {
        let mut value = model();
        let mut calls = Vec::new();
        let result = value.close_with(budget(), |raw| {
            calls.push(raw);
            Err(io::Error::from_raw_os_error(6))
        });
        assert_eq!(result.unwrap_err().raw_os_error(), Some(6));
        assert!(value.uncertain() && value.outstanding());
        assert_eq!(value.pins[1].raw, 103);
        assert_eq!(value.pins[1].state, State::CloseFailed(6));
        assert_eq!(value.pins[0].state, State::Held);
        assert!(
            value
                .close_with(budget(), |raw| {
                    calls.push(raw);
                    Ok(())
                })
                .is_err()
        );
        assert_eq!(calls, vec![103]);
    }
    #[test]
    fn late_close_never_becomes_completed_or_releases_remaining_pins() {
        let mut value = model();
        let mut calls = Vec::new();
        assert!(
            value
                .close_with(Instant::now(), |raw| {
                    calls.push(raw);
                    Ok(())
                })
                .is_err()
        );
        assert!(calls.is_empty());
        assert!(value.uncertain());
        assert!(value.close_with(budget(), |_| Ok(())).is_err());
        assert_eq!(value.pins[0].raw, 101);
        assert_eq!(value.pins[1].raw, 103);
    }
    #[test]
    fn successful_checked_close_consumes_each_original_once_in_reverse_order() {
        let mut value = model();
        let mut calls = Vec::new();
        value
            .close_with(budget(), |raw| {
                calls.push(raw);
                Ok(())
            })
            .unwrap();
        assert_eq!(calls, vec![103, 101]);
        assert!(!value.outstanding());
        assert!(!value.ready());
        assert!(
            value
                .close_with(budget(), |_| panic!("close replay"))
                .is_err()
        );
        assert!(value.open_once(&[PathBuf::from("D:\\")], budget()).is_err());
    }
    #[test]
    fn unwind_keeps_closing_slot_and_cannot_be_retried() {
        let mut value = model();
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _ = value.close_with(budget(), |_| panic!("injected unwind"));
            }))
            .is_err()
        );
        assert_eq!(value.pins[1].state, State::Closing);
        assert_eq!(value.pins[1].raw, 103);
        assert!(value.uncertain() && value.outstanding());
        assert!(
            value
                .close_with(budget(), |_| panic!("close replay"))
                .is_err()
        );
    }
    fn fixture() -> tempfile::TempDir {
        let root = std::env::temp_dir();
        assert!(
            root.to_string_lossy()
                .to_ascii_lowercase()
                .starts_with("d:\\build_cache\\")
        );
        tempfile::Builder::new()
            .prefix("asuser-cwd-pins-")
            .tempdir_in(root)
            .unwrap()
    }
    #[test]
    fn actual_directory_remains_pinned_until_checked_close() {
        let temp = fixture();
        let path = temp.path().join("original");
        let moved = temp.path().join("moved");
        std::fs::create_dir(&path).unwrap();
        let mut value = CwdPins::new();
        value.open_once(&[path.clone()], budget()).unwrap();
        assert!(value.ready() && value.outstanding());
        assert!(std::fs::rename(&path, &moved).is_err());
        value.close_once(budget()).unwrap();
        assert!(!value.outstanding());
        std::fs::rename(&path, &moved).unwrap();
        assert!(value.close_once(budget()).is_err());
    }
    #[test]
    fn actual_failed_second_open_retains_first_pin_until_explicit_close() {
        let temp = fixture();
        let path = temp.path().join("original");
        let moved = temp.path().join("moved");
        std::fs::create_dir(&path).unwrap();
        let mut value = CwdPins::new();
        assert!(
            value
                .open_once(&[path.clone(), path.join("absent")], budget())
                .is_err()
        );
        assert_eq!(value.phase, Phase::Refused);
        assert!(value.outstanding() && !value.uncertain());
        assert_eq!(value.pins[0].state, State::Held);
        assert_eq!(value.pins[1].state, State::Rejected);
        assert!(std::fs::rename(&path, &moved).is_err());
        assert!(value.open_once(&[path.clone()], budget()).is_err());
        value.close_once(budget()).unwrap();
        std::fs::rename(&path, &moved).unwrap();
    }
}
