//! Confirmation narrows the original nomination. It never supplies a new path
//! to capture, a new IO context, another window, or a transferable native owner.
use super::{DirectoryWatch, Error, WatchClock};
use crate::{
    directory_io::{
        DirectoryAbsolutePath,
        native_picker::{self, Event},
    },
    manager::{ManagedInstance, Manager},
};
use morrow_core::dispatch::HostRuntime;
use std::{
    ffi::OsString,
    os::windows::ffi::OsStringExt,
    panic::{AssertUnwindSafe, catch_unwind},
    path::Path,
    sync::Arc,
};

#[derive(Default)]
struct Confirmation {
    attempted: bool,
    captured: bool,
    failure: Option<Error>,
}
impl Confirmation {
    #[allow(clippy::too_many_arguments)]
    fn event(
        &mut self,
        owner: &mut DirectoryWatch,
        m: &Manager,
        h: &HostRuntime,
        i: &ManagedInstance,
        c: &mut impl WatchClock,
        stop: &mut impl FnMut() -> bool,
        event: Event<'_>,
    ) -> Result<(), crate::directory_io::Error> {
        // Native callbacks after a first refusal cannot sample the clock,
        // replace the nomination, reopen an object, or make another charge.
        if self.failure.is_some() {
            return Err(crate::directory_io::Error::NativePickerUnavailable);
        }
        let result = catch_unwind(AssertUnwindSafe(|| {
            let s = owner.state.as_ref().unwrap();
            s.authenticate(m, h, i)?;
            s.usable()?;
            if stop() {
                return Err(Error::Directory(crate::directory_io::Error::Cancelled));
            }
            s.check(m, h, i, c, 0)?;
            if let Event::Select(raw) = event {
                if self.attempted {
                    return Err(Error::Correlation);
                }
                self.attempted = true;
                let spelling = OsString::from_wide(raw);
                let selected = DirectoryAbsolutePath::new(Path::new(&spelling))
                    .map_err(|_| Error::SelectionMismatch)?;
                let (drive, selected) = selected.into_parts();
                let (approved_drive, approved) = s.approval.nominated_path().clone().into_parts();
                // A drive letter has an explicit case-insensitive Windows
                // identity. Every remaining UTF-16 unit must match exactly;
                // no case folding, separator conversion or normalization.
                if !drive.eq_ignore_ascii_case(&approved_drive)
                    || selected.as_ref().map(|p| p.as_components())
                        != approved.as_ref().map(|p| p.as_components())
                {
                    return Err(Error::SelectionMismatch);
                }
                // This uses only the owned nomination already in approval.
                // All ancestors and the original async leaf are acquired
                // before the synchronous OnFileOk callback can accept.
                owner.capture(m, h, i, c)?;
                self.captured = true;
            }
            if stop() {
                return Err(Error::Directory(crate::directory_io::Error::Cancelled));
            }
            owner.state.as_ref().unwrap().check(m, h, i, c, 0)
        }));
        let result = result.unwrap_or_else(|_| {
            // The FFI bridge catches panics too, but an interrupted original
            // capture must additionally retain this whole native owner.
            owner.state.as_mut().unwrap().unknown = true;
            Err(Error::Unknown)
        });
        if let Err(error) = result {
            self.failure = Some(error);
            owner.state.as_mut().unwrap().fail(error);
            Err(crate::directory_io::Error::NativePickerUnavailable)
        } else {
            Ok(())
        }
    }
}

pub(super) fn run(
    owner: &mut DirectoryWatch,
    m: &Manager,
    h: &HostRuntime,
    i: &ManagedInstance,
    c: &mut impl WatchClock,
    mut stop: impl FnMut() -> bool,
) -> Result<(), Error> {
    let ledger = Arc::clone(&owner.state.as_ref().unwrap().ledger);
    let nomination = owner
        .state
        .as_ref()
        .unwrap()
        .approval
        .nominated_path()
        .picker_spelling();
    let mut confirmation = Confirmation::default();
    let result = native_picker::show_nominated(&ledger, &nomination, |event| {
        confirmation.event(owner, m, h, i, c, &mut stop, event)
    });
    if ledger.snapshot().unknown != 0 {
        owner.state.as_mut().unwrap().unknown = true;
        return Err(Error::Unknown);
    }
    if let Some(error) = confirmation.failure {
        return Err(error);
    }
    result.map_err(Error::Directory)?;
    if !confirmation.captured {
        return Err(Error::Directory(
            crate::directory_io::Error::NativePickerUnavailable,
        ));
    }
    owner.state.as_ref().unwrap().check(m, h, i, c, 0)
}

#[cfg(test)]
#[path = "picker_tests.rs"]
mod tests;
