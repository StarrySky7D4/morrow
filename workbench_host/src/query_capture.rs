//! Durable query intent and successful actual-run archive. Ready records a committed
//! result, not UI delivery. A lost snapshot is interrupted, never resumed on new data.
use crate::{Result, Workbench, command, now, query_plan};
use morrow_core::{
    lifecycle::{GrantKind, InstancePhase},
    read_archive::{Budget, Finish, Plan},
    read_capture::{Phase, State},
    store::{CardReadSnapshot, FrozenCard},
    task::{Invocation, Transform},
};
use morrow_workbench_plugin::{Action, Idea, Request, Response, codec};
use prost::Message;

pub(crate) mod proto {
    include!(concat!(
        env!("OUT_DIR"),
        "/morrow.workbench.query_capture.v1.rs"
    ));
}
pub(crate) const SUBJECT: &str = "morrow.workbench.query";
const CONTEXT: &str = "morrow.query.context.v1";
const RESULT: &str = "morrow.query.result.v1";
const TOTAL_FUEL: u64 = 10_000_000_000;

#[derive(Debug)]
pub struct QueryFailure {
    pub message: String,
    /// Only a confirmed terminal capture or immutable operation conflict sets this.
    pub terminal: bool,
    pub capacity: bool,
}
impl std::fmt::Display for QueryFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.message.fmt(f)
    }
}
impl std::error::Error for QueryFailure {}
fn failure(message: impl Into<String>, terminal: bool) -> Box<dyn std::error::Error> {
    Box::new(QueryFailure {
        message: message.into(),
        terminal,
        capacity: false,
    })
}
fn capacity_failure(terminal: bool) -> Box<dyn std::error::Error> {
    Box::new(QueryFailure {
        message: "查询历史容量已满；已有内容已保留，此版本尚不支持清理查询历史。".into(),
        terminal,
        capacity: true,
    })
}
fn part_type(phase: query_plan::Phase) -> &'static str {
    match phase {
        query_plan::Phase::Filter => "morrow.query.filter.v2",
        query_plan::Phase::SortRun => "morrow.query.sort.v2",
        query_plan::Phase::Merge => "morrow.query.merge.v2",
    }
}
struct Capture<'a> {
    host: &'a mut Workbench,
    snapshot: CardReadSnapshot,
    state: State,
    pending: std::vec::IntoIter<FrozenCard>,
    eof: bool,
    ordinal: u32,
    observations: u64,
    remaining: u64,
}
impl Capture<'_> {
    fn append(&mut self, kind: &str, raw: &[u8]) -> Result<()> {
        // Any uncertain write stops capture. The outer boundary resolves the durable
        // state before ending it, so no new source/ordinal is guessed after an error.
        self.state = self
            .host
            .host
            .local_mut()?
            .store_local_mut()
            .append_read_capture(
                SUBJECT,
                &self.state.plan().operation_id,
                &self.state.token(),
                self.ordinal,
                kind,
                raw,
            )?;
        self.ordinal = self
            .ordinal
            .checked_add(1)
            .ok_or("query ordinal exhausted")?;
        Ok(())
    }
}
impl query_plan::Backend for Capture<'_> {
    fn next_candidate(&mut self) -> Result<Option<Idea>> {
        loop {
            self.host.check_query_access()?;
            if let Some(entry) = self.pending.next() {
                let idea = if entry.card().summary().type_id == "org.morrow.idea" {
                    self.host.grant(entry.id(), GrantKind::ReadContent)?;
                    let start = self.host.start;
                    let result = self.host.host.local_mut()?.read_snapshot_content(
                        self.host
                            .pool
                            .root(self.host.plugin.as_ref().ok_or("plugin unavailable")?)?
                            .connection(),
                        &self.snapshot,
                        &entry,
                        || now(start),
                    );
                    self.host.revoke(entry.id(), GrantKind::ReadContent)?;
                    Some(Workbench::decode(&result?)?.idea)
                } else {
                    None
                };
                self.append(
                    "morrow.query.source-fact.v1",
                    &proto::SourceFact {
                        operation_id: entry.latest_operation_id().into(),
                        sequence: entry.latest_sequence(),
                        commit_sha256: entry.latest_commit_sha256().to_vec(),
                    }
                    .encode_to_vec(),
                )?;
                // Separate source facts keep a maximum-sized original card within one part.
                self.append("morrow.query.card.v1", entry.original_bytes())?;
                if idea.is_some() {
                    return Ok(idea);
                }
                continue;
            }
            if self.eof {
                return Ok(None);
            }
            let page = self.snapshot.next_page(32, 16 * 1024 * 1024)?;
            self.eof = page.done;
            self.pending = page.entries.into_iter();
        }
    }
    fn invoke(&mut self, phase: query_plan::Phase, request: Request) -> Result<Response> {
        self.host.check_query_access()?;
        self.host.counter = self
            .host
            .counter
            .checked_add(1)
            .ok_or("task counter exhausted")?;
        let input = Invocation::new_transform(
            &format!("query-{}", self.host.counter),
            Transform {
                handler: "workbench.command".into(),
                input_type: "morrow.workbench.request.v1".into(),
                output_type: "morrow.workbench.response.v1".into(),
                input: codec::encode_request(&request)?,
            },
        )?;
        let result = self.host.pool.record_transform_observation(
            self.host
                .manager
                .as_ref()
                .ok_or("plugin manager unavailable")?,
            self.host.host.local_mut()?,
            self.host.plugin.as_ref().ok_or("plugin unavailable")?,
            &input,
            self.remaining,
        );
        self.host.finish_stopped_session();
        let captured = result?;
        if captured.package_digest()
            != self
                .host
                .bundle
                .as_ref()
                .ok_or("plugin unavailable")?
                .digest()
        {
            return Err("query package changed".into());
        }
        let budget = captured
            .observation()
            .data()
            .budget
            .as_ref()
            .ok_or("missing query budget")?;
        let consumed = budget
            .fuel
            .checked_sub(captured.report().execution.fuel_remaining)
            .ok_or("invalid query fuel report")?;
        self.remaining = self
            .remaining
            .checked_sub(consumed)
            .ok_or("query total fuel exhausted")?;
        let output = captured
            .report()
            .output
            .as_ref()
            .ok_or("missing query output")?;
        if output.type_id != "morrow.workbench.response.v1" {
            return Err("query output type".into());
        }
        let response = codec::decode_response(&output.bytes)?;
        self.append(part_type(phase), captured.observation().raw())?;
        self.observations += 1;
        Ok(response)
    }
}
impl Workbench {
    pub(crate) fn check_query_access(&mut self) -> Result<()> {
        self.host.local()?;
        let manager = self.manager.as_ref().ok_or("plugin manager unavailable")?;
        self.pool.maintain(manager, self.host.local_mut()?)?;
        let root = self
            .pool
            .root(self.plugin.as_ref().ok_or("plugin unavailable")?)?;
        let package = self.bundle.as_ref().ok_or("plugin unavailable")?;
        let selection = manager
            .selection(&package.manifest().package_id)
            .ok_or("plugin unavailable")?;
        if !selection.enabled
            || selection.digest != package.digest()
            || !selection.approved.contains(&GrantKind::ReadContent)
            || self.host.local()?.connection_phase(root.connection())? != InstancePhase::Ready
        {
            return Err("query read capability unavailable".into());
        }
        Ok(())
    }
    pub(crate) fn recover_queries(&mut self) -> Result<()> {
        self.host.local()?;
        // Startup owns the application's library lease. No live in-process query exists.
        // Other capture subjects are not ours to interrupt.
        let mut after = String::new();
        loop {
            let states = self
                .host
                .local()?
                .store_local()
                .list_read_captures(&after, 128)?;
            if states.is_empty() {
                break;
            }
            for state in states {
                after = state.plan().operation_id.clone();
                if state.plan().subject == SUBJECT
                    && state.context_type() == CONTEXT
                    && state.phase() == Phase::Preparing
                {
                    self.host.local_mut()?.store_local_mut().end_read_capture(
                        SUBJECT,
                        &after,
                        &state.token(),
                        Phase::Interrupted,
                        "host_restarted_snapshot_unavailable",
                    )?;
                }
            }
        }
        Ok(())
    }
    pub fn query(
        &mut self,
        section: &str,
        filter: &str,
        text: &str,
        sort: &str,
    ) -> Result<Vec<String>> {
        self.host.local()?;
        let mut nonce = [0; 16];
        getrandom::fill(&mut nonce)?;
        let operation = format!("query-{:032x}", u128::from_le_bytes(nonce));
        self.query_with_operation(&operation, section, filter, text, sort)
    }
    pub fn query_with_operation(
        &mut self,
        operation: &str,
        section: &str,
        filter: &str,
        text: &str,
        sort: &str,
    ) -> Result<Vec<String>> {
        self.host.local()?;
        let mut request = command(Action::Query);
        request.section = section.into();
        request.filter = filter.into();
        request.text = text.into();
        request.sort = sort.into();
        let bytes = codec::encode_request(&request)?;
        if let Some(state) = self
            .host
            .local()?
            .store_local()
            .lookup_read_capture(SUBJECT, operation)?
        {
            if state.plan().request != bytes
                || state.context_type() != CONTEXT
                || state.plan().request_type != "morrow.workbench.request.v1"
                || state.plan().response_type != RESULT
            {
                return Err(failure("查询标识已绑定其他条件，请重新筛选。", true));
            }
            match state.phase() {
                Phase::Ready => return self.deliver_query(&state),
                Phase::Preparing => {
                    // Synchronous host cannot have another active query on this Workbench.
                    // An earlier unwound call lost its owned snapshot; never rebind it.
                    self.host.local_mut()?.store_local_mut().end_read_capture(
                        SUBJECT,
                        operation,
                        &state.token(),
                        Phase::Interrupted,
                        "snapshot_unavailable_on_retry",
                    )?;
                    return Err(failure("此前筛选已中断，请重新筛选。", true));
                }
                _ if state.reason() == "query_archive_capacity" => {
                    return Err(capacity_failure(true));
                }
                _ => return Err(failure("此次筛选已终止，请重新筛选。", true)),
            }
        }
        let conditions = query_plan::Conditions {
            section: section.into(),
            filter: filter.into(),
            text: text.into(),
            sort: sort.into(),
        };
        let result = self.capture_query(operation, bytes, &conditions);
        match result {
            Ok(ids) => Ok(ids),
            Err(error) => {
                let capacity = error.downcast_ref::<morrow_core::Error>()
                    == Some(&morrow_core::Error::ArchiveCapacity);
                // Resolve even CommitUnknown before deciding whether a fresh ID is safe.
                // Ready is never demoted because transport/audit flush/permission failed.
                let lookup = self
                    .host
                    .local()?
                    .store_local()
                    .lookup_read_capture(SUBJECT, operation);
                let terminal = match lookup {
                    Ok(Some(state))
                        if state.phase() == Phase::Preparing
                            && state.owner() == self.query_owner =>
                    {
                        self.host
                            .local_mut()?
                            .store_local_mut()
                            .end_read_capture(
                                SUBJECT,
                                operation,
                                &state.token(),
                                Phase::Failed,
                                if capacity {
                                    "query_archive_capacity"
                                } else {
                                    "query_execution_failed"
                                },
                            )
                            .is_ok()
                    }
                    Ok(Some(state)) => matches!(
                        state.phase(),
                        Phase::Failed | Phase::Cancelled | Phase::Interrupted
                    ),
                    Ok(None) if capacity => true,
                    Ok(None)
                        if error.downcast_ref::<morrow_core::Error>()
                            == Some(&morrow_core::Error::OperationConflict) =>
                    {
                        true
                    }
                    _ => false,
                };
                if capacity {
                    Err(capacity_failure(terminal))
                } else {
                    Err(failure(error.to_string(), terminal))
                }
            }
        }
    }
    fn capture_query(
        &mut self,
        operation: &str,
        request: Vec<u8>,
        conditions: &query_plan::Conditions,
    ) -> Result<Vec<String>> {
        self.host.prepare_write()?;
        self.check_query_access()?;
        let snapshot = self.host.local()?.store_local().open_card_snapshot()?;
        self.host
            .local()?
            .store_local()
            .validate_card_snapshot(&snapshot)?;
        let point = snapshot.readpoint().clone();
        let package = self.bundle.as_ref().ok_or("plugin unavailable")?;
        let digest = package.digest();
        let archive = package.archive().to_vec();
        let context = proto::CaptureContext {
            schema_version: 1,
            plan_version: query_plan::VERSION,
            database_version: point.database_version,
            operation_sequence: point.operation_sequence,
            operation_sha256: point.operation_sha256.map(|v| v.to_vec()),
            package_sha256: digest.to_vec(),
            fuel_budget: TOTAL_FUEL,
        };
        let state = self
            .host
            .local_mut()?
            .store_local_mut()
            .begin_read_capture(
                &Plan {
                    operation_id: operation.into(),
                    subject: SUBJECT.into(),
                    request_type: "morrow.workbench.request.v1".into(),
                    request,
                    response_type: RESULT.into(),
                    budget: Budget {
                        max_parts: 65536,
                        max_bytes: 4 * 1024 * 1024 * 1024,
                    },
                },
                CONTEXT,
                &context.encode_to_vec(),
                self.query_owner,
            )?;
        let mut capture = Capture {
            host: self,
            snapshot,
            state,
            pending: vec![].into_iter(),
            eof: false,
            ordinal: 0,
            observations: 0,
            remaining: TOTAL_FUEL,
        };
        capture.append("morrow.query.package.v1", &archive)?;
        let ids = query_plan::execute(conditions, &mut capture)?;
        let census = capture.snapshot.finish()?;
        let observations = capture.observations;
        let state = capture.state;
        capture.snapshot.close()?;
        self.check_query_access()?;
        let status = self
            .host
            .local()?
            .store_local()
            .lookup_read_archive(SUBJECT, operation)?
            .ok_or("missing query archive")?;
        let end = proto::CaptureEnd {
            schema_version: 1,
            plan_version: query_plan::VERSION,
            database_version: point.database_version,
            operation_sequence: point.operation_sequence,
            operation_sha256: point.operation_sha256.map(|v| v.to_vec()),
            source_count: census.count,
            source_sha256: census.sha256.to_vec(),
            package_sha256: digest.to_vec(),
            fuel_budget: TOTAL_FUEL,
            observation_count: observations,
        };
        let manager = self.manager.as_ref().ok_or("plugin manager unavailable")?;
        let pool = &self.pool;
        let session = self.plugin.as_ref().ok_or("plugin unavailable")?;
        let (ready, _) = self
            .host
            .local_mut()?
            .store_local_mut()
            .finish_read_capture_local_authorized(
                SUBJECT,
                operation,
                &state.token(),
                &Finish {
                    response: proto::QueryResult {
                        schema_version: 1,
                        ids,
                    }
                    .encode_to_vec(),
                    part_count: status.count,
                    logical_bytes: status.logical_bytes,
                    chain_sha256: status.chain_sha256,
                    metadata_type: "morrow.query.end.v1".into(),
                    metadata: end.encode_to_vec(),
                },
                &[],
                || {
                    let selected = manager
                        .selection("org.morrow.workbench")
                        .ok_or(morrow_core::Error::Integrity)?;
                    if !selected.enabled
                        || selected.digest != digest
                        || !selected.approved.contains(&GrantKind::ReadContent)
                        || pool
                            .root(session)
                            .map_err(|_| morrow_core::Error::Integrity)?
                            .package()
                            .package()
                            .digest()
                            != digest
                    {
                        return Err(morrow_core::Error::Integrity);
                    }
                    Ok(())
                },
            )?;
        self.host.finish_maintenance()?;
        self.deliver_query(&ready)
    }
    fn deliver_query(&mut self, state: &State) -> Result<Vec<String>> {
        // Current selection/approval controls every delivery; original ownership is
        // durable identity only and never reinstates permissions from an old session.
        self.check_query_access()?;
        let read = self
            .host
            .local()?
            .store_local()
            .lookup_read(SUBJECT, &state.plan().operation_id)?
            .ok_or("missing committed query")?;
        if Some(read.data().archive_sha256.as_slice())
            != state.archive_root().as_ref().map(|v| v.as_slice())
            || read.data().request != state.plan().request
        {
            return Err("query committed identity mismatch".into());
        }
        let raw = read.data().response.as_slice();
        validate_result_wire(raw)?;
        let result = proto::QueryResult::decode(raw)?;
        if result.schema_version != 1 {
            return Err("query result version".into());
        }
        self.check_query_access()?;
        Ok(result.ids)
    }
}
// Preflight before prost's repeated-string allocation. Retained results are bounded
// separately from the smaller final UI frame, whose failure cannot undo Ready.
fn validate_result_wire(mut raw: &[u8]) -> Result<()> {
    use prost::encoding::{WireType, decode_key, decode_varint};
    if raw.len() > 4 * 1024 * 1024 {
        return Err("query result byte budget".into());
    }
    let mut ids = 0;
    let mut version = false;
    while !raw.is_empty() {
        let (field, wire) = decode_key(&mut raw)?;
        match (field, wire) {
            (1, WireType::Varint) if !version => {
                version = true;
                if decode_varint(&mut raw)? != 1 {
                    return Err("query result version".into());
                }
            }
            (2, WireType::LengthDelimited) => {
                ids += 1;
                let len = usize::try_from(decode_varint(&mut raw)?)?;
                if ids > 4096 || len == 0 || len > 256 || len > raw.len() {
                    return Err("query result ID budget".into());
                }
                raw = &raw[len..];
            }
            _ => return Err("query result wire".into()),
        }
    }
    if !version {
        return Err("missing query result version".into());
    }
    Ok(())
}
#[cfg(test)]
mod tests;
