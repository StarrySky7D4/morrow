//! Implements the real upstream `ThreadStore`, without a local-rollout fallback.
use std::any::Any;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use codex_protocol::{
    ThreadId,
    models::PermissionProfile,
    protocol::{
        AskForApproval, SessionContextWindow, SessionMeta, SessionMetaLine, ThreadHistoryMode,
        ThreadMemoryMode,
    },
};
use codex_rollout::{RolloutItem, is_persisted_rollout_item};
use codex_thread_store::*;
use morrow_agent_session_exec_v1_r2::{
    Action, Event, MAX_BODY_BYTES, Outcome, SessionInfo, SessionSnapshot,
};
use serde::{Deserialize, Serialize};

/// A dedicated original SDK admission. `close` must revoke that admission,
/// including retained clones; dropping a local handle alone is insufficient.
pub trait SessionWriter: Send + Sync {
    fn request(&self, action: Action) -> ThreadStoreResult<Outcome>;
    fn close(&self) -> ThreadStoreResult<()>;
}

/// Trusted native connection boundary. Implementations must preserve the
/// original HostRuntime/Connection and reviewed declaration/approval identities.
/// These methods are not guest inputs and cannot reconstruct an admission.
pub trait SessionControl: Send + Sync {
    fn request(&self, action: Action) -> ThreadStoreResult<Outcome>;
    fn writer(&self, session_id: &str) -> ThreadStoreResult<Arc<dyn SessionWriter>>;
    /// Close all writers, seal and permanently archive the SDK generation,
    /// retire related tool rows, then perform R2 generation/revision/digest CAS
    /// retirement. The UI archive flag is independent of this operation.
    fn retire(&self, session_id: &str) -> ThreadStoreResult<()>;
    fn now(&self) -> DateTime<Utc>;
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Creation {
    format: String,
    params: CreateThreadParams,
    summary: StoredThread,
    session_meta: SessionMetaLine,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", deny_unknown_fields)]
enum DurableEvent {
    Created(Box<Creation>),
    Items(Vec<RolloutItem>),
    Metadata(Box<ThreadMetadataPatch>),
    Archived(Option<DateTime<Utc>>),
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Checkpoint {
    format: String,
    tail: u64,
    summary: StoredThread,
}
struct LiveWriter {
    admission: Arc<dyn SessionWriter>,
    info: SessionInfo,
}
struct DurableThread {
    info: SessionInfo,
    creation: Creation,
    items: Vec<RolloutItem>,
}

/// Every append and metadata change commits synchronously. All persistence
/// contexts therefore meet Codex's flush/shutdown barrier contract. The adapter
/// never compacts rollout events: a compacted foreign history is rejected.
pub struct MorrowThreadStore {
    control: Arc<dyn SessionControl>,
    writers: Mutex<BTreeMap<String, LiveWriter>>,
    pending: Mutex<BTreeMap<String, ThreadMetadataPatch>>,
}
impl MorrowThreadStore {
    pub fn new(control: Arc<dyn SessionControl>) -> Self {
        Self {
            control,
            writers: Mutex::new(BTreeMap::new()),
            pending: Mutex::new(BTreeMap::new()),
        }
    }
    fn load(
        &self,
        thread_id: ThreadId,
        include_archived: bool,
    ) -> ThreadStoreResult<DurableThread> {
        let id = thread_id.to_string();
        let mut after = 0;
        let mut creation: Option<Creation> = None;
        let mut items = Vec::new();
        let info = loop {
            let outcome = self.control.request(Action::Snapshot {
                session_id: id.clone(),
                after,
                // A reply includes both checkpoint and event bodies. One
                // maximum-sized event always fits the 128 KiB frame bound;
                // sixteen individually legal bodies need not fit together.
                limit: 1,
            })?;
            if matches!(
                outcome,
                Outcome::Rejected(morrow_agent_session_exec_v1_r2::Error::NotFound)
            ) {
                return Err(ThreadStoreError::ThreadNotFound { thread_id });
            }
            let snapshot = snapshot(outcome)?;
            if snapshot.gap || snapshot.info.floor != 1 {
                return Err(conflict(
                    "rollout history was compacted outside this adapter",
                ));
            }
            if snapshot.events.is_empty() && after < snapshot.info.tail {
                return Err(invalid("empty nonterminal snapshot"));
            }
            for stored in snapshot.events {
                if stored.sequence
                    != after
                        .checked_add(1)
                        .ok_or_else(|| invalid("snapshot sequence overflow"))?
                {
                    return Err(invalid("noncontiguous snapshot sequence"));
                }
                after = stored.sequence;
                let event: DurableEvent =
                    serde_json::from_slice(&stored.event.body).map_err(internal)?;
                match event {
                    DurableEvent::Created(value) if creation.is_none() => {
                        if value.format != "morrow-codex-thread-r2"
                            || value.params.thread_id != thread_id
                            || value.summary.thread_id != thread_id
                        {
                            return Err(invalid("stored thread identity/format mismatch"));
                        }
                        items.push(RolloutItem::SessionMeta(value.session_meta.clone()));
                        creation = Some(*value);
                    }
                    DurableEvent::Created(_) => return Err(invalid("duplicate creation metadata")),
                    DurableEvent::Items(batch) => items.extend(batch),
                    DurableEvent::Metadata(patch) => apply_patch(
                        creation
                            .as_mut()
                            .ok_or_else(|| invalid("metadata precedes creation"))?,
                        *patch,
                    )?,
                    DurableEvent::Archived(at) => {
                        creation
                            .as_mut()
                            .ok_or_else(|| invalid("archive precedes creation"))?
                            .summary
                            .archived_at = at
                    }
                }
            }
            // The SDK returns a contiguous, correlated range. Advance by its
            // count, not by a caller-selected tail or a saturating cursor.
            if after == snapshot.info.tail {
                break snapshot.info;
            }
            if after > snapshot.info.tail {
                return Err(invalid("snapshot cursor exceeds tail"));
            }
        };
        let creation = creation.ok_or(ThreadStoreError::ThreadNotFound { thread_id })?;
        if creation.summary.archived_at.is_some() && !include_archived {
            return Err(ThreadStoreError::ThreadNotFound { thread_id });
        }
        Ok(DurableThread {
            info,
            creation,
            items,
        })
    }
    fn open(&self, thread_id: ThreadId, info: SessionInfo) -> ThreadStoreResult<()> {
        let id = thread_id.to_string();
        let mut writers = self.writers.lock().map_err(internal)?;
        if writers.contains_key(&id) {
            return Err(conflict("thread already has a live writer"));
        }
        let admission = self.control.writer(&id)?;
        let opened = match admission
            .request(Action::OpenWriter {
                session_id: id.clone(),
                expected_epoch: info.epoch,
            })
            .and_then(session)
        {
            Ok(info) => info,
            Err(error) => {
                admission.close()?;
                return Err(error);
            }
        };
        writers.insert(
            id,
            LiveWriter {
                admission,
                info: opened,
            },
        );
        Ok(())
    }
    fn append_event(&self, thread_id: ThreadId, event: DurableEvent) -> ThreadStoreResult<()> {
        let body = serde_json::to_vec(&event).map_err(internal)?;
        if body.len() > MAX_BODY_BYTES {
            return Err(invalid(
                "typed rollout event exceeds the reviewed profile bound",
            ));
        }
        let id = thread_id.to_string();
        let mut writers = self.writers.lock().map_err(internal)?;
        let writer = writers
            .get_mut(&id)
            .ok_or_else(|| conflict("thread has no live writer"))?;
        let event_id = format!("codex-{}-{}", writer.info.epoch, writer.info.tail + 1);
        let next = writer.admission.request(Action::Append {
            session_id: id,
            epoch: writer.info.epoch,
            expected_tail: writer.info.tail,
            events: vec![Event { event_id, body }],
        })?;
        writer.info = session(next)?;
        Ok(())
    }
    fn checkpoint(&self, thread_id: ThreadId) -> ThreadStoreResult<()> {
        let durable = self.load(thread_id, true)?;
        let id = thread_id.to_string();
        let mut writers = self.writers.lock().map_err(internal)?;
        let writer = writers
            .get_mut(&id)
            .ok_or_else(|| conflict("thread has no live writer"))?;
        if writer.info.tail != durable.info.tail {
            return Err(conflict("writer tail changed before durability barrier"));
        }
        if writer.info.checkpoint_sealed && writer.info.checkpoint_tail == writer.info.tail {
            return Ok(());
        }
        let state = serde_json::to_vec(&Checkpoint {
            format: "morrow-codex-checkpoint-r2".into(),
            tail: durable.info.tail,
            summary: durable.creation.summary,
        })
        .map_err(internal)?;
        let next = writer.admission.request(Action::Checkpoint {
            session_id: id,
            epoch: writer.info.epoch,
            expected_tail: writer.info.tail,
            state,
        })?;
        writer.info = session(next)?;
        Ok(())
    }
    fn close(&self, thread_id: ThreadId, flush: bool) -> ThreadStoreResult<()> {
        // Revocation is also required when the checkpoint/transport fails.
        // All rollout appends are already durable; no in-memory write is lost.
        let flushed = if flush {
            self.checkpoint(thread_id)
        } else {
            Ok(())
        };
        let id = thread_id.to_string();
        let mut writers = self.writers.lock().map_err(internal)?;
        if let Some(writer) = writers.get(&id) {
            writer.admission.close()?;
        }
        writers.remove(&id);
        flushed
    }
    fn mutate(
        &self,
        thread_id: ThreadId,
        include_archived: bool,
        event: DurableEvent,
    ) -> ThreadStoreResult<StoredThread> {
        let durable = self.load(thread_id, include_archived)?;
        if let DurableEvent::Metadata(patch) = &event {
            apply_patch(&mut durable.creation.clone(), (**patch).clone())?;
        }
        let temporary = !self
            .writers
            .lock()
            .map_err(internal)?
            .contains_key(&thread_id.to_string());
        if temporary {
            self.open(thread_id, durable.info)?;
        }
        let result = self.append_event(thread_id, event);
        let close = if temporary {
            self.close(thread_id, false)
        } else {
            Ok(())
        };
        result?;
        close?;
        Ok(self.load(thread_id, true)?.creation.summary)
    }
}
impl ThreadStore for MorrowThreadStore {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn create_thread(&self, params: CreateThreadParams) -> ThreadStoreFuture<'_, ()> {
        Box::pin(async move {
            if params.history_mode != ThreadHistoryMode::Legacy || params.history_base.is_some() {
                return Err(unsupported("paginated_threads"));
            }
            let thread_id = params.thread_id;
            let now = self.control.now();
            let summary = StoredThread {
                originator: Some(params.originator.clone()),
                thread_id,
                extra_config: params.extra_config.clone(),
                rollout_path: None,
                forked_from_id: params.forked_from_id,
                parent_thread_id: params.parent_thread_id,
                preview: String::new(),
                name: None,
                model_provider: params.metadata.model_provider.clone(),
                model: None,
                reasoning_effort: None,
                created_at: now,
                updated_at: now,
                recency_at: now,
                archived_at: None,
                section: None,
                section_position: None,
                section_entered_at: None,
                project_id: None,
                daybreak_enabled: None,
                cwd: params.metadata.cwd.clone().unwrap_or_default(),
                cli_version: String::new(),
                source: params.source.clone(),
                history_mode: params.history_mode,
                thread_source: params.thread_source.clone(),
                agent_nickname: None,
                agent_role: None,
                agent_path: None,
                git_info: None,
                approval_mode: AskForApproval::default(),
                permission_profile: PermissionProfile::default(),
                token_usage: None,
                first_user_message: None,
                history: None,
            };
            let mut creation = Creation {
                format: "morrow-codex-thread-r2".into(),
                session_meta: SessionMetaLine {
                    meta: SessionMeta {
                        creator_user_id: params.creator_user_id.clone(),
                        creator_account_id: params.creator_account_id.clone(),
                        session_id: params.session_id,
                        id: thread_id,
                        forked_from_id: params.forked_from_id,
                        parent_thread_id: params.parent_thread_id,
                        timestamp: now.to_rfc3339(),
                        cwd: summary.cwd.clone(),
                        runtime_workspace_roots: params
                            .runtime_workspace_roots
                            .as_ref()
                            .map(|roots| roots.iter().map(|root| root.to_path_buf()).collect()),
                        originator: params.originator.clone(),
                        cli_version: summary.cli_version.clone(),
                        source: params.source.clone(),
                        thread_source: params.thread_source.clone(),
                        model_provider: Some(params.metadata.model_provider.clone()),
                        base_instructions: Some(params.base_instructions.clone()),
                        dynamic_tools: (!params.dynamic_tools.is_empty())
                            .then(|| params.dynamic_tools.clone()),
                        selected_capability_roots: params.selected_capability_roots.clone(),
                        memory_mode: (params.metadata.memory_mode == ThreadMemoryMode::Disabled)
                            .then(|| "disabled".into()),
                        history_mode: params.history_mode,
                        history_base: params.history_base,
                        subagent_history_start_ordinal: params.subagent_history_start_ordinal,
                        multi_agent_version: params.multi_agent_version,
                        context_window: Some(SessionContextWindow::new(
                            params.initial_window_id.clone(),
                        )),
                        ..Default::default()
                    },
                    git: None,
                },
                params,
                summary,
            };
            if let Some(patch) = self
                .pending
                .lock()
                .map_err(internal)?
                .get(&thread_id.to_string())
                .cloned()
            {
                apply_patch(&mut creation, patch)?;
            }
            let body =
                serde_json::to_vec(&DurableEvent::Created(Box::new(creation))).map_err(internal)?;
            if body.len() > MAX_BODY_BYTES {
                return Err(invalid(
                    "creation metadata exceeds the reviewed profile bound",
                ));
            }
            let info = session(self.control.request(Action::Create {
                session_id: thread_id.to_string(),
                parent: None,
                parent_tail: 0,
            })?)?;
            self.open(thread_id, info)?;
            let event: DurableEvent = serde_json::from_slice(&body).map_err(internal)?;
            let result = self.append_event(thread_id, event);
            if result.is_err() {
                self.close(thread_id, false)?;
            }
            result
        })
    }
    fn stage_pending_thread_metadata(
        &self,
        thread_id: ThreadId,
        patch: ThreadMetadataPatch,
    ) -> ThreadStoreFuture<'_, ()> {
        Box::pin(async move {
            if patch.is_empty() {
                return Err(invalid("pending thread metadata cannot be empty"));
            }
            let mut pending = self.pending.lock().map_err(internal)?;
            if pending.contains_key(&thread_id.to_string()) {
                return Err(invalid("pending thread metadata already exists"));
            }
            pending.insert(thread_id.to_string(), patch);
            Ok(())
        })
    }
    fn read_pending_thread_metadata(
        &self,
        thread_id: ThreadId,
    ) -> ThreadStoreFuture<'_, Option<ThreadMetadataPatch>> {
        Box::pin(async move {
            Ok(self
                .pending
                .lock()
                .map_err(internal)?
                .get(&thread_id.to_string())
                .cloned())
        })
    }
    fn remove_pending_thread_metadata(&self, thread_id: ThreadId) -> ThreadStoreFuture<'_, ()> {
        Box::pin(async move {
            self.pending
                .lock()
                .map_err(internal)?
                .remove(&thread_id.to_string());
            Ok(())
        })
    }
    fn resume_thread(&self, params: ResumeThreadParams) -> ThreadStoreFuture<'_, ()> {
        Box::pin(async move {
            if params.rollout_path.is_some() {
                return Err(unsupported("rollout_path"));
            }
            let durable = self.load(params.thread_id, params.include_archived)?;
            self.open(params.thread_id, durable.info)?;
            let result = self.append_event(
                params.thread_id,
                DurableEvent::Metadata(Box::new(ThreadMetadataPatch {
                    cwd: params.metadata.cwd,
                    model_provider: Some(params.metadata.model_provider),
                    memory_mode: Some(params.metadata.memory_mode),
                    ..Default::default()
                })),
            );
            if result.is_err() {
                self.close(params.thread_id, false)?;
            }
            result
        })
    }
    fn append_items(&self, params: AppendThreadItemsParams) -> ThreadStoreFuture<'_, ()> {
        Box::pin(async move {
            let items = params
                .items
                .into_iter()
                .filter(|item| is_persisted_rollout_item(item, ThreadHistoryMode::Legacy))
                .collect::<Vec<_>>();
            if items.is_empty() {
                return Ok(());
            }
            self.append_event(params.thread_id, DurableEvent::Items(items))
        })
    }
    fn persist_thread(
        &self,
        thread_id: ThreadId,
        _context: PersistContext,
    ) -> ThreadStoreFuture<'_, ()> {
        Box::pin(async move { self.checkpoint(thread_id) })
    }
    fn flush_thread(&self, thread_id: ThreadId) -> ThreadStoreFuture<'_, ()> {
        Box::pin(async move { self.checkpoint(thread_id) })
    }
    fn shutdown_thread(&self, thread_id: ThreadId) -> ThreadStoreFuture<'_, ()> {
        Box::pin(async move { self.close(thread_id, true) })
    }
    fn discard_thread(&self, thread_id: ThreadId) -> ThreadStoreFuture<'_, ()> {
        Box::pin(async move { self.close(thread_id, false) })
    }
    fn load_history(
        &self,
        params: LoadThreadHistoryParams,
    ) -> ThreadStoreFuture<'_, StoredThreadHistory> {
        Box::pin(async move {
            Ok(StoredThreadHistory {
                thread_id: params.thread_id,
                items: self.load(params.thread_id, params.include_archived)?.items,
            })
        })
    }
    fn load_latest_model_context(
        &self,
        params: LoadThreadHistoryParams,
    ) -> ThreadStoreFuture<'_, StoredModelContext> {
        Box::pin(async move {
            Ok(StoredModelContext {
                thread_id: params.thread_id,
                items: self.load(params.thread_id, params.include_archived)?.items,
            })
        })
    }
    fn read_thread(&self, params: ReadThreadParams) -> ThreadStoreFuture<'_, StoredThread> {
        Box::pin(async move {
            let durable = self.load(params.thread_id, params.include_archived)?;
            let mut summary = durable.creation.summary;
            if params.include_history {
                summary.history = Some(StoredThreadHistory {
                    thread_id: params.thread_id,
                    items: durable.items,
                });
            }
            Ok(summary)
        })
    }
    fn read_thread_by_rollout_path(
        &self,
        _params: ReadThreadByRolloutPathParams,
    ) -> ThreadStoreFuture<'_, StoredThread> {
        Box::pin(async { Err(unsupported("read_thread_by_rollout_path")) })
    }
    fn list_threads(&self, params: ListThreadsParams) -> ThreadStoreFuture<'_, ThreadPage> {
        Box::pin(async move {
            if params.page_size == 0 || params.page_size > 16 {
                return Err(invalid("list page size exceeds the reviewed scope"));
            }
            if params.relation_filter.is_some()
                || params.section.is_some()
                || params.project_id.is_some()
                || params.sort_key == ThreadSortKey::SectionPosition
            {
                return Err(unsupported("list_threads_filters"));
            }
            let infos = match self.control.request(Action::List)? {
                Outcome::Sessions(infos) => infos,
                _ => return Err(internal("unexpected SDK list reply")),
            };
            let mut items = Vec::new();
            for info in infos {
                let thread_id = ThreadId::from_string(&info.session_id).map_err(internal)?;
                let summary = match self.load(thread_id, true) {
                    Ok(thread) => thread.creation.summary,
                    Err(ThreadStoreError::ThreadNotFound { .. }) => continue,
                    Err(error) => return Err(error),
                };
                if summary.archived_at.is_some() != params.archived {
                    continue;
                }
                if !params.allowed_sources.is_empty()
                    && !params.allowed_sources.contains(&summary.source)
                {
                    continue;
                }
                if params
                    .model_providers
                    .as_ref()
                    .is_some_and(|p| !p.is_empty() && !p.contains(&summary.model_provider))
                {
                    continue;
                }
                if params
                    .cwd_filters
                    .as_ref()
                    .is_some_and(|p| !p.contains(&summary.cwd))
                {
                    continue;
                }
                if params.search_term.as_ref().is_some_and(|p| {
                    !summary.preview.contains(p)
                        && !summary.name.as_deref().unwrap_or_default().contains(p)
                }) {
                    continue;
                }
                items.push(summary);
            }
            items.sort_by_key(|item| {
                (
                    match params.sort_key {
                        ThreadSortKey::CreatedAt => item.created_at,
                        ThreadSortKey::UpdatedAt => item.updated_at,
                        ThreadSortKey::RecencyAt => item.recency_at,
                        ThreadSortKey::SectionPosition => unreachable!(),
                    },
                    item.thread_id.to_string(),
                )
            });
            if params.sort_direction == SortDirection::Desc {
                items.reverse();
            }
            let cursor_prefix = format!(
                "morrow-codex-r2:{:?}:{:?}:",
                params.sort_key, params.sort_direction
            );
            if let Some(cursor) = params.cursor {
                let anchor = cursor
                    .strip_prefix(&cursor_prefix)
                    .ok_or_else(|| invalid("list cursor belongs to another order"))?;
                let position = items
                    .iter()
                    .position(|item| item.thread_id.to_string() == anchor)
                    .ok_or_else(|| conflict("list cursor anchor changed"))?;
                items.drain(..=position);
            }
            let next_cursor = (items.len() > params.page_size)
                .then(|| format!("{cursor_prefix}{}", items[params.page_size - 1].thread_id));
            items.truncate(params.page_size);
            Ok(ThreadPage { items, next_cursor })
        })
    }
    fn update_thread_metadata(
        &self,
        params: UpdateThreadMetadataParams,
    ) -> ThreadStoreFuture<'_, Option<StoredThread>> {
        Box::pin(async move {
            let result = self.mutate(
                params.thread_id,
                params.include_archived,
                DurableEvent::Metadata(Box::new(params.patch)),
            )?;
            self.pending
                .lock()
                .map_err(internal)?
                .remove(&params.thread_id.to_string());
            Ok(Some(result))
        })
    }
    fn archive_thread(&self, params: ArchiveThreadParams) -> ThreadStoreFuture<'_, ()> {
        Box::pin(async move {
            self.mutate(
                params.thread_id,
                true,
                DurableEvent::Archived(Some(self.control.now())),
            )?;
            Ok(())
        })
    }
    fn unarchive_thread(&self, params: ArchiveThreadParams) -> ThreadStoreFuture<'_, StoredThread> {
        Box::pin(async move { self.mutate(params.thread_id, true, DurableEvent::Archived(None)) })
    }
    fn delete_thread(&self, params: DeleteThreadParams) -> ThreadStoreFuture<'_, ()> {
        Box::pin(async move {
            self.close(params.thread_id, false)?;
            self.control.retire(&params.thread_id.to_string())?;
            self.pending
                .lock()
                .map_err(internal)?
                .remove(&params.thread_id.to_string());
            Ok(())
        })
    }
}
fn apply_patch(creation: &mut Creation, patch: ThreadMetadataPatch) -> ThreadStoreResult<()> {
    if patch.rollout_path.is_some() {
        return Err(unsupported("rollout_path"));
    }
    if let Some(value) = patch.creator_user_id.clone() {
        creation.params.creator_user_id.get_or_insert(value);
    }
    if let Some(value) = patch.creator_account_id.clone() {
        creation.params.creator_account_id.get_or_insert(value);
    }
    if let Some(value) = patch.memory_mode {
        creation.params.metadata.memory_mode = value;
    }
    let mut summary = serde_json::to_value(&creation.summary).map_err(internal)?;
    let object = summary
        .as_object_mut()
        .ok_or_else(|| internal("summary is not an object"))?;
    let patch_value = serde_json::to_value(&patch).map_err(internal)?;
    for (key, value) in patch_value
        .as_object()
        .ok_or_else(|| internal("patch is not an object"))?
    {
        if value.is_null() {
            continue;
        }
        match key.as_str() {
            "creator_user_id" | "creator_account_id" | "memory_mode" => {}
            "title" => {
                if patch.name.is_none() {
                    object.insert("name".into(), value.clone());
                }
            }
            "originator" => {
                if creation.summary.originator.is_none() {
                    object.insert(key.clone(), value.clone());
                }
            }
            "advance_recency_at" => {
                let candidate: DateTime<Utc> =
                    serde_json::from_value(value.clone()).map_err(internal)?;
                if candidate > creation.summary.recency_at {
                    object.insert("recency_at".into(), value.clone());
                }
            }
            "git_info" => {
                let git = object.entry("git_info").or_insert(serde_json::Value::Null);
                if git.is_null() {
                    *git = serde_json::json!({"sha":null,"branch":null,"origin_url":null});
                }
                if let (Some(dst), Some(src)) = (git.as_object_mut(), value.as_object()) {
                    for (k, v) in src {
                        dst.insert(k.clone(), v.clone());
                    }
                }
            }
            _ if object.contains_key(key) => {
                object.insert(key.clone(), value.clone());
            }
            _ => return Err(unsupported("thread_metadata_field")),
        }
    }
    // Preserve explicit clear requests; serde omission and JSON null differ.
    if patch.name == Some(None) {
        object.insert("name".into(), serde_json::Value::Null);
    }
    if patch.reasoning_effort == Some(None) {
        object.insert("reasoning_effort".into(), serde_json::Value::Null);
    }
    if patch.thread_source == Some(None) {
        object.insert("thread_source".into(), serde_json::Value::Null);
    }
    if patch.agent_nickname == Some(None) {
        object.insert("agent_nickname".into(), serde_json::Value::Null);
    }
    if patch.agent_role == Some(None) {
        object.insert("agent_role".into(), serde_json::Value::Null);
    }
    if patch.agent_path == Some(None) {
        object.insert("agent_path".into(), serde_json::Value::Null);
    }
    if patch.project_id == Some(None) {
        object.insert("project_id".into(), serde_json::Value::Null);
    }
    creation.summary = serde_json::from_value(summary).map_err(internal)?;
    let checkpoint = serde_json::to_vec(&Checkpoint {
        format: "morrow-codex-checkpoint-r2".into(),
        tail: u64::MAX,
        summary: creation.summary.clone(),
    })
    .map_err(internal)?;
    // Leave space for a future logical archive timestamp and tail spelling.
    if checkpoint.len() > MAX_BODY_BYTES - 128 {
        return Err(invalid(
            "metadata would exceed the guaranteed checkpoint bound",
        ));
    }
    Ok(())
}
fn session(outcome: Outcome) -> ThreadStoreResult<SessionInfo> {
    match outcome {
        Outcome::Session(info) => Ok(info),
        Outcome::Rejected(error) => Err(internal(error)),
        _ => Err(internal("unexpected SDK session reply")),
    }
}
fn snapshot(outcome: Outcome) -> ThreadStoreResult<SessionSnapshot> {
    match outcome {
        Outcome::Snapshot(snapshot) => Ok(snapshot),
        Outcome::Rejected(error) => Err(internal(error)),
        _ => Err(internal("unexpected SDK snapshot reply")),
    }
}
fn invalid(message: impl ToString) -> ThreadStoreError {
    ThreadStoreError::InvalidRequest {
        message: message.to_string(),
    }
}
fn conflict(message: impl ToString) -> ThreadStoreError {
    ThreadStoreError::Conflict {
        message: message.to_string(),
    }
}
fn internal(message: impl ToString) -> ThreadStoreError {
    ThreadStoreError::Internal {
        message: message.to_string(),
    }
}
fn unsupported(operation: &'static str) -> ThreadStoreError {
    ThreadStoreError::Unsupported { operation }
}
