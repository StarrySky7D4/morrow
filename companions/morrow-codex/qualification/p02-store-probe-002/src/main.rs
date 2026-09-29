//! Qualification only: real upstream LiveThread calls, bounded host-kit fake.
//! Successful fixture open/read is not production resume or durable storage.
use codex_protocol::ThreadId;
use codex_protocol::protocol::{ThreadHistoryMode, ThreadMemoryMode};
use codex_rollout::RolloutItem;
use codex_thread_store::*;
use kit::fake::{FakeHost, FixtureIdentity, hello};
use morrow_agent_host_contract::{self as kit, agent_host_capnp as wire};
use serde_json::{Value, json};
use std::any::Any;
use std::sync::{Arc, Mutex};

struct State {
    host: FakeHost,
    identity: FixtureIdentity,
    fail_on: Option<&'static str>,
    calls: Vec<String>,
    frames: Vec<Value>,
    request_id: u64,
}
struct FixtureStore(Mutex<State>);

fn internal(message: impl ToString) -> ThreadStoreError {
    ThreadStoreError::Internal {
        message: message.to_string(),
    }
}

impl FixtureStore {
    fn new(fail_on: Option<&'static str>) -> Self {
        let identity = FixtureIdentity::default();
        let mut host = FakeHost::new(identity.clone(), vec![]).expect("bounded fixture");
        kit::exchange_checked(&mut host, &hello(&identity, 1)).expect("fixture hello");
        Self(Mutex::new(State {
            host,
            identity,
            fail_on,
            calls: vec![],
            frames: vec![],
            request_id: 1,
        }))
    }

    fn unsupported<T: Send + 'static>(&self, operation: &'static str) -> ThreadStoreFuture<'_, T> {
        self.0.lock().unwrap().calls.push(operation.into());
        Box::pin(async move { Err(ThreadStoreError::Unsupported { operation }) })
    }

    fn event(
        &self,
        call: &'static str,
        op: &'static str,
        payload: Option<Vec<u8>>,
    ) -> ThreadStoreResult<Vec<RolloutItem>> {
        let mut state = self.0.lock().unwrap();
        state.calls.push(call.into());
        state.request_id += 1;
        let mut msg = kit::frame(
            state.request_id,
            &state.identity.session_id,
            state.identity.instance_epoch,
        );
        let mut event = msg
            .get_root::<wire::frame::Builder>()
            .map_err(internal)?
            .init_event();
        event.set_writer_epoch(1);
        match op {
            "open_writer" => event.set_open_writer(()),
            "read_after" => event.set_read_after(0),
            "append_batch" => {
                let payload = payload.ok_or_else(|| internal("missing append payload"))?;
                if payload.len() > kit::MAX_CHUNK_BYTES {
                    return Err(internal("fixture payload limit"));
                }
                let mut batch = event.init_append_batch();
                batch.set_producer_sequence(1);
                batch.set_expected_tail(0);
                let mut item = batch.init_events(1).get(0);
                item.set_turn_id("fixture-turn");
                item.set_item_id("fixture-item");
                item.set_part_id("fixture-part");
                item.set_attempt_id("fixture-attempt");
                item.set_semantic_kind("qualification.rollout-batch");
                item.set_source_digest(&kit::digest(&payload));
                item.set_payload(&payload);
            }
            _ => return Err(internal("unknown fixture event")),
        }
        let bytes = kit::encode(&msg).map_err(internal)?;
        // Decode the exact transport input before the deliberate disconnected refusal.
        kit::decode(&bytes).map_err(internal)?;
        let disconnected = state.fail_on == Some(op);
        state.frames.push(json!({"op":op,"bytes":bytes.len(),"sha256":hex(&kit::digest(&bytes)),"disconnected":disconnected}));
        if disconnected {
            return Err(internal(format!("qualification-disconnected:{op}")));
        }
        let reply = kit::exchange_checked(&mut state.host, &bytes).map_err(internal)?;
        let frame = reply.get_root::<wire::frame::Reader>().map_err(internal)?;
        let wire::frame::Which::Reply(receipt) = frame.which().map_err(internal)? else {
            return Err(internal("host did not return a reply"));
        };
        let receipt = receipt.map_err(internal)?;
        if !receipt.get_qualification_only() {
            return Err(internal("not a qualification receipt"));
        }
        let wire::reply::Which::Event(value) = receipt.which().map_err(internal)? else {
            return Err(internal("host did not return an Event receipt"));
        };
        let value = value.map_err(internal)?;
        if value.get_writer_epoch() != 1 {
            return Err(internal("unexpected writer epoch"));
        }
        // This slice supports only the empty fake history. Never invent stored items.
        if value.get_durable_sequence() != 0 || !value.get_events().map_err(internal)?.is_empty() {
            return Err(ThreadStoreError::Unsupported {
                operation: "nonempty_fixture_history_requires_M04",
            });
        }
        Ok(vec![])
    }
}

macro_rules! refuse {
    ($name:ident, $ty:ty, $out:ty) => {
        fn $name(&self, _params: $ty) -> ThreadStoreFuture<'_, $out> {
            self.unsupported(stringify!($name))
        }
    };
}
impl ThreadStore for FixtureStore {
    fn as_any(&self) -> &dyn Any {
        self
    }
    refuse!(create_thread, CreateThreadParams, ());
    fn resume_thread(&self, _params: ResumeThreadParams) -> ThreadStoreFuture<'_, ()> {
        Box::pin(async { self.event("resume_thread", "open_writer", None).map(|_| ()) })
    }
    fn append_items(&self, params: AppendThreadItemsParams) -> ThreadStoreFuture<'_, ()> {
        Box::pin(async move {
            let payload = serde_json::to_vec(&params.items).map_err(internal)?;
            self.event("append_items", "append_batch", Some(payload))?;
            Err(ThreadStoreError::Unsupported {
                operation: "durable_append_requires_M04",
            })
        })
    }
    fn load_history(
        &self,
        params: LoadThreadHistoryParams,
    ) -> ThreadStoreFuture<'_, StoredThreadHistory> {
        Box::pin(async move {
            Ok(StoredThreadHistory {
                thread_id: params.thread_id,
                items: self.event("load_history", "read_after", None)?,
            })
        })
    }
    fn persist_thread(
        &self,
        _thread_id: ThreadId,
        context: PersistContext,
    ) -> ThreadStoreFuture<'_, ()> {
        self.0
            .lock()
            .unwrap()
            .calls
            .push(format!("persist_context:{context:?}"));
        self.unsupported("persist_thread_requires_M04")
    }
    refuse!(flush_thread, ThreadId, ());
    refuse!(shutdown_thread, ThreadId, ());
    refuse!(discard_thread, ThreadId, ());
    refuse!(record_thread_metadata, UpdateThreadMetadataParams, ());
    refuse!(read_thread, ReadThreadParams, StoredThread);
    refuse!(
        read_thread_by_rollout_path,
        ReadThreadByRolloutPathParams,
        StoredThread
    );
    refuse!(list_threads, ListThreadsParams, ThreadPage);
    refuse!(
        update_thread_metadata,
        UpdateThreadMetadataParams,
        Option<StoredThread>
    );
    refuse!(archive_thread, ArchiveThreadParams, ());
    refuse!(unarchive_thread, ArchiveThreadParams, StoredThread);
    refuse!(delete_thread, DeleteThreadParams, ());
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|v| format!("{v:02x}")).collect()
}
async fn resume(store: Arc<FixtureStore>) -> ThreadStoreResult<LiveThread> {
    LiveThread::resume(
        store,
        ThreadHistoryMode::Legacy,
        ResumeThreadParams {
            thread_id: ThreadId::from_string("00000000-0000-4000-8000-000000000002").unwrap(),
            rollout_path: None,
            history: None,
            include_archived: false,
            metadata: ThreadPersistenceMetadata {
                cwd: None,
                model_provider: "qualification-only".into(),
                memory_mode: ThreadMemoryMode::Disabled,
            },
        },
    )
    .await
}
fn record(
    name: &str,
    store: &FixtureStore,
    error: ThreadStoreError,
    calls: &[&str],
    expected_error: &str,
) -> Value {
    let state = store.0.lock().unwrap();
    assert_eq!(state.calls, calls, "complete actual call order: {name}");
    assert!(
        error.to_string().contains(expected_error),
        "{name}: {error}"
    );
    json!({"case":name,"calls":state.calls,"frames":state.frames,"error":error.to_string(),"assertions":2})
}
async fn qualify() -> Vec<Value> {
    let mut cases = vec![];
    let store = Arc::new(FixtureStore::new(Some("open_writer")));
    let error = resume(store.clone()).await.err().expect("resume must fail");
    cases.push(record(
        "resume_disconnected",
        &store,
        error,
        &["resume_thread"],
        "qualification-disconnected:open_writer",
    ));

    let store = Arc::new(FixtureStore::new(Some("read_after")));
    let error = resume(store.clone())
        .await
        .err()
        .expect("history must fail");
    cases.push(record(
        "resume_history_failure_discards",
        &store,
        error,
        &["resume_thread", "load_history", "discard_thread"],
        "qualification-disconnected:read_after",
    ));

    let store = Arc::new(FixtureStore::new(None));
    let live = resume(store.clone())
        .await
        .expect("empty bounded fixture resume");
    store.0.lock().unwrap().fail_on = Some("append_batch");
    let item = RolloutItem::InterAgentCommunicationMetadata {
        trigger_turn: false,
    };
    let error = live
        .append_items(&[item])
        .await
        .expect_err("append must fail");
    cases.push(record(
        "append_disconnected_no_metadata",
        &store,
        error,
        &["resume_thread", "load_history", "append_items"],
        "qualification-disconnected:append_batch",
    ));

    let store = Arc::new(FixtureStore::new(None));
    let live = resume(store.clone())
        .await
        .expect("empty bounded fixture resume");
    let error = live
        .persist(PersistContext::Standard)
        .await
        .expect_err("no fake durability");
    cases.push(record(
        "standard_persist_unsupported",
        &store,
        error,
        &[
            "resume_thread",
            "load_history",
            "persist_context:Standard",
            "persist_thread_requires_M04",
        ],
        "persist_thread_requires_M04",
    ));

    let store = Arc::new(FixtureStore::new(None));
    let live = resume(store.clone())
        .await
        .expect("empty bounded fixture resume");
    let error = live.flush().await.expect_err("no fake flush durability");
    cases.push(record(
        "flush_unsupported",
        &store,
        error,
        &["resume_thread", "load_history", "flush_thread"],
        "flush_thread",
    ));

    let store = Arc::new(FixtureStore::new(None));
    let live = resume(store.clone())
        .await
        .expect("empty bounded fixture resume");
    store.0.lock().unwrap().fail_on = Some("read_after");
    let error = live
        .load_history(false)
        .await
        .expect_err("history must fail");
    cases.push(record(
        "live_history_disconnected",
        &store,
        error,
        &["resume_thread", "load_history", "load_history"],
        "qualification-disconnected:read_after",
    ));
    cases
}
fn main() {
    let destination = std::env::args_os().nth(1).expect("receipt path required");
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let cases = rt.block_on(qualify());
    let report = json!({"status":"passed_limited_store_callsite_probe","P-02":"not_complete","G0":"not_claimed","cases":cases,"case_count":6,"assertions":12,"limits":["Legacy LiveThread only; no Core session/model loop", "003 FakeHost memory qualification only; no production durable store", "Local disconnected refusal is not OS isolation", "No real executor, model, network, account or credentials"]});
    let mut output = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(destination)
        .unwrap();
    serde_json::to_writer_pretty(&mut output, &report).unwrap();
    println!(
        "{}",
        json!({"status":report["status"],"case_count":6,"assertions":12})
    );
}
