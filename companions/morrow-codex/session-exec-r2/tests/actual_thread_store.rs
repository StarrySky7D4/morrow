//! Actual upstream ThreadStore calls through canonical frames to a separate
//! process owning genuine HostRuntime/Connection/Admission/SQLite authority.
use chrono::{DateTime, Utc};
use codex_protocol::{
    ThreadId,
    models::{BaseInstructions, ContentItem, ResponseItem},
    protocol::{SessionSource, ThreadHistoryMode, ThreadMemoryMode},
};
use codex_rollout::{ResponseItemEnvelope, RolloutItem};
use codex_thread_store::*;
use morrow_agent_session_exec_v1_r2::{Action, Error, Event, Outcome, Reply, Request};
use morrow_codex_session_exec_r2::{
    native_connection::{
        CanonicalSessionControl, OriginalNativeEndpoint, ReviewedNativeSessionPort,
    },
    thread_store::MorrowThreadStore,
};
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

struct Process {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
}
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
struct Rpc(Mutex<Process>, AtomicBool, AtomicBool);
impl Rpc {
    fn send(&self, value: Value) -> ThreadStoreResult<Value> {
        let mut process = self.0.lock().map_err(internal)?;
        serde_json::to_writer(&mut process.input, &value).map_err(internal)?;
        process.input.write_all(b"\n").map_err(internal)?;
        process.input.flush().map_err(internal)?;
        let mut line = String::new();
        process.output.read_line(&mut line).map_err(internal)?;
        let response: Value = serde_json::from_str(&line).map_err(internal)?;
        if let Some(error) = response.get("error") {
            return Err(internal(error));
        }
        Ok(response["ok"].clone())
    }
}
struct Endpoint {
    rpc: Arc<Rpc>,
    id: u64,
    generation: u64,
}
impl OriginalNativeEndpoint for Endpoint {
    fn generation(&self) -> u64 {
        self.generation
    }
    fn exchange(&self, frame: &[u8]) -> ThreadStoreResult<Vec<u8>> {
        let request = Request::decode(frame).map_err(internal)?;
        if matches!(request.action(), Action::Append { .. })
            && self.rpc.1.swap(false, Ordering::SeqCst)
        {
            return Err(internal("injected transport loss before append"));
        }
        if matches!(request.action(), Action::Checkpoint { .. })
            && self.rpc.2.swap(false, Ordering::SeqCst)
        {
            return Err(internal("injected transport loss before checkpoint"));
        }
        serde_json::from_value(
            self.rpc
                .send(json!({"operation":"exchange","endpoint":self.id,"frame":frame}))?["frame"]
                .clone(),
        )
        .map_err(internal)
    }
    fn revoke(&self) -> ThreadStoreResult<()> {
        self.rpc
            .send(json!({"operation":"revoke","endpoint":self.id}))?;
        Ok(())
    }
}
struct Port {
    rpc: Arc<Rpc>,
    control: Arc<Endpoint>,
    writers: Mutex<Vec<Arc<Endpoint>>>,
}
impl ReviewedNativeSessionPort for Port {
    fn control(&self) -> Arc<dyn OriginalNativeEndpoint> {
        self.control.clone()
    }
    fn admit_writer(&self, session: &str) -> ThreadStoreResult<Arc<dyn OriginalNativeEndpoint>> {
        let value = self
            .rpc
            .send(json!({"operation":"admit_writer","session":session}))?;
        let endpoint = Arc::new(Endpoint {
            rpc: self.rpc.clone(),
            id: value["endpoint"].as_u64().unwrap(),
            generation: value["generation"].as_u64().unwrap(),
        });
        self.writers
            .lock()
            .map_err(internal)?
            .push(endpoint.clone());
        Ok(endpoint)
    }
    fn retire(&self, session: &str) -> ThreadStoreResult<()> {
        self.rpc
            .send(json!({"operation":"retire","session":session}))?;
        Ok(())
    }
    fn now(&self) -> DateTime<Utc> {
        DateTime::from_timestamp(1, 0).unwrap()
    }
}
fn params(thread_id: ThreadId) -> CreateThreadParams {
    CreateThreadParams {
        creator_user_id: None,
        creator_account_id: None,
        session_id: thread_id.into(),
        thread_id,
        extra_config: None,
        forked_from_id: None,
        parent_thread_id: None,
        source: SessionSource::Cli,
        thread_source: None,
        originator: "codex-actual-r2-test".into(),
        base_instructions: BaseInstructions {
            text: "test instructions".into(),
            provenance: None,
        },
        dynamic_tools: Vec::new(),
        selected_capability_roots: Vec::new(),
        multi_agent_version: None,
        history_mode: ThreadHistoryMode::Legacy,
        history_base: None,
        subagent_history_start_ordinal: None,
        initial_window_id: "window-test".into(),
        runtime_workspace_roots: None,
        metadata: ThreadPersistenceMetadata {
            cwd: Some(std::path::PathBuf::from("/tmp")),
            model_provider: "test-provider".into(),
            memory_mode: ThreadMemoryMode::Disabled,
        },
    }
}
fn message(text: &str) -> RolloutItem {
    RolloutItem::ResponseItem(ResponseItemEnvelope::new(ResponseItem::Message {
        id: None,
        role: "user".into(),
        content: vec![ContentItem::InputText { text: text.into() }],
        phase: None,
        internal_chat_message_metadata_passthrough: None,
    }))
}
fn list(page_size: usize, cursor: Option<String>) -> ListThreadsParams {
    ListThreadsParams {
        page_size,
        cursor,
        sort_key: ThreadSortKey::CreatedAt,
        sort_direction: SortDirection::Asc,
        allowed_sources: Vec::new(),
        model_providers: Some(Vec::new()),
        cwd_filters: None,
        section: None,
        project_id: None,
        archived: false,
        search_term: None,
        relation_filter: None,
        use_state_db_only: true,
    }
}

#[test]
fn genuine_core_original_admissions_qualify_actual_codex_thread_store() {
    let host = std::env::var_os("MORROW_SESSION_EXEC_R2_HOST_FIXTURE").expect(
        "build the genuine separate host fixture and set MORROW_SESSION_EXEC_R2_HOST_FIXTURE",
    );
    let temp = tempfile::tempdir().unwrap();
    let a = ThreadId::from_u128(1);
    let b = ThreadId::from_u128(2);
    let c = ThreadId::from_u128(3);
    let mut child = Command::new(host)
        .arg(temp.path().join("core.sqlite"))
        .arg(a.to_string())
        .arg(b.to_string())
        .arg(c.to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let input = child.stdin.take().unwrap();
    let output = BufReader::new(child.stdout.take().unwrap());
    let rpc = Arc::new(Rpc(
        Mutex::new(Process {
            child,
            input,
            output,
        }),
        AtomicBool::new(false),
        AtomicBool::new(false),
    ));
    let hello = rpc.send(json!({"operation":"hello"})).unwrap();
    let port = Arc::new(Port {
        rpc: rpc.clone(),
        control: Arc::new(Endpoint {
            rpc,
            id: 0,
            generation: hello["generation"].as_u64().unwrap(),
        }),
        writers: Mutex::new(Vec::new()),
    });
    let control = Arc::new(CanonicalSessionControl::new(port.clone()).unwrap());
    let store: Arc<dyn ThreadStore> = Arc::new(MorrowThreadStore::new(control));
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        assert!(store.stage_pending_thread_metadata(a,ThreadMetadataPatch::default()).await.is_err());
        store.stage_pending_thread_metadata(a, ThreadMetadataPatch { name:Some(Some("pending title".into())), ..Default::default() }).await.unwrap();
        assert!(store.stage_pending_thread_metadata(a,ThreadMetadataPatch { name:Some(Some("must not replace pending".into())),..Default::default() }).await.is_err());
        store.create_thread(params(a)).await.unwrap();
        for index in 0..20 { store.append_items(AppendThreadItemsParams { thread_id:a, items:vec![message(&format!("user {index}"))] }).await.unwrap(); }
        for _ in 0..8 { store.append_items(AppendThreadItemsParams { thread_id:a,items:vec![message(&"large".repeat(4096))] }).await.unwrap(); }
        store.persist_thread(a, PersistContext::SteeredUserInput).await.unwrap();
        store.flush_thread(a).await.unwrap();
        let read = store.read_thread(ReadThreadParams { thread_id:a, include_archived:false, include_history:true }).await.unwrap();
        assert_eq!(read.name.as_deref(), Some("pending title"));
        let history = read.history.unwrap();
        assert_eq!(history.items.len(),29);
        assert!(matches!(&history.items[0],RolloutItem::SessionMeta(meta) if meta.meta.id==a && meta.meta.base_instructions.as_ref().unwrap().text=="test instructions"));
        let original = port.writers.lock().unwrap()[0].clone();
        store.append_items(AppendThreadItemsParams { thread_id:a,items:vec![message("durable before failed checkpoint")] }).await.unwrap();
        port.rpc.2.store(true,Ordering::SeqCst);
        assert!(store.shutdown_thread(a).await.is_err());
        let stale = Request::new_for_generation("stale-original-writer",original.generation(),Action::Append { session_id:a.to_string(),epoch:1,expected_tail:30,events:vec![Event { event_id:"stale".into(),body:b"cannot-write".to_vec() }] }).unwrap();
        assert_original_denied(&original,&stale);
        store.archive_thread(ArchiveThreadParams { thread_id:a }).await.unwrap();
        assert!(matches!(store.load_history(LoadThreadHistoryParams { thread_id:a,include_archived:false }).await,Err(ThreadStoreError::ThreadNotFound { .. })));
        store.unarchive_thread(ArchiveThreadParams { thread_id:a }).await.unwrap();
        port.rpc.1.store(true,Ordering::SeqCst);
        assert!(store.resume_thread(ResumeThreadParams { thread_id:a,rollout_path:None,history:None,include_archived:false,metadata:params(a).metadata }).await.is_err());
        let failed_resume = port.writers.lock().unwrap().last().unwrap().clone();
        assert_original_denied(&failed_resume,&stale);
        store.resume_thread(ResumeThreadParams { thread_id:a,rollout_path:None,history:None,include_archived:false,metadata:params(a).metadata }).await.unwrap();
        store.update_thread_metadata(UpdateThreadMetadataParams { thread_id:a,include_archived:false,patch:ThreadMetadataPatch { name:Some(None),model:Some("test-model".into()),..Default::default() } }).await.unwrap();
        assert!(store.read_pending_thread_metadata(a).await.unwrap().is_none());
        assert_eq!(store.read_thread(ReadThreadParams { thread_id:a,include_archived:false,include_history:false }).await.unwrap().name,None);
        store.discard_thread(a).await.unwrap();
        assert_eq!(store.load_latest_model_context(LoadThreadHistoryParams { thread_id:a,include_archived:false }).await.unwrap().items.len(),30);
        port.rpc.1.store(true,Ordering::SeqCst);
        assert!(store.create_thread(params(c)).await.is_err());
        let failed_create = port.writers.lock().unwrap().last().unwrap().clone();
        let rejected = Request::new_for_generation("failed-create-original-writer",failed_create.generation(),Action::Append {session_id:c.to_string(),epoch:1,expected_tail:0,events:vec![Event {event_id:"after-failure".into(),body:Vec::new()}]}).unwrap();
        assert_original_denied(&failed_create,&rejected);
        assert_eq!(store.list_threads(list(16,None)).await.unwrap().items.len(),1);
        store.delete_thread(DeleteThreadParams { thread_id:c }).await.unwrap();
        store.create_thread(params(b)).await.unwrap();
        let first = store.list_threads(list(1,None)).await.unwrap();
        assert_eq!(first.items[0].thread_id,a);
        let second = store.list_threads(list(1,first.next_cursor)).await.unwrap();
        assert_eq!(second.items[0].thread_id,b);
        assert!(second.next_cursor.is_none());
        store.shutdown_thread(b).await.unwrap();
        store.delete_thread(DeleteThreadParams { thread_id:a }).await.unwrap();
        assert!(matches!(store.read_thread(ReadThreadParams { thread_id:a,include_archived:true,include_history:true }).await,Err(ThreadStoreError::ThreadNotFound { .. })));
        assert_eq!(store.list_threads(list(16,None)).await.unwrap().items.len(),1);
    });
}
fn assert_original_denied(endpoint: &Endpoint, request: &Request) {
    match endpoint.exchange(request.raw()) {
        Ok(bytes) => assert!(matches!(
            Reply::decode_for(request, &bytes).unwrap().outcome,
            Outcome::Rejected(Error::Denied)
        )),
        Err(ThreadStoreError::Internal { message }) => assert!(message.contains("Denied")),
        Err(error) => panic!("unexpected original endpoint error: {error}"),
    }
}
fn internal(message: impl ToString) -> ThreadStoreError {
    ThreadStoreError::Internal {
        message: message.to_string(),
    }
}
