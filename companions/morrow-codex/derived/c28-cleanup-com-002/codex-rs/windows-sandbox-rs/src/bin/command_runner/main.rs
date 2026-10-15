#![cfg_attr(all(target_os = "windows", not(test)), windows_subsystem = "windows")]

#[cfg(target_os = "windows")]
mod win;

#[cfg(target_os = "windows")]
fn main() {
    let status = match std::panic::catch_unwind(win::main) {
        Ok(Ok(exit_code)) => exit_code,
        Ok(Err(err)) => {
            if win::has_unresolved_attempt_slot_entries() {
                let _ = win::hold_until_native_process_exit_allowed();
            }
            eprintln!("Error: {err:?}");
            1
        }
        Err(panic_payload) => {
            win::mark_attempt_slot_unwound();
            if win::has_unresolved_attempt_slot_entries() {
                let _ = win::hold_until_native_process_exit_allowed();
            }
            std::panic::resume_unwind(panic_payload);
        }
    };
    // win::main drops the local context only after native worker completion and
    // checked closure, or holds/reaps the same parked originals on every error.
    // Stable ExitCode only accepts u8; retain the original full Windows status here.
    if win::has_unresolved_attempt_slot_entries() {
        let _ = win::hold_until_native_process_exit_allowed();
    }
    std::process::exit(status);
}

#[cfg(not(target_os = "windows"))]
fn main() {
    panic!("codex-command-runner is Windows-only");
}
