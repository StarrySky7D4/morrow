//! Private editor scopes. Correlation tickets are not grants and never rebind after revocation.
//! One archive is retained per scope; individual tickets retain only original observations.
use super::*;
use crate::{projection::proto, projection_v2 as v2};
use morrow_core::{
    dispatch::{ConnectionBinding, HostBinding},
    task_evidence::{
        self, Evidence,
        proto::{Batch, Observation, TaskEvidence},
    },
    transaction,
};
use prost::Message;
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, sync::Arc, time::Duration};
pub const MAX_SCOPE_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_GLOBAL_BYTES: usize = 128 * 1024 * 1024;
pub const MAX_SCOPES: usize = 8;
pub const TOTAL_FUEL: u64 = 1_000_000_000;
pub const IDLE_TTL: Duration = Duration::from_secs(30 * 60);
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PastePart {
    pub ticket: String,
    pub literal: String,
    pub selection: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PasteEvent {
    pub id: String,
    pub field: String,
    pub before: String,
    pub start_utf16: u32,
    pub end_utf16: u32,
    pub parts: Vec<PastePart>,
    pub after: String,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AttachmentAlias {
    pub id: String,
    pub location: String,
    pub name: String,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EditorSnapshot {
    pub title: String,
    pub description: String,
    pub hypothesis: String,
    pub conclusion: String,
    pub todos: String,
    pub aliases: Vec<AttachmentAlias>,
}
#[derive(Clone, Copy, PartialEq, Eq)]
struct Binding {
    host: HostBinding,
    connection: ConnectionBinding,
    package: [u8; 32],
}
struct Ticket {
    sequence: u64,
    parent: Option<String>,
    observation: Observation,
}
struct Pending {
    operation: String,
    fingerprint: [u8; 32],
    evidence: Arc<Evidence>,
}
struct Scope {
    target: String,
    revision: u64,
    prior: Option<[u8; 32]>,
    binding: Binding,
    archive: Arc<[u8]>,
    idle: Instant,
    bytes: usize,
    fuel: u64,
    sequence: u64,
    tickets: BTreeMap<String, Ticket>,
    events: Vec<PasteEvent>,
    pending: Option<Pending>,
}
impl Scope {
    fn reserve_fuel(&mut self, budget: u64) -> Result<()> {
        self.fuel = self
            .fuel
            .checked_sub(budget)
            .ok_or("editor capture total fuel exhausted")?;
        Ok(())
    }
    fn refund_fuel(&mut self, reserved: u64, actual: &Observation) -> Result<()> {
        let budget = actual.budget.as_ref().ok_or("capture budget missing")?;
        if budget.fuel != reserved || actual.fuel_remaining > reserved {
            return Err("capture execution budget changed".into());
        }
        let remaining = self
            .fuel
            .checked_add(actual.fuel_remaining)
            .filter(|v| *v <= TOTAL_FUEL)
            .ok_or("capture fuel overflow")?;
        self.fuel = remaining;
        Ok(())
    }
}
#[derive(Default)]
pub(super) struct CaptureScopes {
    nonce: Option<String>,
    sequence: u64,
    scopes: BTreeMap<String, Scope>,
}
impl CaptureScopes {
    pub(super) fn clear(&mut self) {
        self.scopes.clear();
    }
    fn expire_at(&mut self, now: Instant) {
        self.scopes
            .retain(|_, s| now.checked_duration_since(s.idle).unwrap_or_default() < IDLE_TTL);
    }
    fn used(&self) -> usize {
        self.scopes.values().map(|s| s.bytes).sum()
    }
    fn room(&self, id: &str, extra: usize) -> Result<()> {
        let s = self.scopes.get(id).ok_or("capture scope unavailable")?;
        if s.bytes
            .checked_add(extra)
            .is_none_or(|n| n > MAX_SCOPE_BYTES)
            || self
                .used()
                .checked_add(extra)
                .is_none_or(|n| n > MAX_GLOBAL_BYTES)
        {
            return Err("capture scope memory limit; nothing was truncated".into());
        }
        Ok(())
    }
    fn key(&mut self) -> Result<String> {
        if self.nonce.is_none() {
            let mut bytes = [0u8; 16];
            getrandom::fill(&mut bytes).map_err(|_| "capture scope entropy unavailable")?;
            self.nonce = Some(bytes.iter().map(|b| format!("{b:02x}")).collect());
        }
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or("capture scope identity exhausted")?;
        Ok(format!(
            "capture-{}-{}",
            self.nonce.as_ref().expect("nonce initialized"),
            self.sequence
        ))
    }
}
fn observation(e: &Evidence) -> Result<Observation> {
    let d = e.data();
    if d.schema_version != task_evidence::VERSION || d.batch.is_some() {
        return Err("expected actual single observation".into());
    }
    Ok(Observation {
        invocation: d.invocation.clone(),
        budget: d.budget,
        backend: d.backend.clone(),
        completion: d.completion.clone(),
        fault: d.fault,
        exit_code: d.exit_code,
        observed_host_calls: d.observed_host_calls,
        fuel_remaining: d.fuel_remaining,
    })
}
fn event_size(e: &PasteEvent) -> usize {
    e.id.len()
        + e.field.len()
        + e.before.len()
        + e.after.len()
        + e.parts
            .iter()
            .map(|p| p.ticket.len() + p.literal.len() + p.selection.len() + 128)
            .sum::<usize>()
        + 256
}
fn fingerprint(request: &Request, snapshot: &proto::EditorSnapshot) -> Result<[u8; 32]> {
    let mut intent = request.clone();
    intent.current = Idea::default();
    intent.now_ms = 0;
    let input = codec::encode_request(&intent)?;
    let mut hash = Sha256::new();
    hash.update((input.len() as u64).to_le_bytes());
    hash.update(input);
    hash.update(snapshot.encode_to_vec());
    Ok(hash.finalize().into())
}
struct Bundle {
    archive: Arc<[u8]>,
    observations: Vec<Observation>,
    parents: Vec<proto::CaptureParent>,
    applications: Vec<proto::PasteApplication>,
}
fn bundle(scope: &Scope) -> Result<Bundle> {
    let mut selected = BTreeSet::new();
    for event in &scope.events {
        for part in &event.parts {
            if !part.ticket.is_empty() {
                let mut next = Some(part.ticket.as_str());
                while let Some(id) = next {
                    let ticket = scope.tickets.get(id).ok_or("adopted capture missing")?;
                    if !selected.insert(id.to_owned()) {
                        break;
                    }
                    next = ticket.parent.as_deref();
                }
            }
        }
    }
    if selected.len() >= task_evidence::MAX_BATCH_OBSERVATIONS {
        return Err("capture save observation limit".into());
    }
    let mut selected = selected
        .into_iter()
        .map(|id| {
            let seq = scope.tickets[&id].sequence;
            (seq, id)
        })
        .collect::<Vec<_>>();
    selected.sort_by_key(|v| v.0);
    let indexes = selected
        .iter()
        .enumerate()
        .map(|(i, (_, id))| (id.clone(), i as u32))
        .collect::<BTreeMap<_, _>>();
    let mut raw = scope.archive.len();
    for (_, id) in &selected {
        raw = raw
            .checked_add(scope.tickets[id].observation.encoded_len())
            .ok_or("capture byte overflow")?;
        if raw > task_evidence::MAX_RAW_BYTES {
            return Err("adopted captures exceed evidence capacity".into());
        }
    }
    let mut observations = Vec::new();
    let mut parents = Vec::new();
    for (_, id) in selected {
        let t = &scope.tickets[&id];
        parents.push(proto::CaptureParent {
            parent: t.parent.as_ref().map(|p| indexes[p]),
        });
        observations.push(t.observation.clone());
    }
    let applications = scope
        .events
        .iter()
        .map(|e| proto::PasteApplication {
            id: e.id.clone(),
            field: e.field.clone(),
            before: e.before.clone(),
            start_utf16: e.start_utf16,
            end_utf16: e.end_utf16,
            parts: e
                .parts
                .iter()
                .map(|p| proto::PastePart {
                    observation: (!p.ticket.is_empty()).then(|| indexes[&p.ticket]),
                    literal: p.literal.clone(),
                    selection: p.selection.clone(),
                })
                .collect(),
            after: e.after.clone(),
        })
        .collect();
    Ok(Bundle {
        archive: Arc::clone(&scope.archive),
        observations,
        parents,
        applications,
    })
}
impl Workbench {
    fn capture_binding(&self) -> Result<Binding> {
        self.host.local()?;
        if !self.writable() {
            return Err("capture scope is unavailable while host or plugin is read only".into());
        }
        let root = self
            .pool
            .root(self.plugin.as_ref().ok_or("plugin unavailable")?)?;
        Ok(Binding {
            host: self.host.local()?.binding(),
            connection: root.connection().binding(),
            package: root.package().package().digest(),
        })
    }
    fn check_capture_scope(&mut self, id: &str, allow_pending: bool) -> Result<()> {
        self.host.local()?;
        self.capture_scopes.expire_at(Instant::now());
        let binding = match self.capture_binding() {
            Ok(b) => b,
            Err(e) => {
                self.capture_scopes.scopes.remove(id);
                return Err(e);
            }
        };
        self.capture_scopes
            .scopes
            .retain(|_, s| s.binding == binding);
        let valid = self
            .capture_scopes
            .scopes
            .get(id)
            .is_some_and(|s| s.binding == binding);
        if !valid {
            self.capture_scopes.scopes.remove(id);
            return Err("capture scope expired, closed, or its package/connection changed".into());
        }
        let s = self
            .capture_scopes
            .scopes
            .get_mut(id)
            .expect("checked scope");
        if !allow_pending && s.pending.is_some() {
            return Err("capture scope awaits reconciliation of its original save".into());
        }
        s.idle = Instant::now();
        Ok(())
    }
    pub fn open_capture_scope(&mut self, target: &str, revision: u64) -> Result<String> {
        self.host.local()?;
        self.prepare_write()?;
        self.capture_scopes.expire_at(Instant::now());
        // The same fixed identity validation as the persisted content API, without granting write.
        morrow_core::runtime::Command::ReadSummary {
            request_id: "capture-open".into(),
            card_id: target.into(),
        }
        .validate()?;
        let prior = if revision == 0 {
            if self.host.local()?.store_local().card(target)?.is_some() {
                return Err("capture create target already exists".into());
            }
            None
        } else {
            let card = self.authorized_read(target)?;
            if card.summary().revision != revision {
                return Err("capture editor revision conflict".into());
            }
            Self::decode(&card)?;
            Some(<[u8; 32]>::from(Sha256::digest(card.encode())))
        };
        let binding = self.capture_binding()?;
        self.capture_scopes
            .scopes
            .retain(|_, s| s.binding == binding);
        if self.capture_scopes.scopes.len() >= MAX_SCOPES {
            return Err("too many open capture editors".into());
        }
        let archive = self
            .pool
            .root(self.plugin.as_ref().ok_or("plugin unavailable")?)?
            .package()
            .package()
            .archive();
        let bytes = archive
            .len()
            .checked_add(target.len() + 512)
            .ok_or("capture bytes overflow")?;
        if bytes > MAX_SCOPE_BYTES
            || self
                .capture_scopes
                .used()
                .checked_add(bytes)
                .is_none_or(|n| n > MAX_GLOBAL_BYTES)
        {
            return Err("capture memory limit".into());
        }
        let archive: Arc<[u8]> = Arc::from(archive);
        let id = self.capture_scopes.key()?;
        self.capture_scopes.scopes.insert(
            id.clone(),
            Scope {
                target: target.into(),
                revision,
                prior,
                binding,
                archive,
                idle: Instant::now(),
                bytes,
                fuel: TOTAL_FUEL,
                sequence: 0,
                tickets: BTreeMap::new(),
                events: Vec::new(),
                pending: None,
            },
        );
        Ok(id)
    }
    pub fn close_capture_scope(&mut self, scope: &str) {
        self.capture_scopes.scopes.remove(scope);
    }
    fn reserve_capture_fuel(&mut self, scope: &str) -> Result<u64> {
        self.check_capture_scope(scope, false)?;
        let budget = self
            .pool
            .root(self.plugin.as_ref().ok_or("plugin unavailable")?)?
            .package()
            .limits()
            .fuel;
        let s = self
            .capture_scopes
            .scopes
            .get_mut(scope)
            .expect("checked scope");
        s.reserve_fuel(budget)?;
        Ok(budget)
    }
    fn refund_capture_fuel(
        &mut self,
        scope: &str,
        reserved: u64,
        actual: &Observation,
    ) -> Result<()> {
        if let Some(s) = self.capture_scopes.scopes.get_mut(scope)
            && let Err(error) = s.refund_fuel(reserved, actual)
        {
            self.capture_scopes.scopes.remove(scope);
            return Err(error);
        }
        Ok(())
    }
    pub fn capture_scoped(
        &mut self,
        scope: &str,
        input: Vec<u8>,
        parent: &str,
    ) -> Result<(String, Vec<u8>)> {
        self.host.local()?;
        self.check_capture_scope(scope, false)?;
        let (format, source) = v2::capture_input(&input)?;
        if !parent.is_empty() {
            let s = &self.capture_scopes.scopes[scope];
            let p = s
                .tickets
                .get(parent)
                .ok_or("capture parent is outside this editor")?;
            let (_, _, markdown, _) = v2::captured(&p.observation)?;
            if format != "plain" || source != markdown {
                return Err("capture input differs from parent output".into());
            }
        }
        if self.capture_scopes.scopes[scope].tickets.len() >= 1023 {
            return Err("editor capture ticket limit".into());
        }
        self.capture_scopes
            .room(scope, 2 * morrow_core::task::MAX_TASK_BYTES + 4096)?;
        self.counter = self
            .counter
            .checked_add(1)
            .ok_or("task counter exhausted")?;
        let task = Invocation::new_transform(
            &format!("capture-{}", self.counter),
            Transform {
                handler: "capture.convert".into(),
                input_type: "morrow.capture.request.v1".into(),
                output_type: "morrow.capture.response.v1".into(),
                input,
            },
        )?;
        let reserved = self.reserve_capture_fuel(scope)?;
        let result = self.pool.record_transform(
            self.manager.as_ref().ok_or("plugin manager unavailable")?,
            self.host.local_mut()?,
            self.plugin.as_ref().ok_or("plugin unavailable")?,
            &task,
        );
        self.finish_stopped_session();
        // If no trustworthy execution observation exists the reserved maximum stays charged.
        let recorded = result?;
        let actual = observation(recorded.evidence())?;
        self.refund_capture_fuel(scope, reserved, &actual)?;
        self.check_capture_scope(scope, false)?;
        let (_, _, _, payload) = v2::captured(&actual)?;
        let extra = actual.encoded_len() + parent.len() + 512;
        self.capture_scopes.room(scope, extra)?;
        let s = self
            .capture_scopes
            .scopes
            .get_mut(scope)
            .expect("checked scope");
        s.sequence = s
            .sequence
            .checked_add(1)
            .ok_or("capture ticket identity exhausted")?;
        let ticket = format!("{scope}-ticket-{}", s.sequence);
        s.bytes += extra;
        s.tickets.insert(
            ticket.clone(),
            Ticket {
                sequence: s.sequence,
                parent: (!parent.is_empty()).then(|| parent.into()),
                observation: actual,
            },
        );
        Ok((ticket, payload))
    }
    pub fn record_paste(&mut self, scope: &str, event: PasteEvent) -> Result<()> {
        self.host.local()?;
        self.check_capture_scope(scope, false)?;
        v2::event_bounds(&event)?;
        let s = &self.capture_scopes.scopes[scope];
        if let Some(old) = s.events.iter().find(|e| e.id == event.id) {
            return if old == &event {
                Ok(())
            } else {
                Err("paste application identity conflict".into())
            };
        }
        if s.events.len() >= v2::MAX_APPLICATIONS {
            return Err("editor paste application limit".into());
        }
        let mut inserted = String::new();
        for p in &event.parts {
            if p.ticket.is_empty() {
                if !p.selection.is_empty() {
                    return Err("literal paste selection must be empty".into());
                }
                inserted.push_str(&p.literal);
            } else {
                if !p.literal.is_empty() {
                    return Err("mixed paste part".into());
                }
                let t = s
                    .tickets
                    .get(&p.ticket)
                    .ok_or("capture ticket is outside this editor")?;
                let (format, source, markdown, _) = v2::captured(&t.observation)?;
                match p.selection.as_str() {
                    "outputMarkdown" => inserted.push_str(&markdown),
                    "inputPlainText" if format == "plain" => inserted.push_str(&source),
                    _ => return Err("unsupported capture selection".into()),
                }
            }
            if inserted.len() > v2::MAX_TEXT {
                return Err("paste insertion limit".into());
            }
        }
        v2::replacement(
            &event.before,
            event.start_utf16,
            event.end_utf16,
            &inserted,
            &event.after,
        )?;
        let extra = event_size(&event);
        self.capture_scopes.room(scope, extra)?;
        let s = self
            .capture_scopes
            .scopes
            .get_mut(scope)
            .expect("checked scope");
        s.bytes += extra;
        s.events.push(event);
        Ok(())
    }
}

impl Workbench {
    fn retry_captured_save(
        &mut self,
        operation: &str,
        id: &str,
        request: &Request,
        revision: Option<u64>,
        scope: &str,
        snapshot: &EditorSnapshot,
    ) -> Result<Option<Record>> {
        if matches!(
            self.host.local()?.store_local().lookup(operation)?,
            transaction::Lookup::Absent
        ) {
            return Ok(None);
        }
        let (commit, receipt) = self
            .host
            .local()?
            .store_local()
            .operation_commit(id, operation)?
            .ok_or("operation belongs to another content object")?;
        let evidence = self
            .host
            .local()?
            .store_local()
            .operation_evidence(id, operation)?;
        if evidence.len() != 1 {
            return Err("capture retry requires one original batch".into());
        }
        let batch = evidence[0]
            .data()
            .batch
            .as_ref()
            .ok_or("operation has no original capture provenance")?;
        if batch.intent_type != v2::INTENT_TYPE {
            return Err("operation has a different original capture route".into());
        }
        let facts = v2::decode(&batch.intent)?;
        if facts.scope != scope || facts.snapshot.as_ref() != Some(&v2::snapshot_proto(snapshot)?) {
            return Err("capture retry scope or editor endpoint conflict".into());
        }
        let result = self.retry_projected(id, request, revision, &commit, &receipt, &evidence)?;
        self.close_capture_scope(scope);
        Ok(result)
    }
    fn project_captured(
        &mut self,
        operation: &str,
        id: &str,
        request: Request,
        prior: Option<&CardRecord>,
        scope: &str,
        snapshot: EditorSnapshot,
    ) -> Result<(projection::Projection, Arc<Evidence>)> {
        self.check_capture_scope(scope, true)?;
        let snapshot = v2::snapshot_proto(&snapshot)?;
        v2::verify_snapshot(&snapshot, &request)?;
        let fingerprint = fingerprint(&request, &snapshot)?;
        let s = &self.capture_scopes.scopes[scope];
        let hash = prior.map(|p| <[u8; 32]>::from(Sha256::digest(p.encode())));
        if s.target != id
            || s.revision != prior.map_or(0, |p| p.summary().revision)
            || s.prior != hash
        {
            return Err("editor scope target or original revision changed".into());
        }
        if let Some(pending) = &s.pending {
            if pending.operation != operation || pending.fingerprint != fingerprint {
                return Err("capture scope is locked to its original save intent".into());
            }
            let e = Arc::clone(&pending.evidence);
            return Ok((projection::derive(&e)?, e));
        }
        let bundle = bundle(s)?;
        // Reject known input/fact excess before another guest attempt. Final encoded size is checked
        // again with the exact actual completion; no adopted observation is silently dropped.
        let rough = bundle.archive.len()
            + bundle
                .observations
                .iter()
                .map(Message::encoded_len)
                .sum::<usize>()
            + bundle
                .applications
                .iter()
                .map(Message::encoded_len)
                .sum::<usize>()
            + snapshot.encoded_len()
            + prior.map_or(0, |p| p.encode().len());
        if rough > task_evidence::MAX_RAW_BYTES {
            return Err("adopted paste evidence exceeds save capacity".into());
        }
        let reserved = self.reserve_capture_fuel(scope)?;
        // On any error lacking a returned trusted report the reserved maximum remains charged.
        let (_, actual) = self.project_content(operation, id, request, prior, None)?;
        let b = actual
            .data()
            .batch
            .as_ref()
            .ok_or("missing final workbench observation")?;
        let final_observation = b
            .observations
            .first()
            .ok_or("missing final observation")?
            .clone();
        self.refund_capture_fuel(scope, reserved, &final_observation)?;
        self.check_capture_scope(scope, false)?;
        if actual.data().package_archive.as_slice() != bundle.archive.as_ref() {
            return Err("capture package changed before save".into());
        }
        let facts = proto::ContentProjectionV2 {
            schema_version: 2,
            content: b.intent.clone(),
            scope: scope.into(),
            captures: bundle.parents,
            applications: bundle.applications,
            snapshot: Some(snapshot),
        };
        if facts.encoded_len() > task_evidence::MAX_INTENT_BYTES {
            return Err("capture intent exceeds save capacity".into());
        }
        let mut observations = bundle.observations;
        observations.push(final_observation);
        let evidence = task_evidence::encode(TaskEvidence {
            schema_version: task_evidence::BATCH_VERSION,
            package_archive: bundle.archive.to_vec(),
            batch: Some(Batch {
                intent_type: v2::INTENT_TYPE.into(),
                intent: facts.encode_to_vec(),
                total_fuel: TOTAL_FUEL,
                observations,
            }),
            ..Default::default()
        })?;
        let projection = projection::derive(&evidence)?;
        // Pending retains original bytes for all uncertain outcomes. Account raw, decoded fields
        // and compressed container in addition to the original ticket storage.
        let extra = evidence
            .raw()
            .len()
            .checked_mul(2)
            .and_then(|n| n.checked_add(evidence.container().len()))
            .ok_or("capture pending size overflow")?;
        self.capture_scopes.room(scope, extra)?;
        let evidence = Arc::new(evidence);
        let s = self
            .capture_scopes
            .scopes
            .get_mut(scope)
            .expect("checked scope");
        s.bytes += extra;
        s.pending = Some(Pending {
            operation: operation.into(),
            fingerprint,
            evidence: Arc::clone(&evidence),
        });
        Ok((projection, evidence))
    }
    pub fn create_captured(
        &mut self,
        operation: &str,
        draft: Idea,
        scope: &str,
        snapshot: EditorSnapshot,
    ) -> Result<Record> {
        self.host.local()?;
        self.prepare_write()?;
        let id = draft.id.clone();
        let mut request = command(Action::Create);
        request.proposed = draft;
        if let Some(record) =
            self.retry_captured_save(operation, &id, &request, None, scope, &snapshot)?
        {
            return Ok(record);
        }
        let (projection, evidence) =
            self.project_captured(operation, &id, request, None, scope, snapshot)?;
        self.commit_projection(&projection, std::slice::from_ref(evidence.as_ref()))?;
        self.close_capture_scope(scope);
        self.staged.retain(|(card, _), _| card != &id);
        Self::decode(&projection.card)
    }
    pub fn apply_captured(
        &mut self,
        mutation: Mutation<'_>,
        scope: &str,
        snapshot: EditorSnapshot,
    ) -> Result<Record> {
        self.host.local()?;
        self.prepare_write()?;
        let Mutation {
            operation,
            id,
            revision,
            action,
            proposed,
            text,
            flag,
        } = mutation;
        if action != Action::Edit {
            return Err("captured editor save must be Edit".into());
        }
        let mut intent = command(action);
        intent.proposed = proposed.unwrap_or_default();
        intent.text = text.into();
        intent.flag = flag;
        if let Some(record) =
            self.retry_captured_save(operation, id, &intent, Some(revision), scope, &snapshot)?
        {
            return Ok(record);
        }
        let prior = self.authorized_read(id)?;
        let old = Self::decode(&prior)?;
        if old.revision != revision {
            return Err("revision conflict".into());
        }
        intent.current = old.idea;
        intent.current.description.clear();
        intent.current.hypothesis.clear();
        intent.current.conclusion.clear();
        intent.now_ms = now(self.start);
        let (projection, evidence) =
            self.project_captured(operation, id, intent, Some(&prior), scope, snapshot)?;
        self.commit_projection(&projection, std::slice::from_ref(evidence.as_ref()))?;
        self.close_capture_scope(scope);
        self.undo.remove(id);
        self.staged.retain(|(card, _), _| card != id);
        Self::decode(&projection.card)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn binding() -> Binding {
        let dir = tempfile::tempdir().unwrap();
        let store =
            morrow_core::store::Store::open(&dir.path().join("scope-tests.db"), Default::default())
                .unwrap();
        let mut host = morrow_core::dispatch::HostRuntime::new(store).unwrap();
        let connection = host.connect().unwrap();
        Binding {
            host: host.binding(),
            connection: connection.binding(),
            package: [1; 32],
        }
    }
    fn scope(binding: Binding, idle: Instant, bytes: usize) -> Scope {
        Scope {
            target: "synthetic-card".into(),
            revision: 0,
            prior: None,
            binding,
            archive: Arc::from([]),
            idle,
            bytes,
            fuel: TOTAL_FUEL,
            sequence: 0,
            tickets: BTreeMap::new(),
            events: vec![],
            pending: None,
        }
    }
    #[test]
    fn idle_expiration_releases_actual_scope_accounting_at_the_exact_boundary() {
        let start = Instant::now();
        let mut scopes = CaptureScopes::default();
        scopes
            .scopes
            .insert("older".into(), scope(binding(), start, 400));
        scopes.scopes.insert(
            "newer".into(),
            scope(binding(), start + Duration::from_secs(1), 200),
        );
        scopes.expire_at(start + IDLE_TTL - Duration::from_nanos(1));
        assert_eq!(scopes.scopes.len(), 2);
        assert_eq!(scopes.used(), 600);
        scopes.expire_at(start + IDLE_TTL);
        assert!(!scopes.scopes.contains_key("older"));
        assert_eq!(scopes.used(), 200);
        scopes.expire_at(start + IDLE_TTL + Duration::from_secs(1));
        assert_eq!(scopes.used(), 0);
    }
    #[test]
    fn scope_and_global_quota_checks_do_not_silently_truncate_or_mutate_usage() {
        let mut scopes = CaptureScopes::default();
        let binding = binding();
        let start = Instant::now();
        scopes
            .scopes
            .insert("one".into(), scope(binding, start, MAX_SCOPE_BYTES - 1));
        assert!(scopes.room("one", 1).is_ok());
        assert!(scopes.room("one", 2).is_err());
        scopes
            .scopes
            .insert("two".into(), scope(binding, start, MAX_SCOPE_BYTES));
        assert!(scopes.room("one", 1).is_ok());
        scopes
            .scopes
            .insert("three".into(), scope(binding, start, 1));
        assert_eq!(scopes.used(), MAX_GLOBAL_BYTES);
        assert!(scopes.room("one", 1).is_err());
        assert_eq!(scopes.scopes["one"].bytes, MAX_SCOPE_BYTES - 1);
        scopes.scopes.remove("two");
        assert!(scopes.room("one", 1).is_ok());
    }
    #[test]
    fn failed_attempts_keep_fuel_charged_and_trusted_reports_refund_only_unused_fuel() {
        use task_evidence::proto::{ExecutionBudget, StableFault};
        let mut s = scope(binding(), Instant::now(), 0);
        let budget = 20_000_000;
        s.reserve_fuel(budget).unwrap();
        // A failed execution without a trustworthy report cannot reclaim its reservation.
        assert_eq!(s.fuel, TOTAL_FUEL - budget);
        s.reserve_fuel(budget).unwrap();
        let actual = Observation {
            budget: Some(ExecutionBudget {
                fuel: budget,
                memory_bytes: 65536,
                host_calls: 0,
            }),
            fault: StableFault::Trap as i32,
            fuel_remaining: 7_000_000,
            ..Default::default()
        };
        s.refund_fuel(budget, &actual).unwrap();
        assert_eq!(s.fuel, TOTAL_FUEL - 2 * budget + actual.fuel_remaining);
        let before = s.fuel;
        assert!(s.refund_fuel(budget + 1, &actual).is_err());
        assert_eq!(s.fuel, before);
        s.reserve_fuel(before).unwrap();
        assert_eq!(s.fuel, 0);
        assert!(s.reserve_fuel(1).is_err());
        assert_eq!(s.fuel, 0);
    }
}
