//! Independent trusted resource owner. No extra nested business-host process.
#[allow(dead_code)]
mod cli {include!("../main.rs");pub async fn supervised()->morrow_native_session_stream::Result<()> {run(true).await}}
#[tokio::main(flavor="multi_thread",worker_threads=2)]
async fn main(){if let Err(error)=cli::supervised().await {
 use tokio::io::AsyncWriteExt;
 let mut diagnostic=format!("native supervisor: {error}\n").into_bytes();diagnostic.truncate(1024);
 let mut stderr=tokio::io::stderr();
 let _=tokio::time::timeout(std::time::Duration::from_millis(25),async {
  stderr.write_all(&diagnostic).await?;
  stderr.flush().await
 }).await;
 morrow_native_pipe_win::job::terminate_current_process(2);
}}
