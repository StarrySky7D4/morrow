//! Synthetic workspace-only fixture. Not a product entry point.
use std::{
    io::{self, Read, Write},
    time::Duration,
};
fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("closed-stdio-check") => {
            use morrow_linux_supervisor_foundation::{OwnedProcess, SealedExecutable, digest};
            use std::{ffi::OsStr, fs, path::Path};
            let artifact = Path::new(&args[1]);
            let cwd = Path::new(&args[2]);
            let receipt = Path::new(&args[3]);
            let expected = digest(&fs::read(artifact).unwrap());
            // Separate subprocess: never close the test harness's stdio.
            unsafe {
                libc::close(0);
                libc::close(1);
                libc::close(2);
            }
            let executable = SealedExecutable::read(artifact, expected).unwrap();
            let mut child = OwnedProcess::spawn(executable, cwd, &[OsStr::new("echo")]).unwrap();
            let done = child.wait_bounded(Duration::from_secs(2)).unwrap();
            assert!(done && child.cleanup_complete());
            assert!(child.output().stdout.starts_with(b"sealed-main-elf\n"));
            assert_eq!(child.output().stderr, b"sealed-stderr\n");
            fs::write(receipt, b"closed-stdio-sealed-exec-pidfd-cleanup-ok\n").unwrap();
        }
        Some("fd-check") => {
            let extra = (3..1024)
                .filter(|fd| unsafe { libc::fcntl(*fd, libc::F_GETFD) } >= 0)
                .count();
            println!("extra-open-fds={extra}");
        }
        Some("echo") => {
            println!("sealed-main-elf");
            eprintln!("sealed-stderr");
            println!("env-count={}", std::env::vars_os().count());
            let mut input = String::new();
            io::stdin().read_to_string(&mut input).unwrap();
            println!("stdin-bytes={}", input.len());
            println!("cwd={}", std::env::current_dir().unwrap().display());
            std::process::exit(17);
        }
        Some("sleep") => std::thread::sleep(Duration::from_secs(30)),
        Some("flood") => {
            io::stdout().write_all(&vec![b'o'; 256 * 1024]).unwrap();
            io::stderr().write_all(&vec![b'e'; 128 * 1024]).unwrap();
        }
        Some("retain-output") => {
            // Deliberately demonstrate that a pidfd is not process-tree ownership.
            // Single-threaded fixture; child uses only libc sleep/_exit.
            let pid = unsafe { libc::fork() };
            assert!(pid >= 0);
            if pid == 0 {
                unsafe {
                    libc::usleep(200_000);
                    libc::_exit(0);
                }
            }
        }
        _ => std::process::exit(2),
    }
}
