#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
#[cfg(windows)]
#[path = "../workbench_supervisor.rs"]
mod supervisor;
fn main() {
    #[cfg(windows)]
    {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        let result = runtime.block_on(supervisor::run());
        match result {
            Ok(code) => morrow_native_pipe_win::job::terminate_current_process(code),
            Err(e) => {
                runtime.block_on(async {
                    use tokio::io::AsyncWriteExt;
                    let mut stderr = tokio::io::stderr();
                    let detail = format!("Morrow supervisor: {e}\n");
                    let _ = tokio::time::timeout(std::time::Duration::from_millis(25), async {
                        stderr.write_all(detail.as_bytes()).await?;
                        stderr.flush().await
                    })
                    .await;
                });
                morrow_native_pipe_win::job::terminate_current_process(2)
            }
        }
    }
    #[cfg(not(windows))]
    {
        eprintln!("Windows workbench supervision is unavailable on this platform");
        std::process::exit(2);
    }
}
