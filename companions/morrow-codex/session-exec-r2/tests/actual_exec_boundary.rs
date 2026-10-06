//! Actual upstream ExecBackend dispatch: unsupported modes never reach the
//! trusted review/start boundary, and a denied domain never falls back to spawn.
use codex_exec_server::{ExecBackend, ExecBackendFuture, ExecParams, ExecServerError};
use morrow_agent_session_exec_v1_r2::Intent;
use morrow_codex_session_exec_r2::exec_backend::{
    MorrowExecBackend, ReviewedArtifact, ReviewedExecConnection,
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[derive(Default)]
struct DeniedDomain {
    reviewed: AtomicUsize,
    starts: AtomicUsize,
}
impl ReviewedExecConnection for DeniedDomain {
    fn review_artifact(&self, _params: &ExecParams) -> Result<ReviewedArtifact, ExecServerError> {
        self.reviewed.fetch_add(1, Ordering::SeqCst);
        Err(ExecServerError::Protocol("domain review denied".into()))
    }
    fn start_claimed(&self, _intent: Intent, _params: ExecParams) -> ExecBackendFuture<'_> {
        self.starts.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Err(ExecServerError::Protocol("no admitted executor".into())) })
    }
}
fn params() -> ExecParams {
    serde_json::from_value(serde_json::json!({ "processId":"actual-codex-fixed-test","argv":["/usr/bin/printf","hello"],"cwd":"file:///tmp","env":{},"tty":false })).unwrap()
}
#[test]
fn actual_exec_backend_rejects_deferred_modes_before_review_and_denied_domains_before_start() {
    let connection = Arc::new(DeniedDomain::default());
    let backend: Arc<dyn ExecBackend> = Arc::new(MorrowExecBackend::new(connection.clone()));
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let mut tty = params();
        tty.tty = true;
        assert!(backend.start(tty).await.is_err());
        let mut pipe = params();
        pipe.pipe_stdin = true;
        assert!(backend.start(pipe).await.is_err());
        let mut arg0 = params();
        arg0.arg0 = Some("override".into());
        assert!(backend.start(arg0).await.is_err());
        let mut empty = params();
        empty.argv.clear();
        assert!(backend.start(empty).await.is_err());
        assert_eq!(connection.reviewed.load(Ordering::SeqCst), 0);
        assert!(backend.start(params()).await.is_err());
        assert_eq!(connection.reviewed.load(Ordering::SeqCst), 1);
        assert_eq!(connection.starts.load(Ordering::SeqCst), 0);
    });
}
