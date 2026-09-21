//! Private binary UI transport. The child process and all paths are selected by
//! the trusted Flutter host; plugins receive only the registered business task.
use crate::{Result, Workbench, WorkbenchState, host_capnp as wire};
use capnp::{
    message::{Builder, ReaderOptions},
    serialize,
};
use morrow_workbench_plugin::{Action, Response, codec};
use sha2::{Digest, Sha256};
use zeroize::{Zeroize, Zeroizing};
#[path = "service_run_protocol.rs"]
mod service_run;
pub fn digest() -> [u8; 32] {
    Sha256::digest(
        include_str!("../schemas/host.capnp")
            .replace("\r\n", "\n")
            .as_bytes(),
    )
    .into()
}
fn text(v: capnp::Result<capnp::text::Reader<'_>>) -> Result<String> {
    Ok(v?.to_str()?.to_owned())
}
fn io_error(value: std::result::Result<(), morrow_plugin_runtime::io_jobs::JobError>) -> u16 {
    use morrow_plugin_runtime::io_jobs::JobError::*;
    match value {
        Ok(()) => 0,
        Err(InvalidOptions) => 1,
        Err(Busy) => 2,
        Err(Closed) => 3,
        Err(Unavailable) => 4,
        Err(Consumed) => 5,
        Err(ReadBound) => 6,
        Err(Limit) => 7,
        Err(Spawn) => 8,
        Err(Disconnect) => 9,
    }
}
fn io_state_reply(
    value: &crate::io_tasks::Snapshot,
    submission: Option<[u8; 32]>,
    mut out: wire::io_state::Builder<'_>,
) {
    use crate::io_tasks::StoragePhase;
    use morrow_plugin_runtime::io_jobs::Poll;
    if let Some(key) = value.key {
        out.set_key(key.as_bytes());
    }
    if let Some(submission) = submission {
        out.set_submission(&submission);
    }
    out.set_storage(match value.storage {
        StoragePhase::Local => 0,
        StoragePhase::Running => 1,
        StoragePhase::Stopping => 2,
        StoragePhase::Reclaimed => 3,
        StoragePhase::RecoveryRequired => 4,
        StoragePhase::Unavailable => 5,
    });
    out.set_delivery(match value.delivery {
        None => 0,
        Some(Poll::Pending) => 1,
        Some(Poll::Ready) => 2,
        Some(Poll::Consumed) => 3,
        Some(Poll::Unavailable) => 4,
    });
    if let Some(exit) = value.exit {
        out.set_has_exit(true);
        out.set_execution(io_error(exit.execution));
        out.set_disconnect(io_error(exit.disconnect));
        out.set_maintenance(io_error(exit.maintenance));
    }
}
fn io_result_reply(
    value: &morrow_plugin_runtime::io_jobs::JobReport,
    mut out: wire::io_result::Builder<'_>,
) -> Result<()> {
    use morrow_plugin_runtime::Fault;
    out.set_present(true);
    out.set_cancelled(value.cancelled);
    out.set_unknown(value.unknown);
    out.set_calls(value.calls);
    out.set_charged_bytes(value.bytes);
    match &value.task.execution.outcome {
        Ok(code) => out.set_exit_code(*code),
        Err(fault) => out.set_execution_fault(match fault {
            Fault::TaskProtocol => 1,
            Fault::Deadline => 2,
            Fault::PackageBinding => 3,
            Fault::InactiveConnection => 4,
            Fault::InvalidModule => 5,
            Fault::UnsupportedAbi => 6,
            Fault::Limits => 7,
            Fault::Cancelled => 8,
            Fault::Trap => 9,
        }),
    }
    if let Some(http) = &value.http_response {
        out.set_has_http(true);
        out.set_status(http.status as u16);
        out.set_http_status(http.http_status);
        out.set_body(&http.body);
        let mut headers = out.init_headers(http.headers.len().try_into()?);
        for (i, h) in http.headers.iter().enumerate() {
            let mut item = headers.reborrow().get(i as u32);
            item.set_name(h.name.as_str());
            item.set_value(&h.value);
        }
    }
    Ok(())
}
trait ResponseTarget {
    fn response_budget(&self, _action: wire::Action) -> usize {
        128 * 1024
    }
    fn dispatch(
        &mut self,
        request: wire::request::Reader<'_>,
        out: wire::response::Builder<'_>,
    ) -> Result<()>;
    fn writable(&self) -> bool;
    fn warning(&self) -> Option<&str>;
}

pub fn respond(host: &mut Workbench, bytes: &[u8]) -> Result<Vec<u8>> {
    respond_target(host, bytes)
}

/// The original owner executes business commands while a worker holds it.
/// Scheduler commands are unavailable here, including nested HttpStart.
pub(crate) fn respond_state(host: &mut WorkbenchState, bytes: &[u8]) -> Result<Vec<u8>> {
    respond_target(host, bytes)
}

fn is_scheduler_action(action: wire::Action) -> bool {
    service_run::is_action(action)
        || matches!(
            action,
            wire::Action::HttpStart
                | wire::Action::IoStatus
                | wire::Action::IoPoll
                | wire::Action::IoRead
                | wire::Action::IoCancel
                | wire::Action::IoRepair
                | wire::Action::IoAcknowledge
        )
}

fn requires_writable_state(action: wire::Action) -> bool {
    // These routes previously borrowed only &WorkbenchState. Administrative pages and
    // preference downloads mutate authority/transfer state and intentionally stay gated.
    !matches!(
        action,
        wire::Action::Read
            | wire::Action::Page
            | wire::Action::ExportFile
            | wire::Action::BackupSnapshot
            | wire::Action::BackupProtection
            | wire::Action::PluginCatalog
            | wire::Action::PluginInspect
            | wire::Action::ReadUiLocale
    )
}

impl ResponseTarget for Workbench {
    fn response_budget(&self, action: wire::Action) -> usize {
        if action == wire::Action::CommandRead {
            256 * 1024
        } else {
            128 * 1024
        }
    }
    fn writable(&self) -> bool {
        Workbench::writable(self)
    }
    fn warning(&self) -> Option<&str> {
        self.maintenance_warning()
    }
    fn dispatch(
        &mut self,
        r: wire::request::Reader<'_>,
        mut out: wire::response::Builder<'_>,
    ) -> Result<()> {
        let action = r.get_action()?;
        if is_command_frame(action) {
            return Err("frame staging requires the original owner command lane".into());
        }
        // Parse and validate first. Only an actually completed worker can return
        // the original state; malformed frames do not trigger recovery work.
        let _ = self.state.try_reclaim();
        if !is_scheduler_action(action) {
            // Keep this gate outside service error redaction so Busy/RecoveryRequired survive.
            let state = if requires_writable_state(action) {
                self.local_state_mut()?
            } else {
                self.state.local_mut()?
            };
            return handle_business(state, r, out);
        }
        if service_run::is_action(action) {
            return service_run::handle(self, r, out);
        }
        let host = self;
        if action == wire::Action::HttpStart {
            host.local_state_mut()?;
        }
        match action {
            wire::Action::HttpStart => {
                let h = r.get_http_start()?;
                let headers = h.get_headers()?;
                if headers.len() > 64 {
                    return Err("HTTP header count".into());
                }
                host.start_http(crate::http_tasks::HttpStart {
                    submission: h.get_submission()?.try_into()?,
                    endpoint: h.get_endpoint()?.try_into()?,
                    endpoint_revision: h.get_endpoint_revision(),
                    package_digest: h.get_package_digest()?.try_into()?,
                    registry_revision: h.get_registry_revision(),
                    method: text(h.get_method())?,
                    target: text(h.get_target())?,
                    headers: headers
                        .iter()
                        .map(|v| {
                            Ok(morrow_core::io::Header {
                                name: text(v.get_name())?,
                                value: v.get_value()?.to_vec(),
                            })
                        })
                        .collect::<Result<_>>()?,
                    body: h.get_body()?.to_vec(),
                    timeout_ms: u64::from(h.get_timeout_ms()),
                })?;
                io_state_reply(
                    &host.io_status(),
                    host.http_submission(),
                    out.reborrow().init_io_state(),
                );
            }
            wire::Action::IoStatus => {
                io_state_reply(
                    &host.io_status(),
                    host.http_submission(),
                    out.reborrow().init_io_state(),
                );
            }
            wire::Action::IoPoll
            | wire::Action::IoCancel
            | wire::Action::IoRepair
            | wire::Action::IoAcknowledge => {
                let key = crate::io_tasks::TaskKey::from_bytes(r.get_io_key()?)?;
                let status = match action {
                    wire::Action::IoCancel => host.cancel_io(key)?,
                    wire::Action::IoRepair => host.repair_io(key)?,
                    wire::Action::IoAcknowledge => {
                        host.acknowledge_io(key)?;
                        host.io_status()
                    }
                    _ => host.poll_io(key)?,
                };
                io_state_reply(
                    &status,
                    host.http_submission(),
                    out.reborrow().init_io_state(),
                );
            }
            wire::Action::IoRead => {
                let key = crate::io_tasks::TaskKey::from_bytes(r.get_io_key()?)?;
                // Worker-owned reports are bounded; expose only the single HTTP
                // outcome, never duplicate guest completion or unbounded diagnostics.
                let report = host.read_io(key, 4 * 1024 * 1024)?;
                if let Some(report) = report {
                    io_result_reply(&report, out.reborrow().init_io_result())?;
                }
                io_state_reply(
                    &host.io_status(),
                    host.http_submission(),
                    out.reborrow().init_io_state(),
                );
            }
            _ => return Err("not a scheduler action".into()),
        }
        Ok(())
    }
}

impl ResponseTarget for WorkbenchState {
    fn writable(&self) -> bool {
        WorkbenchState::writable(self)
    }
    fn warning(&self) -> Option<&str> {
        self.maintenance_warning()
    }
    fn dispatch(
        &mut self,
        r: wire::request::Reader<'_>,
        out: wire::response::Builder<'_>,
    ) -> Result<()> {
        handle_business(self, r, out)
    }
}
fn respond_target(host: &mut impl ResponseTarget, bytes: &[u8]) -> Result<Vec<u8>> {
    let mut output = Builder::new_default();
    let mut response_budget = 128 * 1024;
    let failure = {
        let mut out = output.init_root::<wire::response::Builder>();
        out.set_version(1);
        out.set_digest(&digest());
        match handle(host, bytes, out.reborrow()) {
            Ok(budget) => {
                response_budget = budget;
                None
            }
            Err(error) => Some(error),
        }
    };
    if let Some(e) = failure {
        // Never serialize a partial success together with an error. In
        // particular, a one-time token must not survive in abandoned segments.
        wipe_sensitive_reply(&mut output)?;
        output = Builder::new_default();
        let mut out = output.init_root::<wire::response::Builder>();
        out.set_version(1);
        out.set_digest(&digest());
        if let Some(access) = e.downcast_ref::<crate::io_tasks::AccessError>() {
            out.set_ui_code(match access {
                crate::io_tasks::AccessError::Busy => 110,
                crate::io_tasks::AccessError::RecoveryRequired => 111,
                crate::io_tasks::AccessError::OwnerUnavailable => 112,
                crate::io_tasks::AccessError::StaleTask => 113,
                crate::io_tasks::AccessError::UnacknowledgedTask => 114,
            });
        }
        if let Some(query) = e.downcast_ref::<crate::query_capture::QueryFailure>() {
            out.set_ui_code(match (query.capacity, query.terminal) {
                (true, true) => 101,
                (true, false) => 102,
                (false, true) => 100,
                (false, false) => 0,
            });
        }
        out.set_error(e.to_string().as_str());
    }
    {
        let mut out = output.get_root::<wire::response::Builder>()?;
        out.set_read_only(!host.writable());
        if let Some(warning) = host.warning() {
            out.set_maintenance_warning(warning);
        }
    }
    // The CLI immediately takes ownership in another Zeroizing buffer. Keep
    // this intermediate owner protected too, including oversized replies.
    let mut bytes = Zeroizing::new(serialize::write_message_to_words(&output));
    wipe_sensitive_reply(&mut output)?;
    if bytes.len() > response_budget {
        // A fresh message contains neither partial payload nor hidden segments.
        // The request may already have executed; this is a delivery error only.
        let mut bounded = Builder::new_default();
        let mut error = bounded.init_root::<wire::response::Builder>();
        error.set_version(1);
        error.set_digest(&digest());
        error.set_read_only(!host.writable());
        error.set_error("response exceeds 128 KiB frame budget; no result delivered; verify any operation before retrying");
        return Ok(serialize::write_message_to_words(&bounded));
    }
    Ok(std::mem::take(&mut *bytes))
}
fn wipe_sensitive_reply(output: &mut Builder<capnp::message::HeapAllocator>) -> Result<()> {
    let mut out = output.get_root::<wire::response::Builder>()?;
    out.reborrow().get_issued_token()?.zeroize();
    // A nested business response can itself carry a one-time credential.
    out.reborrow().get_payload()?.zeroize();
    Ok(())
}
fn validated_request(
    bytes: &[u8],
    max_bytes: usize,
) -> Result<capnp::message::Reader<serialize::BufferSegments<&[u8]>>> {
    if bytes.is_empty() || bytes.len() > max_bytes {
        return Err("host frame budget".into());
    }
    // Borrow the caller-owned frame; do not copy plaintext credential input into
    // a second owned segment allocation. The native input owner wipes its frame.
    let mut remaining = bytes;
    let message = serialize::read_message_from_flat_slice(
        &mut remaining,
        ReaderOptions {
            traversal_limit_in_words: Some(32768),
            nesting_limit: 20,
        },
    )?;
    if !remaining.is_empty() {
        return Err("trailing host bytes".into());
    }
    let r = message.get_root::<wire::request::Reader>()?;
    if r.get_version() != 1 || r.get_digest()? != digest() {
        return Err("host contract mismatch".into());
    }
    r.get_action()?;
    Ok(message)
}
fn is_command_frame(action: wire::Action) -> bool {
    matches!(
        action,
        wire::Action::CommandFrameBegin
            | wire::Action::CommandFrameAppend
            | wire::Action::CommandFrameFinish
            | wire::Action::CommandFrameAbort
    )
}
fn validate_command_frame(bytes: &[u8]) -> Result<()> {
    let message = validated_request(bytes, 64 * 1024)?;
    if is_scheduler_action(message.get_root::<wire::request::Reader>()?.get_action()?) {
        return Err("scheduler actions require the outer application".into());
    }
    Ok(())
}
fn handle(
    host: &mut impl ResponseTarget,
    bytes: &[u8],
    out: wire::response::Builder<'_>,
) -> Result<usize> {
    let message = validated_request(bytes, 128 * 1024)?;
    let r = message.get_root::<wire::request::Reader>()?;
    let budget = host.response_budget(r.get_action()?);
    host.dispatch(r, out)?;
    Ok(budget)
}

fn handle_business(
    host: &mut WorkbenchState,
    r: wire::request::Reader<'_>,
    mut out: wire::response::Builder<'_>,
) -> Result<()> {
    let action = r.get_action()?;
    // This owner already runs inside a worker; scheduling here would recursively move it.
    if is_scheduler_action(action) {
        return Err("scheduler actions require the outer application".into());
    }
    if is_command_frame(action) {
        let token = text(r.get_transfer())?;
        let now = std::time::Instant::now();
        match action {
            wire::Action::CommandFrameBegin => {
                host.command_frame
                    .begin(&token, r.get_total_length(), r.get_sha256()?, now)?;
                out.set_offset(0);
            }
            wire::Action::CommandFrameAppend => {
                out.set_offset(host.command_frame.append(
                    &token,
                    r.get_offset(),
                    r.get_payload()?,
                    now,
                )? as u64);
            }
            wire::Action::CommandFrameFinish => {
                let bytes =
                    host.command_frame
                        .take(&token, r.get_total_length(), r.get_sha256()?, now)?;
                let message = validated_request(&bytes, 128 * 1024)?;
                let inner = message.get_root::<wire::request::Reader>()?;
                let action = inner.get_action()?;
                if is_scheduler_action(action) || is_command_frame(action) {
                    return Err("nested scheduling or staging is forbidden".into());
                }
                return handle_business(host, inner, out);
            }
            wire::Action::CommandFrameAbort => host.command_frame.abort(&token),
            _ => unreachable!(),
        }
        return Ok(());
    }
    let id = if crate::service_protocol::is_action(action) {
        String::new()
    } else {
        text(r.get_id())?
    };
    match action {
        wire::Action::ServiceConfigPage
        | wire::Action::ServiceConfigSave
        | wire::Action::ServiceConfigDisable
        | wire::Action::ServiceAuthorityPage
        | wire::Action::ServiceAuthenticationIssue
        | wire::Action::ServiceAuthorityDisable
        | wire::Action::ServicePublicationSave => {
            // Service errors can contain host paths. Expose no raw backend
            // details; the shared access gate above still preserves Busy codes.
            crate::service_protocol::handle(host, r, out.reborrow())
                .map_err(|_| "service administration request could not be completed")?;
        }
        wire::Action::HttpStart
        | wire::Action::IoStatus
        | wire::Action::IoPoll
        | wire::Action::IoRead
        | wire::Action::IoCancel
        | wire::Action::IoRepair
        | wire::Action::IoAcknowledge
        | wire::Action::ServiceRunStart
        | wire::Action::ServiceRunStatus
        | wire::Action::CommandSubmit
        | wire::Action::CommandStatus
        | wire::Action::CommandRead
        | wire::Action::CommandCancel => {
            return Err("scheduler actions require the outer application".into());
        }
        wire::Action::EndpointPage => {
            let page = host.endpoint_page(r.get_endpoint_cursor()?, r.get_endpoint_snapshot()?)?;
            out.set_endpoint_snapshot(&page.snapshot);
            out.set_endpoint_cursor(page.next.as_ref().map_or(&[], |v| v.as_slice()));
            let mut entries = out.reborrow().init_endpoints(page.entries.len() as u32);
            for (i, entry) in page.entries.iter().enumerate() {
                endpoint_reply(entry, entries.reborrow().get(i as u32))?;
            }
        }
        wire::Action::EndpointSave => {
            let entry = host.save_endpoint(crate::endpoint_control::EndpointUpdate {
                reference: r.get_endpoint_reference()?.to_vec(),
                expected_revision: r.get_revision(),
                registry_revision: r.get_endpoint_registry_revision(),
                lifetime_days: r.get_endpoint_days(),
                policy: endpoint_policy(r.get_endpoint_policy()?)?,
            })?;
            endpoint_reply(&entry, out.reborrow().init_endpoints(1).get(0))?;
        }
        wire::Action::EndpointDisable => {
            let entry = host.disable_endpoint(r.get_endpoint_reference()?, r.get_revision())?;
            endpoint_reply(&entry, out.reborrow().init_endpoints(1).get(0))?;
        }
        wire::Action::CredentialPage => {
            let page =
                host.credential_page(r.get_credential_cursor()?, r.get_credential_snapshot()?)?;
            out.set_credential_snapshot(&page.snapshot);
            out.set_credential_cursor(page.next.as_ref().map_or(&[], |v| v.as_slice()));
            let mut entries = out.reborrow().init_credentials(page.entries.len() as u32);
            for (i, entry) in page.entries.iter().enumerate() {
                credential_reply(entry, entries.reborrow().get(i as u32));
            }
        }
        wire::Action::CredentialSave => {
            let entry = host.save_credential(
                r.get_credential_reference()?,
                r.get_revision(),
                r.get_credential_header()?.to_str()?,
                r.get_credential_secret()?.to_str()?,
                r.get_credential_days(),
            )?;
            credential_reply(&entry, out.reborrow().init_credentials(1).get(0));
        }
        wire::Action::CredentialDisable => {
            let entry = host.disable_credential(r.get_credential_reference()?, r.get_revision())?;
            credential_reply(&entry, out.reborrow().init_credentials(1).get(0));
        }
        wire::Action::PluginCatalog => {
            let expected = r.get_catalog_revision_bound().then_some(r.get_revision());
            plugin_catalog_reply(
                host.catalog_page(&text(r.get_cursor())?, expected)?,
                out.reborrow(),
            );
        }
        wire::Action::ReadUiLocale => {
            let (locale, revision) = host.read_ui_locale()?;
            out.set_payload(locale.as_bytes());
            out.set_revision(revision);
        }
        wire::Action::SaveUiLocale => {
            let locale = std::str::from_utf8(r.get_payload()?)?;
            let revision =
                host.save_ui_locale(&text(r.get_operation())?, r.get_revision(), locale)?;
            out.set_payload(locale.as_bytes());
            out.set_revision(revision);
        }
        wire::Action::PluginInspect => {
            plugin_catalog_reply(
                host.inspect_plugin(std::path::Path::new(&text(r.get_selected_path())?))?,
                out.reborrow(),
            );
        }
        wire::Action::PluginImport => {
            host.import_plugin(
                std::path::Path::new(&text(r.get_selected_path())?),
                r.get_sha256()?,
                r.get_revision(),
            )?;
        }
        wire::Action::PluginApprove => {
            if r.get_limit() > 1 {
                return Err("invalid enable decision".into());
            }
            let values = r.get_approved_capabilities()?;
            if values.len() > 7 {
                return Err("capability decision budget".into());
            }
            let approved = values.iter().map(text).collect::<Result<Vec<_>>>()?;
            host.configure_external(
                &id,
                r.get_sha256()?,
                r.get_revision(),
                &approved,
                r.get_limit() == 1,
            )?;
        }
        wire::Action::PluginApproveIo => {
            let values = r.get_approved_io_capabilities()?;
            if values.len() > 10 {
                return Err("IO capability decision budget".into());
            }
            let approved = values.iter().map(text).collect::<Result<Vec<_>>>()?;
            host.configure_external_io(&id, r.get_sha256()?, r.get_revision(), &approved)?;
        }
        wire::Action::PluginRemove => {
            host.remove_external(&id, r.get_sha256()?, r.get_revision())?;
        }
        wire::Action::PluginTransform => {
            out.set_payload(&host.run_external_transform(
                &id,
                r.get_sha256()?,
                r.get_revision(),
                &text(r.get_handler())?,
                &text(r.get_input_type())?,
                &text(r.get_output_type())?,
                r.get_payload()?,
            )?);
        }
        wire::Action::ExternalUiOpen => {
            ui_reply(
                host.external_ui_open(
                    &id,
                    r.get_sha256()?,
                    r.get_revision(),
                    &text(r.get_name())?,
                )?,
                out.reborrow(),
            );
        }
        wire::Action::ExternalUiEvent => {
            ui_reply(
                host.external_ui_event(&id, r.get_offset(), r.get_payload()?)?,
                out.reborrow(),
            );
        }
        wire::Action::ExternalUiClose => {
            host.external_ui_close(&id, r.get_offset())?;
        }
        wire::Action::PluginState => {
            host.refresh_plugin_state()?;
            plugin_status(host, out.reborrow());
        }
        wire::Action::PluginConfigure => {
            if r.get_limit() > 1 {
                return Err("invalid enable decision".into());
            }
            host.configure_plugin(r.get_revision(), r.get_sha256()?, r.get_limit() == 1)?;
            plugin_status(host, out.reborrow());
        }
        wire::Action::UiOpen => {
            ui_reply(host.ui_open(&text(r.get_name())?)?, out.reborrow());
        }
        wire::Action::UiEvent => {
            ui_reply(
                host.ui_event(r.get_offset(), r.get_payload()?)?,
                out.reborrow(),
            );
        }
        wire::Action::UiClose => {
            host.ui_close(r.get_offset());
        }

        wire::Action::BackupSnapshot => {
            host.backup_snapshot(std::path::Path::new(&text(r.get_selected_path())?))?;
        }
        wire::Action::BackupProtection => {
            host.backup_key(std::path::Path::new(&text(r.get_selected_path())?))?;
        }
        wire::Action::Read => {
            let record = host.read(&id)?;
            out.set_revision(record.revision);
            out.set_payload(&codec::encode_response(&Response {
                idea: record.idea,
                ids: vec![],
            })?);
        }
        wire::Action::Page => {
            let (records, cursor) = host.page(&text(r.get_cursor())?, r.get_limit())?;
            out.set_cursor(cursor.as_str());
            let mut ids = out.reborrow().init_ids(records.len() as u32);
            for (i, record) in records.iter().enumerate() {
                ids.set(i as u32, record.idea.id.as_str());
            }
        }
        wire::Action::Mutate => {
            if !text(r.get_capture_scope())?.is_empty() {
                return Err("captured saves require their complete editor metadata".into());
            }
            let req = codec::decode_request(r.get_payload()?)?;
            let operation = text(r.get_operation())?;
            let record = if req.action == Action::Create {
                if req.proposed.id != id || r.get_revision() != 0 {
                    return Err("create target".into());
                }
                host.create(&operation, req.proposed)?
            } else {
                host.apply(crate::Mutation {
                    operation: &operation,
                    id: &id,
                    revision: r.get_revision(),
                    action: req.action,
                    proposed: Some(req.proposed),
                    text: &req.text,
                    flag: req.flag,
                })?
            };
            out.set_revision(record.revision);
            out.set_payload(&codec::encode_response(&Response {
                idea: record.idea,
                ids: vec![],
            })?);
        }
        wire::Action::Query => {
            let req = codec::decode_request(r.get_payload()?)?;
            if req.action != Action::Query {
                return Err("query route".into());
            }
            let operation = text(r.get_operation())?;
            let result = if operation.is_empty() {
                host.query(&req.section, &req.filter, &req.text, &req.sort)?
            } else {
                host.query_with_operation(
                    &operation,
                    &req.section,
                    &req.filter,
                    &req.text,
                    &req.sort,
                )?
            };
            if result.len() > 4096 {
                return Err("query response pagination required".into());
            }
            let mut ids = out.reborrow().init_ids(result.len() as u32);
            for (i, id) in result.iter().enumerate() {
                ids.set(i as u32, id.as_str());
            }
        }
        wire::Action::ImportFile => {
            let path = text(r.get_selected_path())?;
            let mut file = std::fs::File::open(path)?;
            let meta = file.metadata()?;
            if !meta.is_file() {
                return Err("selected resource is not a file".into());
            }
            let asset = host.import(
                &id,
                &text(r.get_name())?,
                &text(r.get_kind())?,
                &mut file,
                meta.len(),
            )?;
            let mut idea = morrow_workbench_plugin::Idea::default();
            idea.assets.push(asset);
            out.set_payload(&codec::encode_response(&Response { idea, ids: vec![] })?);
        }
        wire::Action::ExportFile => {
            let path = text(r.get_selected_path())?;
            let path = std::path::Path::new(&path);
            let parent = path.parent().ok_or("export parent")?;
            if !parent.is_dir() {
                return Err("export directory unavailable".into());
            }
            // Exclusive destination: never truncate a file on a failed/partial export.
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)?;
            let result = host
                .export(&id, &text(r.get_attachment())?, &mut file)
                .and_then(|_| {
                    file.sync_all()?;
                    Ok(())
                });
            drop(file);
            if result.is_err() {
                let _ = std::fs::remove_file(path);
            }
            result?;
        }
        wire::Action::ReadPreferences => {
            if let Some(bytes) = host.read_preferences()? {
                let part = host.transfers.open(bytes, crate::now(host.start))?;
                write_chunk(out.reborrow(), &part);
            }
        }
        wire::Action::ReadPreferencesPart => {
            let part = host.transfers.read(
                &text(r.get_transfer())?,
                r.get_offset(),
                crate::now(host.start),
            )?;
            write_chunk(out.reborrow(), &part);
        }
        wire::Action::BeginPreferences => {
            if !host.writable() {
                return Err("plugin unavailable".into());
            }
            let token = host.transfers.begin(
                text(r.get_operation())?,
                r.get_total_length(),
                r.get_sha256()?,
                crate::now(host.start),
            )?;
            out.set_transfer(token.as_str());
        }
        wire::Action::AppendPreferences => {
            let token = text(r.get_transfer())?;
            let offset = host.transfers.append(
                &token,
                r.get_offset(),
                r.get_payload()?,
                crate::now(host.start),
            )?;
            out.set_transfer(token.as_str());
            out.set_offset(offset as u64);
        }
        wire::Action::FinishPreferences => {
            let token = text(r.get_transfer())?;
            let (operation, bytes) = host.transfers.finish(&token, crate::now(host.start))?;
            let bytes = host.save_preferences(&operation, bytes)?;
            // A committed result is acknowledged by its full digest; download is a separate snapshot.
            out.set_sha256(&Sha256::digest(&bytes));
            out.set_total_length(bytes.len() as u64);
        }
        wire::Action::AbortPreferences => {
            host.transfers.abort(&text(r.get_transfer())?);
        }
        wire::Action::SavePreferences => {
            // Retain the small-message route for internal callers only.
            let input = r.get_payload()?;
            if input.len() > 65536 {
                return Err("inline preferences budget".into());
            }
            out.set_payload(&host.save_preferences(&text(r.get_operation())?, input.to_vec())?);
        }
        wire::Action::OpenCaptureScope => {
            let scope = host.open_capture_scope(&id, r.get_revision())?;
            out.set_capture_scope(&scope);
        }
        wire::Action::CloseCaptureScope => {
            host.close_capture_scope(&text(r.get_capture_scope())?);
        }
        wire::Action::BeginCaptureUpload => {
            host.prepare_write()?;
            let token = host.capture_transfers.begin(
                text(r.get_operation())?,
                r.get_total_length(),
                r.get_sha256()?,
                crate::now(host.start),
            )?;
            out.set_transfer(&token);
        }
        wire::Action::AppendCaptureUpload => {
            let token = text(r.get_transfer())?;
            let offset = host.capture_transfers.append(
                &token,
                r.get_offset(),
                r.get_payload()?,
                crate::now(host.start),
            )?;
            out.set_transfer(&token);
            out.set_offset(offset as u64);
        }
        wire::Action::AbortCaptureUpload => {
            host.capture_transfers.abort(&text(r.get_transfer())?);
        }
        wire::Action::FinishPaste => {
            let (operation, bytes) = host
                .capture_transfers
                .finish(&text(r.get_transfer())?, crate::now(host.start))?;
            let message = editor_message(&bytes)?;
            let upload = message.get_root::<wire::paste_upload::Reader>()?;
            let event = paste_event(upload.get_event()?)?;
            if event.id != operation {
                return Err("paste upload identity mismatch".into());
            }
            host.record_paste(&text(upload.get_scope())?, event)?;
        }
        wire::Action::FinishCapturedSave => {
            let (operation, bytes) = host
                .capture_transfers
                .finish(&text(r.get_transfer())?, crate::now(host.start))?;
            let message = editor_message(&bytes)?;
            let upload = message.get_root::<wire::captured_save::Reader>()?;
            if text(upload.get_operation())? != operation {
                return Err("save upload identity mismatch".into());
            }
            let target = text(upload.get_target())?;
            let scope = text(upload.get_scope())?;
            let revision = upload.get_revision();
            let req = codec::decode_request(upload.get_payload()?)?;
            let snapshot = editor_snapshot(upload.get_snapshot()?)?;
            let record = match req.action {
                Action::Create if revision == 0 && req.proposed.id == target => {
                    host.create_captured(&operation, req.proposed, &scope, snapshot)?
                }
                Action::Edit if revision > 0 && req.proposed.id == target => host.apply_captured(
                    crate::Mutation {
                        operation: &operation,
                        id: &target,
                        revision,
                        action: req.action,
                        proposed: Some(req.proposed),
                        text: &req.text,
                        flag: req.flag,
                    },
                    &scope,
                    snapshot,
                )?,
                _ => return Err("captured save target or route".into()),
            };
            out.set_revision(record.revision);
            out.set_payload(&codec::encode_response(&Response {
                idea: record.idea,
                ids: vec![],
            })?);
        }
        wire::Action::Capture => {
            let scope = text(r.get_capture_scope())?;
            let parent = text(r.get_capture_parent())?;
            if scope.is_empty() {
                if !parent.is_empty() {
                    return Err("capture parent requires an editor scope".into());
                }
                out.set_payload(&host.capture(r.get_payload()?.to_vec())?);
            } else {
                let (ticket, bytes) =
                    host.capture_scoped(&scope, r.get_payload()?.to_vec(), &parent)?;
                out.set_capture_ticket(&ticket);
                out.set_payload(&bytes);
            }
        }
        wire::Action::Service => {
            out.set_payload(&host.service(r.get_payload()?.to_vec())?);
        }
        wire::Action::CommandFrameBegin
        | wire::Action::CommandFrameAppend
        | wire::Action::CommandFrameFinish
        | wire::Action::CommandFrameAbort => unreachable!(),
    }
    Ok(())
}

fn write_chunk(mut out: wire::response::Builder<'_>, part: &crate::transfer::Chunk) {
    out.set_transfer(part.token.as_str());
    out.set_offset(part.offset as u64);
    out.set_total_length(part.total as u64);
    out.set_sha256(&part.sha);
    out.set_payload(&part.bytes);
}

fn plugin_status(host: &WorkbenchState, mut out: wire::response::Builder<'_>) {
    let s = host.plugin_status();
    out.set_revision(s.revision);
    out.set_sha256(&s.digest);
    out.set_plugin_enabled(s.enabled);
    out.set_plugin_approved(s.approved);
    out.set_plugin_available(s.available);
}
fn ui_reply(reply: morrow_plugin_runtime::inline_ui::Reply, mut out: wire::response::Builder<'_>) {
    use morrow_plugin_runtime::inline_ui::Failure;
    out.set_ui_view(reply.view.as_str());
    out.set_ui_generation(reply.generation);
    out.set_revision(reply.revision);
    out.set_ui_serial(reply.serial);
    if let Some(document) = reply.document {
        out.set_payload(&document);
    }
    if let Some(failure) = reply.failure {
        let (code, message) = match failure {
            Failure::Plugin(f) => (2, f.message),
            Failure::Execution(_) => (3, "插件未能完成这次操作，当前内容已保留。".into()),
            Failure::InvalidDocument => (1, "表单操作或返回内容无效，请检查输入。".into()),
            Failure::Revoked => (4, "插件已停用，请重新打开表单。".into()),
        };
        out.set_ui_code(code);
        out.set_ui_failure(message.as_str());
    }
}

// Large editor metadata uses the same bounded ordered transfer primitive in a separate lane.
// It remains runtime Cap'n Proto; the host produces the persistent Protobuf projection itself.
fn editor_message(bytes: &[u8]) -> Result<capnp::message::Reader<capnp::serialize::OwnedSegments>> {
    if bytes.is_empty() || bytes.len() > 4 * 1024 * 1024 {
        return Err("editor upload bounds".into());
    }
    let mut cursor = std::io::Cursor::new(bytes);
    let message = serialize::read_message(
        &mut cursor,
        ReaderOptions {
            traversal_limit_in_words: Some(1024 * 1024),
            nesting_limit: 20,
        },
    )?;
    if cursor.position() != bytes.len() as u64 {
        return Err("trailing editor upload bytes".into());
    }
    Ok(message)
}
fn paste_event(r: wire::paste_event::Reader<'_>) -> Result<crate::capture_provenance::PasteEvent> {
    use crate::capture_provenance::{PasteEvent, PastePart};
    let raw = r.get_parts()?;
    if raw.len() > 1024 {
        return Err("paste part budget".into());
    }
    let parts = raw
        .iter()
        .map(|v| {
            Ok(PastePart {
                ticket: text(v.get_ticket())?,
                literal: text(v.get_literal())?,
                selection: text(v.get_selection())?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(PasteEvent {
        id: text(r.get_id())?,
        field: text(r.get_field())?,
        before: text(r.get_before())?,
        start_utf16: r.get_start_utf16(),
        end_utf16: r.get_end_utf16(),
        parts,
        after: text(r.get_after())?,
    })
}
fn editor_snapshot(
    r: wire::editor_snapshot::Reader<'_>,
) -> Result<crate::capture_provenance::EditorSnapshot> {
    use crate::capture_provenance::{AttachmentAlias, EditorSnapshot};
    let raw = r.get_aliases()?;
    if raw.len() > 1024 {
        return Err("editor alias budget".into());
    }
    let aliases = raw
        .iter()
        .map(|v| {
            Ok(AttachmentAlias {
                id: text(v.get_id())?,
                location: text(v.get_location())?,
                name: text(v.get_name())?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(EditorSnapshot {
        title: text(r.get_title())?,
        description: text(r.get_description())?,
        hypothesis: text(r.get_hypothesis())?,
        conclusion: text(r.get_conclusion())?,
        todos: text(r.get_todos())?,
        aliases,
    })
}

fn plugin_catalog_reply(
    page: crate::plugin_catalog::PluginCatalogPage,
    mut out: wire::response::Builder<'_>,
) {
    out.set_revision(page.revision);
    out.set_cursor(page.cursor.as_str());
    let mut entries = out.init_plugins(page.entries.len() as u32);
    for (index, entry) in page.entries.into_iter().enumerate() {
        let mut row = entries.reborrow().get(index as u32);
        row.set_package_id(entry.id.as_str());
        row.set_name(entry.name.as_str());
        row.set_package_version(entry.version.as_str());
        row.set_digest(&entry.digest);
        row.set_enabled(entry.enabled);
        row.set_builtin(entry.builtin);
        row.set_available(entry.available);
        row.set_issue(entry.issue.as_str());
        for (index, value) in entry.declared.iter().enumerate() {
            if index == 0 {
                row.reborrow().init_declared(entry.declared.len() as u32);
            }
            row.reborrow()
                .get_declared()
                .expect("initialized list")
                .set(index as u32, value.as_str());
        }
        for (index, value) in entry.approved.iter().enumerate() {
            if index == 0 {
                row.reborrow().init_approved(entry.approved.len() as u32);
            }
            row.reborrow()
                .get_approved()
                .expect("initialized list")
                .set(index as u32, value.as_str());
        }
        {
            let mut values = row
                .reborrow()
                .init_declared_io(entry.declared_io.len() as u32);
            for (index, value) in entry.declared_io.iter().enumerate() {
                values.set(index as u32, value.as_str());
            }
        }
        {
            let mut values = row
                .reborrow()
                .init_approved_io(entry.approved_io.len() as u32);
            for (index, value) in entry.approved_io.iter().enumerate() {
                values.set(index as u32, value.as_str());
            }
        }
        {
            let mut values = row
                .reborrow()
                .init_io_handlers(entry.io_handlers.len() as u32);
            for (index, value) in entry.io_handlers.iter().enumerate() {
                values.set(index as u32, value.as_str());
            }
        }
        for (index, value) in entry.dependencies.iter().enumerate() {
            if index == 0 {
                row.reborrow()
                    .init_dependencies(entry.dependencies.len() as u32);
            }
            row.reborrow()
                .get_dependencies()
                .expect("initialized list")
                .set(index as u32, value.as_str());
        }
        let mut handlers = row.init_handlers(entry.handlers.len() as u32);
        for (index, value) in entry.handlers.into_iter().enumerate() {
            let mut handler = handlers.reborrow().get(index as u32);
            handler.set_name(value.name.as_str());
            handler.set_input_type(value.input_type.as_str());
            handler.set_output_type(value.output_type.as_str());
            handler.set_max_input_bytes(value.max_input_bytes);
            handler.set_max_output_bytes(value.max_output_bytes);
        }
    }
}

fn credential_reply(
    value: &crate::credential_control::CredentialInfo,
    mut out: wire::credential_info::Builder<'_>,
) {
    out.set_reference(&value.reference);
    out.set_revision(value.revision);
    out.set_created_ms(value.created_ms);
    out.set_expires_ms(value.expires_ms);
    out.set_disabled(value.disabled);
}

fn endpoint_policy(
    value: wire::endpoint_policy::Reader<'_>,
) -> Result<morrow_core::outbound_authority::proto::Endpoint> {
    let methods = value.get_methods()?;
    if methods.len() > 16 {
        return Err("endpoint method count exceeds budget".into());
    }
    Ok(morrow_core::outbound_authority::proto::Endpoint {
        package_id: text(value.get_package_id())?,
        package_sha256: value.get_package_digest()?.to_vec(),
        origin: text(value.get_origin())?,
        profile: i32::from(value.get_profile()),
        methods: methods.iter().map(text).collect::<Result<_>>()?,
        credential_reference: value.get_credential_reference()?.to_vec(),
        root_certificate: value.get_root_certificate()?.to_vec(),
        max_request_bytes: u64::from(value.get_max_request_bytes()),
        max_response_bytes: u64::from(value.get_max_response_bytes()),
        max_header_bytes: u64::from(value.get_max_header_bytes()),
        max_concurrent: u32::from(value.get_max_concurrent()),
        timeout_ms: u64::from(value.get_timeout_ms()),
        max_frame_bytes: u64::from(value.get_max_frame_bytes()),
    })
}
fn endpoint_reply(
    value: &crate::endpoint_control::EndpointInfo,
    mut out: wire::endpoint_info::Builder<'_>,
) -> Result<()> {
    out.set_reference(&value.reference);
    out.set_revision(value.revision);
    out.set_created_ms(value.created_ms);
    out.set_expires_ms(value.expires_ms);
    out.set_disabled(value.disabled);
    let policy = &value.policy;
    let mut p = out.init_policy();
    p.set_package_id(&policy.package_id);
    p.set_package_digest(&policy.package_sha256);
    p.set_origin(&policy.origin);
    p.set_profile(policy.profile.try_into()?);
    p.set_credential_reference(&policy.credential_reference);
    p.set_root_certificate(&policy.root_certificate);
    p.set_max_request_bytes(policy.max_request_bytes.try_into()?);
    p.set_max_response_bytes(policy.max_response_bytes.try_into()?);
    p.set_max_header_bytes(policy.max_header_bytes.try_into()?);
    p.set_max_concurrent(policy.max_concurrent.try_into()?);
    p.set_timeout_ms(policy.timeout_ms.try_into()?);
    p.set_max_frame_bytes(policy.max_frame_bytes.try_into()?);
    let mut methods = p.init_methods(policy.methods.len().try_into()?);
    for (i, method) in policy.methods.iter().enumerate() {
        methods.set(i as u32, method);
    }
    Ok(())
}
