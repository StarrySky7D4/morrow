//! Private original-worker FFI. Native pointers and callback state never become
//! a guest capability, public wire field, replay key, or a second owner.
use super::{Error, retirement::Ledger};
use std::{
    cell::{Cell, UnsafeCell},
    ffi::c_void,
    panic::{AssertUnwindSafe, catch_unwind},
};

// Our controlled UTF16/callback bookkeeping; opaque system dialog allocations
// are not claimed to fit this charge or to be synchronously interruptible.
pub(crate) const INPUT_BYTES: u64 = 64 * 1024;
pub(crate) enum Event<'a> {
    Check,
    Select(&'a [u16]),
}
struct Context<'a, F> {
    callback: UnsafeCell<F>,
    busy: Cell<bool>,
    first_error: Cell<Option<Error>>,
    ledger: &'a Ledger,
}
unsafe extern "C" {
    fn morrow_directory_picker(
        callback: unsafe extern "C" fn(*mut c_void, u32, *const u16, usize) -> i32,
        context: *mut c_void,
        nomination: *const u16,
        count: usize,
    ) -> i32;
}
unsafe extern "C" fn dispatch<F: FnMut(Event<'_>) -> Result<(), Error>>(
    opaque: *mut c_void,
    kind: u32,
    path: *const u16,
    count: usize,
) -> i32 {
    // C++ calls this synchronously on the same original STA worker. It retains
    // that stack forever on ambiguous native retirement, rather than freeing a
    // context which might still be referenced by COM or a modal callback.
    let context = unsafe { &*opaque.cast::<Context<'_, F>>() };
    if kind == 2 {
        context.ledger.picker_unknown();
        return 1;
    }
    if context.first_error.get().is_some() || context.busy.replace(true) {
        return 1;
    }
    let result = catch_unwind(AssertUnwindSafe(|| {
        let event = match kind {
            0 if path.is_null() && count == 0 => Event::Check,
            1 if !path.is_null() && (1..=4096).contains(&count) => {
                Event::Select(unsafe { std::slice::from_raw_parts(path, count) })
            }
            _ => return Err(Error::InvalidNativeRecord),
        };
        unsafe { (&mut *context.callback.get())(event) }
    }))
    .unwrap_or(Err(Error::NativePickerUnavailable));
    context.busy.set(false);
    match result {
        Ok(()) => 0,
        Err(error) => {
            context.first_error.set(Some(error));
            1
        }
    }
}

pub(crate) fn show<F: FnMut(Event<'_>) -> Result<(), Error>>(
    ledger: &Ledger,
    callback: F,
) -> Result<(), Error> {
    show_impl(ledger, None, callback)
}

/// A display-only hint from the existing watch approval. Ordinary directory
/// selection keeps its original API and has no pre-existing nomination.
pub(crate) fn show_nominated<F: FnMut(Event<'_>) -> Result<(), Error>>(
    ledger: &Ledger,
    nomination: &[u16],
    callback: F,
) -> Result<(), Error> {
    if !(4..=4097).contains(&nomination.len())
        || nomination.last() != Some(&0)
        || nomination[..nomination.len() - 1].contains(&0)
    {
        return Err(Error::InvalidNativeRecord);
    }
    show_impl(ledger, Some(nomination), callback)
}

fn show_impl<F: FnMut(Event<'_>) -> Result<(), Error>>(
    ledger: &Ledger,
    nomination: Option<&[u16]>,
    callback: F,
) -> Result<(), Error> {
    ledger.begin_picker()?;
    let mut context = Context {
        callback: UnsafeCell::new(callback),
        busy: Cell::new(false),
        first_error: Cell::new(None),
        ledger,
    };
    let status = unsafe {
        morrow_directory_picker(
            dispatch::<F>,
            (&mut context as *mut Context<'_, F>).cast(),
            nomination.map_or(std::ptr::null(), |v| v.as_ptr()),
            nomination.map_or(0, |v| v.len() - 1),
        )
    };
    // Native return means all owned callbacks/timer/COM references retired on
    // the original worker. This says nothing about its File close or join.
    ledger.finish_picker()?;
    if let Some(error) = context.first_error.get() {
        return Err(error);
    }
    match status {
        0 => Ok(()),
        1 => Err(Error::Cancelled),
        _ => Err(Error::NativePickerUnavailable),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn invoke<F: FnMut(Event<'_>) -> Result<(), Error>>(
        context: &mut Context<'_, F>,
        kind: u32,
        path: *const u16,
        count: usize,
    ) -> i32 {
        unsafe { dispatch::<F>((context as *mut Context<'_, F>).cast(), kind, path, count) }
    }

    #[test]
    fn native_bridge_preflight_cancellation_returns_without_a_dialog() {
        let ledger = Ledger::new();
        ledger.bind_worker();
        let calls = Cell::new(0);
        assert_eq!(
            show(&ledger, |event| {
                assert!(matches!(event, Event::Check));
                calls.set(calls.get() + 1);
                Err(Error::Cancelled)
            }),
            Err(Error::Cancelled)
        );
        assert_eq!(calls.get(), 1);
        let observation = ledger.snapshot();
        assert_eq!(observation.objects, 0);
        assert!(observation.picker_started && observation.picker_closed);
        assert!(observation.all_closed());
    }

    #[test]
    fn nominated_bridge_preflight_cancel_keeps_original_callback_and_no_dialog() {
        let ledger = Ledger::new();
        ledger.bind_worker();
        let calls = Cell::new(0);
        assert_eq!(
            show_nominated(&ledger, &[68, 58, 92, 0], |event| {
                assert!(matches!(event, Event::Check));
                calls.set(calls.get() + 1);
                Err(Error::Cancelled)
            }),
            Err(Error::Cancelled)
        );
        assert_eq!(calls.get(), 1);
        let observation = ledger.snapshot();
        assert!(observation.picker_started && observation.picker_closed);
        assert!(observation.all_closed() && observation.objects == 0);
    }

    #[test]
    fn invalid_prefill_is_refused_before_native_picker_or_callback() {
        for value in [
            vec![],
            vec![68, 58, 92],
            vec![68, 58, 92, 0, 0],
            vec![1; 4098],
        ] {
            let ledger = Ledger::new();
            ledger.bind_worker();
            assert_eq!(
                show_nominated(&ledger, &value, |_| panic!("no callback")),
                Err(Error::InvalidNativeRecord)
            );
            assert!(!ledger.snapshot().picker_started);
        }
    }

    #[test]
    fn malformed_callback_is_rejected_before_path_access_and_is_sticky() {
        let ledger = Ledger::new();
        let calls = Cell::new(0);
        let mut context = Context {
            callback: UnsafeCell::new(|_: Event<'_>| {
                calls.set(calls.get() + 1);
                Ok(())
            }),
            busy: Cell::new(false),
            first_error: Cell::new(None),
            ledger: &ledger,
        };
        assert_eq!(
            invoke(&mut context, 1, std::ptr::dangling::<u16>(), 4097),
            1
        );
        assert_eq!(context.first_error.get(), Some(Error::InvalidNativeRecord));
        assert_eq!(invoke(&mut context, 0, std::ptr::null(), 0), 1);
        assert_eq!(calls.get(), 0);
    }

    #[test]
    fn callback_panic_never_crosses_native_boundary_or_enables_retry() {
        let ledger = Ledger::new();
        let mut context = Context {
            callback: UnsafeCell::new(|_: Event<'_>| -> Result<(), Error> {
                panic!("test-only callback failure")
            }),
            busy: Cell::new(false),
            first_error: Cell::new(None),
            ledger: &ledger,
        };
        assert_eq!(invoke(&mut context, 0, std::ptr::null(), 0), 1);
        assert_eq!(
            context.first_error.get(),
            Some(Error::NativePickerUnavailable)
        );
        assert!(!context.busy.get());
        assert_eq!(invoke(&mut context, 0, std::ptr::null(), 0), 1);
    }

    #[test]
    fn nested_callback_cannot_borrow_the_mutable_closure() {
        let ledger = Ledger::new();
        let calls = Cell::new(0);
        let mut context = Context {
            callback: UnsafeCell::new(|_: Event<'_>| {
                calls.set(calls.get() + 1);
                Ok(())
            }),
            busy: Cell::new(true),
            first_error: Cell::new(None),
            ledger: &ledger,
        };
        assert_eq!(invoke(&mut context, 0, std::ptr::null(), 0), 1);
        assert_eq!(calls.get(), 0);
        assert!(context.busy.get());
    }
}
