//! Sealed native grant adapter: existing Core owns attempts/materials, native ledger
//! owns approval. Neither persisted data nor a guest hash reconstructs live authority.
use crate::{
    Result,
    authority::{self, proto},
    pipe_driver::{Gate, TicketOwnership, network_ticket},
    wire,
};
use morrow_core::{
    io,
    io_evidence::{Kind, Material},
    io_intent::{Command, ObservationSource, Record},
    plugin_package::io::IoCapability,
    store::Store,
};
use morrow_network_node_stream::{
    Limits, RawHttpRequest,
    client::{Client, EndpointPolicy},
    stream::{SendContext, StreamGuard},
};
use prost::Message;
use rusqlite::{TransactionBehavior, params};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;
use url::Url;
#[derive(Clone)]
pub(crate) struct Parent {
    pub pid: u32,
    pub session: u64,
    pub epoch: u64,
    pub root: PathBuf,
    pub expected_profile: proto::Profile,
    pub approval: proto::Approval,
    pub store: Arc<Mutex<Store>>,
    pub gate: Gate,
    pub origin: String,
    pub deadline: Instant,
}
pub(crate) struct Proposal {
    pub prepare: wire::Prepare,
    pub body: Vec<u8>,
    pub decision: wire::Decision,
}
pub(crate) struct NativeHttpGrant {
    parent: Parent,
    record: proto::HttpApproval,
    proposal: Proposal,
    request: io::Request,
    command: Command,
    claimed: Option<Record>,
}
pub(crate) struct SendTicket {
    pub client: Client,
    pub request: RawHttpRequest,
    pub context: SendContext,
    pub ordinal: u64,
    pub owner: TicketOwnership,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct NativeRevocation {
    pub source: u32,
    pub reason: u32,
}
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn nonce() -> Result<Vec<u8>> {
    let mut b = vec![0; 32];
    getrandom::fill(&mut b).map_err(err)?;
    Ok(b)
}
fn load(db: &rusqlite::Connection, id: &[u8]) -> Result<proto::HttpApproval> {
    let bytes:Vec<u8>=db.query_row("SELECT CASE WHEN length(payload)<=8352 THEN payload ELSE NULL END FROM http_approvals WHERE id=?1",[wire::hex(id)],|r|r.get(0)).map_err(err)?;
    authority::unpack(&bytes)
}
fn save(db: &rusqlite::Connection, record: &proto::HttpApproval) -> Result<()> {
    db.execute("INSERT INTO http_approvals(id,payload) VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload",params![wire::hex(&record.id),authority::pack(record)?]).map_err(err)?;
    Ok(())
}
impl Parent {
    pub(crate) fn revoke_native(&self, reason: u32) -> Result<NativeRevocation> {
        let mut db = authority::connect(&self.root)?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(err)?;
        self.check_record(&tx, true)?;
        let mut record = authority::load_grant(&tx, &self.approval.id)?;
        authority::record_revocation(&tx, &mut record, 2, reason)?;
        tx.commit().map_err(err)?;
        Ok(NativeRevocation {
            source: record.revocation_source,
            reason: record.revocation_reason,
        })
    }
    fn check_record(&self, db: &rusqlite::Connection, history: bool) -> Result<()> {
        authority::validate_profile_mode(db,&self.expected_profile,self.expected_profile.runtime_mode==1)?;
        let owner = authority::load_owner(db)?.ok_or("native owner missing")?;
        if owner.grant_id != self.approval.id
            || owner.issuer != self.approval.issuer
            || owner.generation != self.approval.generation
            || owner.child_pid != self.pid
            || owner.session != self.session
            || owner.epoch != self.epoch
            || (!history && !matches!(owner.phase.as_str(), "Preparing" | "Active"))
        {
            return Err("native owner binding/state drift".into());
        }
        let actual = authority::load_grant(db, &self.approval.id)?;
        let mut expected = self.approval.clone();
        if history && actual.state == 3 {
            expected.state = 3;
            expected.revocation_source = actual.revocation_source;
            expected.revocation_reason = actual.revocation_reason;
        }
        if actual != expected || !matches!(actual.state, 2 | 3) || (!history && actual.state != 2) {
            return Err("native parent drift/revoked".into());
        }
        if !history {
            let g = self.gate.lock().map_err(|_| "gate poison")?;
            if g.revoked || Instant::now() >= g.deadline {
                return Err("original grant expired/revoked".into());
            }
        }
        Ok(())
    }
    pub(crate) fn proposal(
        &self,
        initial: &wire::Frame,
        prepare: wire::Prepare,
        body: Vec<u8>,
    ) -> Result<Proposal> {
        let request_sha256 = wire::request_digest(
            initial.session,
            initial.instance_epoch,
            &initial.operation_id,
            initial.attempt,
            &prepare,
            &body,
        )
        .map_err(err)?
        .to_vec();
        let target = Url::parse(&prepare.absolute_target).map_err(err)?;
        let origin = Url::parse(&self.origin).map_err(err)?;
        if origin.scheme() != "http"
            || origin.host_str() != Some("127.0.0.1")
            || origin.path() != "/"
            || origin.query().is_some()
            || origin.fragment().is_some()
            || !origin.username().is_empty()
            || origin.password().is_some()
            || target.origin() != origin.origin()
            || target.as_str() != prepare.absolute_target
        {
            return Err("fixture endpoint mismatch".into());
        }
        const ALLOWED: &[&str] = &[
            "accept",
            "content-type",
            "originator",
            "session-id",
            "thread-id",
            "x-client-request-id",
            "x-codex-turn-metadata",
            "x-codex-window-id",
        ];
        if prepare
            .headers
            .iter()
            .any(|h| !ALLOWED.contains(&h.name.to_ascii_lowercase().as_str()))
        {
            return Err("request header not approved for fixture profile".into());
        }
        let body_sha256 = wire::digest(&body).to_vec();
        let decision = wire::Decision {
            proposal_ref: nonce()?,
            body_sha256,
            request_sha256,
            http_grant_ref: vec![],
            endpoint_ref: vec![],
            body_bytes: prepare.body_bytes,
            send_budget: 0,
            response_limit_bytes: prepare.response_limit_bytes,
        };
        // Same Core policy used before any grant, even while endpointRef is only a placeholder.
        let submission = io::HttpSubmission {
            operation_id: initial.operation_id.clone(),
            deadline_ms: self.approval.lifetime_ms,
            endpoint: vec![b'a'; 64],
            method: prepare.method.clone(),
            relative_target: relative(&target),
            headers: prepare
                .headers
                .iter()
                .map(|h| io::Header {
                    name: h.name.clone(),
                    value: h.value.clone(),
                })
                .collect(),
            body: body.clone(),
            credential: vec![],
        };
        io::validate_http_submission(&submission).map_err(err)?;
        Ok(Proposal {
            prepare,
            body,
            decision,
        })
    }
}
fn relative(target: &Url) -> String {
    let mut p = target.path().to_owned();
    if let Some(q) = target.query() {
        p.push('?');
        p.push_str(q);
    }
    p
}
impl NativeHttpGrant {
    pub(crate) fn intent_phase(&self) -> Result<wire::IntentPhase> {
        let store = self.parent.store.lock().map_err(|_| "Store poisoned")?;
        Ok(
            match store
                .lookup_io_intent(&self.command.subject, &self.command.operation_id)
                .map_err(err)?
                .map(|r| r.phase())
            {
                Some(morrow_core::io_intent::Phase::Prepared) => wire::IntentPhase::Prepared,
                Some(morrow_core::io_intent::Phase::Observed) => wire::IntentPhase::Observed,
                Some(morrow_core::io_intent::Phase::CancelledBeforeDispatch) => {
                    wire::IntentPhase::CancelledBeforeDispatch
                }
                Some(morrow_core::io_intent::Phase::OutcomeUnknown) => wire::IntentPhase::Unknown,
                _ => wire::IntentPhase::Absent,
            },
        )
    }
    /// Called only from trusted operator control, never from guest frame dispatch.
    pub(crate) fn approve(
        parent: Parent,
        mut proposal: Proposal,
        expected_ref: &[u8],
        expected_hash: &[u8],
        response_limit: u32,
    ) -> Result<Self> {
        if proposal.decision.proposal_ref != expected_ref
            || proposal.decision.request_sha256 != expected_hash
            || response_limit == 0
            || response_limit > proposal.prepare.response_limit_bytes
        {
            return Err("HTTP approval expected input/limit mismatch".into());
        }
        let mut db = authority::connect(&parent.root)?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(err)?;
        parent.check_record(&tx, false)?;
        let count: i64 = tx
            .query_row("SELECT count(*) FROM http_approvals", [], |r| r.get(0))
            .map_err(err)?;
        if count >= 512 {
            return Err("HTTP approval quota".into());
        }
        let endpoint = nonce()?;
        let id = nonce()?;
        proposal.decision.endpoint_ref = endpoint.clone();
        proposal.decision.http_grant_ref = id.clone();
        proposal.decision.send_budget = 1;
        proposal.decision.response_limit_bytes = response_limit;
        let target = Url::parse(&proposal.prepare.absolute_target).map_err(err)?;
        let submission = io::HttpSubmission {
            operation_id: parent.approval.operation.as_bytes().to_vec(),
            deadline_ms: parent.approval.lifetime_ms,
            // Existing Core reference contract is printable bytes; v3 carries
            // the original 32-byte opaque reference, mapped bijectively as hex.
            endpoint: wire::hex(&endpoint).into_bytes(),
            method: proposal.prepare.method.clone(),
            relative_target: relative(&target),
            headers: proposal
                .prepare
                .headers
                .iter()
                .map(|h| io::Header {
                    name: h.name.clone(),
                    value: h.value.clone(),
                })
                .collect(),
            body: proposal.body.clone(),
            credential: vec![],
        };
        let request = io::Request::encode_http_submit(1, &submission).map_err(err)?;
        let record = proto::HttpApproval {
            version: 1,
            id,
            parent_id: parent.approval.id.clone(),
            parent_sha256: wire::digest(&parent.approval.encode_to_vec()).to_vec(),
            proposal_ref: proposal.decision.proposal_ref.clone(),
            request_sha256: proposal.decision.request_sha256.clone(),
            body_sha256: proposal.decision.body_sha256.clone(),
            endpoint_ref: proposal.decision.endpoint_ref.clone(),
            core_request_sha256: request.digest().to_vec(),
            response_limit,
            state: 1,
            send_budget: 1,
        };
        let command = Command {
            operation_id: parent.approval.operation.clone(),
            subject: parent.approval.plugin_id.clone(),
            package_sha256: parent
                .approval
                .artifact_sha256
                .as_slice()
                .try_into()
                .map_err(|_| "artifact digest")?,
            capability: IoCapability::HttpRequest,
            protocol_sha256: io::schema_digest(),
            request_sha256: request.digest(),
            approval_sha256: wire::digest(&record.encode_to_vec()),
            target_sha256: wire::digest(parent.origin.as_bytes()),
            request_bytes: request.bytes().len() as u64,
            response_limit: io::MAX_FRAME_BYTES as u64,
        };
        Record::prepared(command.clone()).map_err(err)?;
        save(&tx, &record)?;
        tx.commit()
            .map_err(|e| format!("HTTP approval commit unknown: {e}"))?;
        Ok(Self {
            parent,
            record,
            proposal,
            request,
            command,
            claimed: None,
        })
    }
    pub(crate) fn decision(&self) -> &wire::Decision {
        &self.proposal.decision
    }
    pub(crate) fn claim(
        &mut self,
        commit: &wire::Decision,
        cancel: CancellationToken,
    ) -> Result<SendTicket> {
        if self.claimed.is_some() || commit != self.decision() {
            return Err("duplicate/mismatched HTTP Commit".into());
        }
        let mut db = authority::connect(&self.parent.root)?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(err)?;
        self.parent.check_record(&tx, false)?;
        if load(&tx, &self.record.id)? != self.record || self.record.state != 1 {
            return Err("HTTP grant drift/consumed".into());
        }
        self.record.state = 2;
        save(&tx, &self.record)?;
        tx.commit()
            .map_err(|e| format!("HTTP consumed commit unknown: {e}"))?;
        let mut store = self.parent.store.lock().map_err(|_| "Store poisoned")?;
        if store
            .lookup_io_intent(&self.command.subject, &self.command.operation_id)
            .map_err(err)?
            .is_some()
        {
            return Err("existing operation is history, never resend authority".into());
        }
        let gate = self.parent.gate.clone();
        let check = || {
            let g = gate
                .lock()
                .map_err(|_| morrow_core::Error::Invalid("native gate poison"))?;
            if g.revoked || Instant::now() >= g.deadline {
                Err(morrow_core::Error::Invalid("native grant revoked/expired"))
            } else {
                Ok(())
            }
        };
        let prepared = Record::prepared(self.command.clone()).map_err(err)?;
        store
            .append_io_intent_local_authorized(&prepared, check)
            .map_err(err)?;
        store
            .reserve_io_intent_followup(&self.command, check)
            .map_err(err)?;
        store
            .reserve_io_materials(&self.command, check)
            .map_err(err)?;
        let material = Material::encode(
            Kind::Request,
            &self.command.operation_id,
            &self.command.subject,
            self.request.digest(),
            self.request.bytes(),
        )
        .map_err(err)?;
        store
            .store_io_material(&self.command.subject, Kind::Request, &material, check)
            .map_err(err)?;
        let unknown = store
            .claim_io_dispatch_local_authorized(
                &prepared.propose_dispatch_boundary().map_err(err)?,
                check,
            )
            .map_err(err)?;
        self.claimed = Some(unknown);
        drop(store);
        // The native ledger and Core Store are separate transactions. Only this exact
        // successful Core claim may reach the fence; any uncertain result never dispatches.
        let mut db = authority::connect(&self.parent.root)?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(err)?;
        self.parent.check_record(&tx, false)?;
        if load(&tx, &self.record.id)? != self.record {
            return Err("HTTP grant drift before effect fence".into());
        }
        let (ordinal, owner) = network_ticket(&self.parent.gate)?;
        let deadline = self.parent.deadline;
        tx.commit()
            .map_err(|e| format!("HTTP fence transaction uncertain: {e}"))?;
        let limits = Limits {
            max_request_bytes: 32768,
            max_response_bytes: self.record.response_limit as usize,
            max_header_bytes: 8192,
            max_concurrent: 1,
            timeout: Duration::from_millis(self.parent.approval.lifetime_ms),
        };
        let client = Client::new(
            EndpointPolicy::new(
                &self.parent.origin,
                &[self.proposal.prepare.method.as_str()],
                true,
            )
            .map_err(err)?,
            limits,
        )
        .map_err(err)?;
        let request = RawHttpRequest {
            method: self.proposal.prepare.method.clone(),
            target: self.proposal.prepare.absolute_target.clone(),
            headers: self
                .proposal
                .prepare
                .headers
                .iter()
                .map(|h| (h.name.clone(), h.value.clone()))
                .collect(),
            body: self.proposal.body.clone(),
        };
        Ok(SendTicket {
            client,
            request,
            context: SendContext::guarded(
                tokio::time::Instant::from_std(deadline),
                cancel,
                Arc::new(LiveGuard(self.parent.gate.clone())),
            ),
            ordinal,
            owner,
        })
    }
    pub(crate) fn revoke(&mut self) -> Result<()> {
        let mut db = authority::connect(&self.parent.root)?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(err)?;
        self.parent.check_record(&tx, true)?;
        let mut current = load(&tx, &self.record.id)?;
        if current != self.record {
            return Err("HTTP revoke binding drift".into());
        }
        current.state = 3;
        save(&tx, &current)?;
        tx.commit().map_err(err)?;
        self.record = current;
        Ok(())
    }
    pub(crate) fn observe(&mut self, head: &wire::Head, body: &[u8]) -> Result<()> {
        if body.len() > self.record.response_limit as usize {
            return Err("response evidence quota".into());
        }
        let record = self.claimed.as_ref().ok_or("no dispatched intent")?;
        let outcome = io::HttpOutcome {
            status: io::Status::Completed,
            http_status: head.status,
            headers: head
                .headers
                .iter()
                .map(|h| io::Header {
                    name: h.name.clone(),
                    value: h.value.clone(),
                })
                .collect(),
            body: body.to_vec(),
        };
        let bytes = io::Response::encode_http(&self.request, &outcome).map_err(err)?;
        let material = Material::encode(
            Kind::Response,
            &self.command.operation_id,
            &self.command.subject,
            self.request.digest(),
            &bytes,
        )
        .map_err(err)?;
        let db = authority::connect(&self.parent.root)?;
        self.parent.check_record(&db, true)?;
        let actual = load(&db, &self.record.id)?;
        let mut expected = self.record.clone();
        if actual.state == 3 {
            expected.state = 3;
        }
        if actual != expected || !matches!(actual.state, 2 | 3) {
            return Err("HTTP evidence binding drift".into());
        }
        let mut store = self.parent.store.lock().map_err(|_| "Store poison")?;
        store
            .store_io_material(&self.command.subject, Kind::Response, &material, || Ok(()))
            .map_err(err)?;
        let observed = record
            .propose_observation(material.digest(), ObservationSource::OriginalResponse)
            .map_err(err)?;
        store
            .append_io_intent_local_authorized(&observed, || Ok(()))
            .map_err(err)?;
        self.claimed = Some(observed);
        Ok(())
    }
}
struct LiveGuard(Gate);
#[cfg(test)]
#[path = "http_authority_tests.rs"]
mod http_api_tests;
impl StreamGuard for LiveGuard {
    fn check(&self) -> morrow_network_node_stream::Result<()> {
        let g = self
            .0
            .lock()
            .map_err(|_| morrow_network_node_stream::Error::Cancelled)?;
        if g.revoked {
            Err(morrow_network_node_stream::Error::Cancelled)
        } else if Instant::now() >= g.deadline {
            Err(morrow_network_node_stream::Error::Timeout)
        } else {
            Ok(())
        }
    }
}
