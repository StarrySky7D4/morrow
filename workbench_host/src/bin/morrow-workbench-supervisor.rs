#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
fn main() {
    #[cfg(windows)]
    {
        // Source047 private A1 P path precedes the legacy runtime, lease
        // callback, kill-on-drop JobChild and unconditional TerminateProcess.
        // Its own failure core parks; normal C/native closure returns normally.
        let arguments=std::env::args().skip(1).collect::<Vec<_>>();
        if let Some(code)=morrow_workbench_host::workbench_supervision::run_asuser_parent(&arguments) {
            if code==0{return;}
            std::process::exit(code as i32); // invalid static selector, no effects
        }
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        let result = runtime.block_on(morrow_workbench_host::workbench_supervision::run_supervisor());
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
