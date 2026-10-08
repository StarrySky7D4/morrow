use super::{join_output_reader, verified_exit_code};
use std::io;
use std::time::{Duration, Instant};

#[test]
fn timeout_failed_wait_and_failed_exit_code_never_authorize_terminal() {
    assert!(verified_exit_code(0x102, true, 1).is_err());
    assert!(verified_exit_code(u32::MAX, true, 1).is_err());
    assert!(verified_exit_code(0, false, 1).is_err());
    assert_eq!(verified_exit_code(0, true, 13).unwrap(), 13);
}

#[test]
fn output_reader_panic_and_io_failure_cannot_be_eof() {
    let failed = std::thread::spawn(|| Err(io::Error::other("synthetic reader failure")));
    assert!(join_output_reader(failed, Instant::now() + Duration::from_secs(1)).is_err());
    let panicked = std::thread::spawn(|| -> io::Result<()> { panic!("synthetic reader panic") });
    assert!(join_output_reader(panicked, Instant::now() + Duration::from_secs(1)).is_err());
    let eof = std::thread::spawn(|| Ok(()));
    assert!(join_output_reader(eof, Instant::now() + Duration::from_secs(1)).is_ok());
}

#[test]
fn absent_output_eof_is_bounded_and_never_joined_as_success() {
    let (release, wait) = std::sync::mpsc::channel();
    let (finished, done) = std::sync::mpsc::channel();
    let reader = std::thread::spawn(move || {
        wait.recv_timeout(Duration::from_secs(2)).unwrap();
        finished.send(()).unwrap();
        Ok(())
    });
    let started = Instant::now();
    assert_eq!(
        join_output_reader(reader, started + Duration::from_millis(20))
            .unwrap_err()
            .kind(),
        io::ErrorKind::TimedOut,
    );
    assert!(started.elapsed() < Duration::from_secs(1));
    release.send(()).unwrap();
    done.recv_timeout(Duration::from_secs(1)).unwrap();
}
// Pure supplied-result/thread tests; no child, protected account, or sandbox proof.
