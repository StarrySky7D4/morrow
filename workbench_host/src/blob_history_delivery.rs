//! One explicitly selected history disclosure, with its original receipt and owner.
//! Preview/read/discovery never dispatch or reclaim a background owner. This is
//! separate from upload execution and never completes original upload metadata.
use crate::{
    Result, Workbench, WorkbenchState,
    product_gate::{ProductGate, StopRegistration},
};
use morrow_blob_upload_history_v1 as sdk;
use morrow_core::{
    dispatch::HostBinding,
    store::BlobStageStoreBinding,
    task::{Invocation, PluginFailure, Transform, TransformOutput},
};
use morrow_plugin_runtime::{Report, manager::ManagedInstance, package::TaskReport};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeliveryKey([u8; 32]);
impl DeliveryKey {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let bytes = bytes.try_into()?;
        if bytes == [0; 32] {
            return Err("invalid history delivery key".into());
        }
        Ok(Self(bytes))
    }
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}
#[derive(Clone, Debug)]
pub struct DeliveryRequest {
    /// Client-generated discovery correlation, consumed even after a lost reply.
    pub request_id: [u8; 32],
    pub submission: [u8; 32],
    pub package_id: String,
    pub package_digest: [u8; 32],
    pub registry_revision: u64,
    pub handler: String,
    pub output_type: String,
    pub lifetime: Duration,
}
#[derive(Clone, Debug)]
pub struct DeliveryPreview {
    pub key: DeliveryKey,
    pub request: DeliveryRequest,
    pub task_id: String,
    pub packet: Vec<u8>,
    pub packet_sha256: [u8; 32],
}
#[derive(Clone, Copy, Debug)]
pub struct DeliveryApproval {
    pub key: DeliveryKey,
    pub packet_sha256: [u8; 32],
    pub package_digest: [u8; 32],
    pub registry_revision: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeliveryError {
    Cancelled,
    Expired,
    StaleOwner,
    StaleSelection,
    HistoryChanged,
    GateClosed,
    Preparation,
    Execution,
    PluginFailure,
    Protocol,
    Unwind,
    Close,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseState {
    NotCreated,
    Pending,
    Closed,
    Failed,
    Unknown,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CloseFailure {
    /// Exact original trusted close refusal, separate from the execution error.
    Core(morrow_core::Error),
    Unwind,
}
#[derive(Clone, Debug)]
pub struct DeliveryReceipt {
    pub key: DeliveryKey,
    pub request_id: [u8; 32],
    pub consumed: bool,
    pub dispatched: bool,
    pub finished: bool,
    pub acknowledged: bool,
    pub close: CloseState,
    pub close_failure: Option<CloseFailure>,
    /// Original dispatch error is never overwritten by close/delivery failure.
    pub error: Option<DeliveryError>,
    pub delivery_error: Option<DeliveryError>,
    pub execution: Option<Report>,
    pub failure: Option<PluginFailure>,
    pub output: Option<TransformOutput>,
}
struct Attempt {
    preview: DeliveryPreview,
    nonce: [u8; 32],
    owner: [u8; 32],
    host: HostBinding,
    store: BlobStageStoreBinding,
    gate: ProductGate,
    deadline: Instant,
    input: Invocation,
    consumed: bool,
    dispatched: bool,
    finished: bool,
    acknowledged: bool,
    read: bool,
    close: CloseState,
    close_failure: Option<CloseFailure>,
    error: Option<DeliveryError>,
    report: Option<TaskReport>,
    instance: Option<ManagedInstance>,
    stop: Option<StopRegistration>,
}
impl Attempt {
    fn owner_matches(&self, w: &Workbench) -> bool {
        w.state.history_delivery_owner().is_ok_and(|s| {
            s.query_owner == self.owner
                && s.host.binding() == self.host
                && s.host
                    .store_local()
                    .validate_blob_stage_store_binding(&self.store)
                    .is_ok()
        }) && self.gate.same_gate(&w.state.product_gate)
    }
    fn veto(&self, w: &Workbench, admission: bool) -> Option<DeliveryError> {
        if !self.owner_matches(w) {
            return Some(DeliveryError::StaleOwner);
        }
        if self.gate.closed() {
            return Some(DeliveryError::GateClosed);
        }
        if admission && Instant::now() >= self.deadline {
            return Some(DeliveryError::Expired);
        }
        if w.state
            .history_delivery_owner()
            .and_then(|s| recipient(s, &self.preview.request, &self.input))
            .is_err()
        {
            return Some(DeliveryError::StaleSelection);
        }
        None
    }
    fn receipt(&self, delivery_error: Option<DeliveryError>) -> DeliveryReceipt {
        DeliveryReceipt {
            key: self.preview.key,
            request_id: self.preview.request.request_id,
            consumed: self.consumed,
            dispatched: self.dispatched,
            finished: self.finished,
            acknowledged: self.acknowledged,
            close: self.close,
            close_failure: self.close_failure.clone(),
            error: self.error,
            delivery_error,
            execution: self.report.as_ref().map(|r| r.execution.clone()),
            failure: if delivery_error.is_none() {
                self.report.as_ref().and_then(|r| r.failure.clone())
            } else {
                None
            },
            output: if delivery_error.is_none()
                && self.error.is_none()
                && self.close == CloseState::Closed
            {
                self.report.as_ref().and_then(|r| r.output.clone())
            } else {
                None
            },
        }
    }
    fn stop(&self) {
        if let Some(i) = &self.instance {
            i.request_stop();
        }
    }
}
#[derive(Default)]
pub(crate) struct Deliveries {
    attempt: Option<Attempt>,
    seen: BTreeSet<[u8; 32]>,
}
impl Deliveries {
    fn get(&self, key: DeliveryKey) -> Result<&Attempt> {
        self.attempt
            .as_ref()
            .filter(|a| a.preview.key == key)
            .ok_or_else(|| "stale history delivery".into())
    }
    fn get_mut(&mut self, key: DeliveryKey) -> Result<&mut Attempt> {
        self.attempt
            .as_mut()
            .filter(|a| a.preview.key == key)
            .ok_or_else(|| "stale history delivery".into())
    }
    pub(crate) fn request_stop(&self) {
        if let Some(a) = &self.attempt {
            a.stop();
        }
    }
    pub(crate) fn finish(&mut self) -> Result<()> {
        if let Some(a) = &mut self.attempt {
            a.stop();
            if matches!(
                a.close,
                CloseState::Pending | CloseState::Failed | CloseState::Unknown
            ) {
                return Err(
                    "original history delivery cleanup unresolved; no automatic retry".into(),
                );
            }
            if !a.consumed {
                a.consumed = true;
                a.finished = true;
                a.error = Some(DeliveryError::Cancelled);
            }
        }
        Ok(())
    }
}
fn fresh() -> Result<[u8; 32]> {
    let mut value = [0; 32];
    getrandom::fill(&mut value).map_err(|_| "history delivery randomness unavailable")?;
    if value == [0; 32] {
        return Err("empty history delivery identity".into());
    }
    Ok(value)
}
fn recipient(s: &WorkbenchState, r: &DeliveryRequest, input: &Invocation) -> Result<()> {
    let p = s.external_package(&r.package_id, &r.package_digest, r.registry_revision)?;
    if p.io_declaration().is_some()
        || p.channel_declaration().is_some()
        || p.blob_upload_declaration().is_some()
        || !p.manifest().dependencies.is_empty()
    {
        return Err("history disclosure requires an independent pure transform".into());
    }
    let h = p.transform_handler(input.transform().ok_or("history transform absent")?)?;
    if h.max_output_bytes > sdk::MAX_PACKET_BYTES as u32 {
        return Err("history recipient output exceeds the retained reply bound".into());
    }
    s.manager
        .as_ref()
        .ok_or("plugin manager unavailable")?
        .activation_plan(&r.package_id, &[], r.registry_revision)?;
    Ok(())
}
impl Workbench {
    /// Freeze one exact public packet and recipient; no guest is started here.
    pub fn prepare_blob_history_delivery(
        &mut self,
        request: DeliveryRequest,
    ) -> Result<DeliveryPreview> {
        self.state.require_writable()?;
        self.state.require_history_idle()?;
        if request.request_id == [0; 32]
            || request.submission == [0; 32]
            || request.lifetime.is_zero()
            || request.lifetime > Duration::from_secs(120)
        {
            return Err("invalid history delivery request".into());
        }
        if self.history_deliveries.seen.contains(&request.request_id)
            || self.history_deliveries.seen.len() >= 1024
            || self
                .history_deliveries
                .attempt
                .as_ref()
                .is_some_and(|a| !a.acknowledged)
        {
            return Err("history delivery already retained; discover the original receipt".into());
        }
        let nonce = fresh()?;
        let packet = self.read_blob_upload_history_packet(&request.submission, nonce)?;
        let task_id = sdk::task_id(request.submission, nonce)?;
        let input = Invocation::new_transform(
            &task_id,
            Transform {
                handler: request.handler.clone(),
                input_type: sdk::INPUT_TYPE.into(),
                output_type: request.output_type.clone(),
                input: packet.clone(),
            },
        )?;
        let owner = self.state.local()?;
        recipient(owner, &request, &input)?;
        let preview = DeliveryPreview {
            key: DeliveryKey(fresh()?),
            packet_sha256: Sha256::digest(&packet).into(),
            packet,
            task_id,
            request,
        };
        let attempt = Attempt {
            nonce,
            owner: owner.query_owner,
            host: owner.host.binding(),
            store: owner.host.store_local().blob_stage_store_binding(),
            gate: self.state.product_gate.clone(),
            deadline: Instant::now()
                .checked_add(preview.request.lifetime)
                .ok_or("history deadline overflow")?,
            preview: preview.clone(),
            input,
            consumed: false,
            dispatched: false,
            finished: false,
            acknowledged: false,
            read: false,
            close: CloseState::NotCreated,
            close_failure: None,
            error: None,
            report: None,
            instance: None,
            stop: None,
        };
        attempt.gate.check()?;
        self.history_deliveries
            .seen
            .insert(preview.request.request_id);
        self.history_deliveries.attempt = Some(attempt);
        Ok(preview)
    }
    /// Lost preparation/approval replies discover this original attempt only.
    pub fn discover_blob_history_delivery(
        &self,
        request_id: &[u8; 32],
    ) -> Result<Option<DeliveryReceipt>> {
        if *request_id == [0; 32] {
            return Err("invalid history request identity".into());
        }
        let Some(a) = self
            .history_deliveries
            .attempt
            .as_ref()
            .filter(|a| &a.preview.request.request_id == request_id)
        else {
            return Ok(None);
        };
        Ok(Some(a.receipt(a.veto(self, false))))
    }
    pub fn preview_blob_history_delivery(&self, key: DeliveryKey) -> Result<DeliveryPreview> {
        let a = self.history_deliveries.get(key)?;
        if a.consumed || a.veto(self, true).is_some() {
            return Err("history preview no longer valid".into());
        }
        Ok(a.preview.clone())
    }
    /// Consumes the shown approval before any connection or guest invocation.
    /// A lost response must use read/discover, never another approval call.
    pub fn approve_blob_history_delivery(
        &mut self,
        approval: DeliveryApproval,
    ) -> Result<DeliveryReceipt> {
        let a = self.history_deliveries.get(approval.key)?;
        if a.consumed
            || approval.packet_sha256 != a.preview.packet_sha256
            || approval.package_digest != a.preview.request.package_digest
            || approval.registry_revision != a.preview.request.registry_revision
        {
            return Err("history approval is stale or already consumed".into());
        }
        // Validly addressed decisions are consumed even when a final fence refuses.
        self.history_deliveries.get_mut(approval.key)?.consumed = true;
        let a = self.history_deliveries.get(approval.key)?;
        let mut refusal = a.veto(self, true);
        if refusal.is_none()
            && (self.state.require_writable().is_err()
                || self.state.require_history_idle().is_err())
        {
            refusal = Some(DeliveryError::StaleOwner);
        }
        if refusal.is_none()
            && self
                .read_blob_upload_history_packet(&a.preview.request.submission, a.nonce)
                .map_or(true, |bytes| bytes != a.preview.packet)
        {
            refusal = Some(DeliveryError::HistoryChanged);
        }
        if let Some(error) = refusal {
            let a = self.history_deliveries.get_mut(approval.key)?;
            a.error = Some(error);
            a.finished = true;
            return Ok(a.receipt(None));
        }
        // This method cannot create a host connection: the runtime API accepts
        // only immutable Manager state. Refusal therefore carries NotCreated,
        // without guessing from a missing instance/table entry after connect.
        let prepared = catch_unwind(AssertUnwindSafe(|| -> Result<()> {
            let a = self.history_deliveries.get(approval.key)?;
            self.state
                .local()?
                .manager
                .as_ref()
                .ok_or("plugin manager unavailable")?
                .validate_preparation(
                    &a.preview.request.package_id,
                    a.preview.request.package_digest,
                    a.preview.request.registry_revision,
                )?;
            Ok(())
        }));
        if !matches!(prepared, Ok(Ok(()))) {
            let a = self.history_deliveries.get_mut(approval.key)?;
            a.error = Some(if prepared.is_err() {
                DeliveryError::Unwind
            } else {
                DeliveryError::Preparation
            });
            a.finished = true;
            return self.read_blob_history_delivery(approval.key);
        }
        // Preparation may take time. The approval deadline and original gate
        // still apply immediately before entering the connection factory.
        if let Some(error) = self.history_deliveries.get(approval.key)?.veto(self, true) {
            let a = self.history_deliveries.get_mut(approval.key)?;
            a.error = Some(error);
            a.finished = true;
            return self.read_blob_history_delivery(approval.key);
        }
        // This latch is set before admission and cannot be cleared by storage maintenance.
        self.state.history_delivery_debt = true;
        {
            let a = self.history_deliveries.get_mut(approval.key)?;
            a.close = CloseState::Unknown;
            let owner = self.state.history_delivery_owner_mut()?;
            let outcome = catch_unwind(AssertUnwindSafe(|| {
                // Retain the exact ManagedInstance directly. Pool removal is not close proof.
                let instance = owner
                    .manager
                    .as_mut()
                    .ok_or(DeliveryError::Preparation)?
                    .connect(&a.preview.request.package_id, &mut owner.host)
                    .map_err(|_| DeliveryError::Preparation)?;
                a.instance = Some(instance);
                a.close = CloseState::Pending;
                let stop = a.instance.as_ref().unwrap().stop_handle();
                a.stop = Some(
                    a.gate
                        .register_scoped(Arc::new(move || stop.request_stop()))
                        .map_err(|_| DeliveryError::GateClosed)?,
                );
                a.gate.check().map_err(|_| DeliveryError::GateClosed)?;
                a.dispatched = true;
                let start = owner.start;
                let report =
                    a.instance
                        .as_ref()
                        .unwrap()
                        .run_task(&mut owner.host, &a.input, || crate::now(start));
                let error = if report.execution.outcome != Ok(0) {
                    Some(DeliveryError::Execution)
                } else if report.response.is_some() || report.execution.host_calls != 0 {
                    Some(DeliveryError::Protocol)
                } else if report.failure.is_some() {
                    Some(DeliveryError::PluginFailure)
                } else if report.output.is_none() {
                    Some(DeliveryError::Protocol)
                } else {
                    None
                };
                a.report = Some(report);
                error.map_or(Ok(()), Err)
            }));
            a.error = match outcome {
                Ok(Ok(())) => None,
                Ok(Err(e)) => Some(e),
                Err(_) => Some(DeliveryError::Unwind),
            };
            if let Some(instance) = &a.instance {
                let closed = catch_unwind(AssertUnwindSafe(|| {
                    // Dedicated boundary fault: the original stop signal is
                    // not evidence of close. No OS close/panic is simulated.
                    #[cfg(feature = "fault-injection")]
                    if std::env::var("MORROW_HISTORY_DELIVERY_FAULT").as_deref()
                        == Ok("original-close-unwind")
                    {
                        instance.request_stop();
                        panic!("qualification: original close outcome unavailable");
                    }
                    // Dedicated negative-control process only. Exercise a real
                    // second-close refusal; disappearance must never count as proof.
                    #[cfg(feature = "fault-injection")]
                    if std::env::var("MORROW_HISTORY_DELIVERY_FAULT").as_deref()
                        == Ok("retired-original-close")
                    {
                        instance.close(&mut owner.host)?;
                    }
                    instance.close(&mut owner.host)
                }));
                match closed {
                    Ok(Ok(())) => a.close = CloseState::Closed,
                    Ok(Err(error)) => {
                        a.close = CloseState::Failed;
                        a.close_failure = Some(CloseFailure::Core(error));
                    }
                    Err(_) => {
                        a.close = CloseState::Unknown;
                        a.close_failure = Some(CloseFailure::Unwind);
                    }
                }
                if a.close == CloseState::Closed {
                    a.instance = None;
                    a.stop = None;
                } else if a.error.is_none() {
                    a.error = Some(DeliveryError::Close);
                }
            }
            a.finished = true;
            self.state.history_delivery_debt = a.close != CloseState::Closed;
        }
        self.read_blob_history_delivery(approval.key)
    }
    /// Reading the original report never reclaims, closes or dispatches anything.
    pub fn read_blob_history_delivery(&mut self, key: DeliveryKey) -> Result<DeliveryReceipt> {
        let a = self.history_deliveries.get(key)?;
        let veto = a.veto(self, false);
        let result = a.receipt(veto);
        self.history_deliveries.get_mut(key)?.read = true;
        Ok(result)
    }
    pub fn cancel_blob_history_delivery(&mut self, key: DeliveryKey) -> Result<DeliveryReceipt> {
        let a = self.history_deliveries.get_mut(key)?;
        a.stop();
        if !a.consumed {
            a.consumed = true;
            a.finished = true;
            a.error = Some(DeliveryError::Cancelled);
        }
        self.read_blob_history_delivery(key)
    }
    /// Acknowledge this disclosure receipt only, never the original upload result.
    pub fn acknowledge_blob_history_delivery(&mut self, key: DeliveryKey) -> Result<()> {
        let a = self.history_deliveries.get(key)?;
        if !a.owner_matches(self)
            || !a.finished
            || !a.read
            || a.instance.is_some()
            || !matches!(a.close, CloseState::NotCreated | CloseState::Closed)
        {
            return Err("original history delivery is unread or unresolved".into());
        }
        self.history_deliveries.get_mut(key)?.acknowledged = true;
        Ok(())
    }
}
