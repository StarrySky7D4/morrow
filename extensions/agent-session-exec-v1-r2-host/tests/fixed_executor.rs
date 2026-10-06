#![cfg(target_os = "linux")]
use morrow_agent_session_exec_v1_r2::{Environment, Error, Intent, hash};
use morrow_agent_session_exec_v1_r2_host::fixed_executor::LinuxFixedExecutor;
fn intent(program: &str, cwd: &str, args: Vec<String>, input: Vec<u8>) -> Intent {
    Intent {
        operation_id: "fixed".into(),
        artifact_sha256: hash(&std::fs::read(program).unwrap()),
        program: program.into(),
        argv: args,
        cwd: cwd.into(),
        env: vec![],
        input,
        execution_domain: "fixed-domain".into(),
        max_runtime_ms: 1000,
    }
}
#[test]
fn delivers_fixed_input_and_observes_true_exit_and_output_eof() {
    let temp = tempfile::tempdir().unwrap();
    let cwd = temp.path().to_str().unwrap();
    let request = intent("/usr/bin/cat", cwd, vec![], vec![b'x'; 32768]);
    let exec = LinuxFixedExecutor::register(
        &request.program,
        cwd,
        &request.execution_domain,
        request.artifact_sha256,
        65536,
    )
    .unwrap();
    let result = exec.execute(&request).unwrap();
    assert_eq!(result.stdout, request.input);
    assert!(result.stderr.is_empty());
    assert_eq!(result.facts.exit_code, Some(0));
    assert!(result.facts.output_closed);
    assert_eq!(result.facts.stdout_sha256, hash(&request.input));
}
#[test]
fn executes_sealed_reviewed_image_after_path_replacement() {
    let temp = tempfile::tempdir().unwrap();
    let cwd = temp.path().to_str().unwrap();
    let path = temp.path().join("artifact");
    std::fs::copy("/usr/bin/cat", &path).unwrap();
    let request = intent(path.to_str().unwrap(), cwd, vec![], b"fixed-input".to_vec());
    let exec = LinuxFixedExecutor::register(
        &request.program,
        cwd,
        &request.execution_domain,
        request.artifact_sha256,
        65536,
    )
    .unwrap();
    std::fs::copy("/usr/bin/false", &path).unwrap();
    let result = exec.execute(&request).unwrap();
    assert_eq!(result.stdout, b"fixed-input");
    assert_eq!(result.facts.exit_code, Some(0));
}
#[test]
fn fixed_environment_cannot_inherit_host_environment() {
    let temp = tempfile::tempdir().unwrap();
    let cwd = temp.path().to_str().unwrap();
    let mut request = intent("/usr/bin/env", cwd, vec![], vec![]);
    request.env = vec![Environment {
        name: "MORROW_FIXED".into(),
        value: "declared".into(),
    }];
    let exec = LinuxFixedExecutor::register(
        &request.program,
        cwd,
        &request.execution_domain,
        request.artifact_sha256,
        65536,
    )
    .unwrap();
    assert_eq!(
        exec.execute(&request).unwrap().stdout,
        b"MORROW_FIXED=declared\n"
    );
}
#[test]
fn timeout_reaps_the_actual_process_and_reports_signal_fact() {
    let temp = tempfile::tempdir().unwrap();
    let cwd = temp.path().to_str().unwrap();
    let mut request = intent("/usr/bin/sleep", cwd, vec!["5".into()], vec![]);
    request.max_runtime_ms = 30;
    let exec = LinuxFixedExecutor::register(
        &request.program,
        cwd,
        &request.execution_domain,
        request.artifact_sha256,
        65536,
    )
    .unwrap();
    let start = std::time::Instant::now();
    let result = exec.execute(&request).unwrap();
    assert!(result.timed_out);
    assert_eq!(result.facts.exit_code, Some(-9));
    assert!(result.facts.output_closed);
    assert!(start.elapsed() < std::time::Duration::from_secs(2));
}
#[test]
fn excessive_output_is_bounded_and_cannot_claim_complete_facts() {
    let temp = tempfile::tempdir().unwrap();
    let cwd = temp.path().to_str().unwrap();
    let request = intent("/usr/bin/yes", cwd, vec![], vec![]);
    let exec = LinuxFixedExecutor::register(
        &request.program,
        cwd,
        &request.execution_domain,
        request.artifact_sha256,
        1024,
    )
    .unwrap();
    assert!(matches!(exec.execute(&request), Err(Error::Limit)));
}
#[test]
fn artifact_domain_and_input_identity_are_rechecked() {
    let temp = tempfile::tempdir().unwrap();
    let cwd = temp.path().to_str().unwrap();
    let mut request = intent("/usr/bin/cat", cwd, vec![], vec![]);
    assert!(matches!(
        LinuxFixedExecutor::register(
            &request.program,
            cwd,
            &request.execution_domain,
            [8; 32],
            1024
        ),
        Err(Error::Denied)
    ));
    let exec = LinuxFixedExecutor::register(
        &request.program,
        cwd,
        &request.execution_domain,
        request.artifact_sha256,
        1024,
    )
    .unwrap();
    request.execution_domain = "foreign-domain".into();
    assert!(matches!(exec.execute(&request), Err(Error::Denied)));
}

#[test]
fn deadline_closes_descendant_stdio_after_leader_exits_without_reusing_reaped_pid() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("leader-exit.c");
    let artifact = temp.path().join("leader-exit");
    std::fs::write(
        &source,
        r#"
#define _POSIX_C_SOURCE 200809L
#include <unistd.h>
#include <time.h>
#include <stdlib.h>
int main(void) {
    int ready[2];
    if (pipe(ready) != 0) return 2;
    pid_t child = fork();
    if (child < 0) return 3;
    if (child == 0) {
        close(ready[0]);
        const char held[] = "descendant-holds-stdio\n";
        if (write(STDOUT_FILENO, held, sizeof(held)-1) != sizeof(held)-1) _exit(4);
        if (write(ready[1], "x", 1) != 1) _exit(5);
        close(ready[1]);
        struct timespec pause = { .tv_sec = 0, .tv_nsec = 800000000 };
        nanosleep(&pause, NULL);
        const char late[] = "descendant-natural-finish\n";
        write(STDOUT_FILENO, late, sizeof(late)-1);
        _exit(0);
    }
    close(ready[1]);
    char byte;
    if (read(ready[0], &byte, 1) != 1) return 6;
    close(ready[0]);
    const char exited[] = "leader-exits-first\n";
    if (write(STDERR_FILENO, exited, sizeof(exited)-1) != sizeof(exited)-1) return 7;
    // The descendant retains both original stdio descriptors for at most 800ms.
    _exit(0);
}
"#,
    )
    .unwrap();
    let compiled = std::process::Command::new("cc")
        .args(["-std=c11", "-O2", "-Wall", "-Wextra", "-Werror"])
        .arg(&source)
        .arg("-o")
        .arg(&artifact)
        .output()
        .unwrap();
    assert!(
        compiled.status.success(),
        "fixture compilation: {}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let cwd = temp.path().to_str().unwrap();
    let mut request = intent(artifact.to_str().unwrap(), cwd, vec![], vec![]);
    request.max_runtime_ms = 75;
    let executor = LinuxFixedExecutor::register(
        &request.program,
        cwd,
        &request.execution_domain,
        request.artifact_sha256,
        1024,
    )
    .unwrap();
    let started = std::time::Instant::now();
    let result = executor.execute(&request).unwrap();
    assert!(result.timed_out);
    // The leader exited normally before the deadline; group cleanup only killed
    // the descendant. Exit and observed EOF are separate, truthful facts.
    assert_eq!(result.facts.exit_code, Some(0));
    assert!(result.facts.output_closed);
    assert_eq!(result.stdout, b"descendant-holds-stdio\n");
    assert_eq!(result.stderr, b"leader-exits-first\n");
    assert_eq!(result.facts.stdout_sha256, hash(&result.stdout));
    assert_eq!(result.facts.stderr_sha256, hash(&result.stderr));
    assert!(started.elapsed() >= std::time::Duration::from_millis(75));
    assert!(started.elapsed() < std::time::Duration::from_millis(750));
}
