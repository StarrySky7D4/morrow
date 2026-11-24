//! One Windows Workbench confirmation authorizes one command on one card.
use crate::{Result, WorkbenchState, host_capnp as wire, now};
use morrow_core::{
    content_change::ContentChange,
    lifecycle::GrantKind,
    plugin_package::Package,
    response::Outcome,
    runtime::{Command, CreateContent, ReadContent},
    task::Invocation,
    transaction::{Lookup, Receipt},
};

pub(crate) fn supports(package: &Package) -> bool {
    cfg!(windows)
        && package.manifest().guest_abi_version == 2
        && package.io_declaration().is_none()
        && package.channel_declaration().is_none()
        && package.blob_upload_declaration().is_none()
        && !package.manifest().dependencies.iter().any(|d| !d.optional)
        && package.capabilities().iter().any(|kind| {
            matches!(
                kind,
                GrantKind::CreateContent
                    | GrantKind::EditContent
                    | GrantKind::ReadContent
                    | GrantKind::QueryOperation
            )
        })
}

fn reviewed(task: wire::plugin_content_task::Reader<'_>) -> Result<(GrantKind, Command)> {
    use wire::plugin_content_task::Which;
    let card = task.get_card()?.to_str()?.to_owned();
    let id = task.get_request_id()?.to_str()?.to_owned();
    Ok(match task.which()? {
        Which::Unspecified(()) => return Err("choose a card operation".into()),
        Which::Create(value) => {
            let v = value?;
            (
                GrantKind::CreateContent,
                Command::CreateContent(CreateContent {
                    operation_id: id,
                    card_id: card,
                    type_id: v.get_type_id()?.to_str()?.into(),
                    format_version: 1,
                    title: v.get_title()?.to_str()?.into(),
                    body: v.get_body()?.into(),
                }),
            )
        }
        Which::Edit(value) => {
            let v = value?;
            (
                GrantKind::EditContent,
                Command::EditContent(ContentChange {
                    operation_id: id,
                    card_id: card,
                    expected_revision: v.get_revision(),
                    title: v.get_title()?.to_str()?.into(),
                    body: v.get_body()?.into(),
                    preview_text: v.get_preview()?.to_str()?.into(),
                    attachments: None,
                }),
            )
        }
        Which::Read(value) => {
            let v = value?;
            (
                GrantKind::ReadContent,
                Command::ReadContent(ReadContent {
                    request_id: id,
                    card_id: card,
                    expected_revision: v.get_revision(),
                    offset: v.get_offset(),
                    length: v.get_length(),
                }),
            )
        }
        Which::Lookup(operation) => (
            GrantKind::QueryOperation,
            Command::QueryOperation {
                request_id: id,
                card_id: card,
                operation_id: operation?.to_str()?.into(),
            },
        ),
    })
}

fn committed(out: &mut wire::plugin_content_result::Builder<'_>, receipt: &Receipt) {
    out.set_status(wire::PluginContentStatus::Committed);
    out.set_operation(receipt.operation_id.as_str());
    out.set_revision(receipt.revision);
    out.set_event(receipt.event_id.as_str());
    out.set_digest(&receipt.content_sha256);
}

pub(crate) fn handle(
    state: &mut WorkbenchState,
    package_id: &str,
    digest: &[u8],
    revision: u64,
    task: wire::plugin_content_task::Reader<'_>,
    mut out: wire::plugin_content_result::Builder<'_>,
) -> Result<()> {
    let (kind, command) = reviewed(task)?;
    let invocation = Invocation::new(command.request_id(), &command)?;
    let package = state.external_package(package_id, digest, revision)?;
    let selected = state
        .manager
        .as_ref()
        .and_then(|m| m.selection(package_id))
        .ok_or("card plugin selection changed")?;
    if !supports(&package)
        || !selected.enabled
        || !package.capabilities().contains(&kind)
        || !selected.approved.contains(&kind)
    {
        return Err("this card operation requires current plugin approval".into());
    }
    if matches!(kind, GrantKind::CreateContent | GrantKind::EditContent) {
        state.prepare_write()?;
    }
    let session = state.start_external(package_id, digest, revision)?;
    let start = state.start;
    let time = now(start);
    // The task runtime compares the guest exchange with the reviewed command bytes
    // before dispatch. Closing the fresh session also retires its only card grant.
    let result = (|| -> Result<_> {
        state.pool.grant_root(
            &mut state.host,
            &session,
            kind,
            command.card_id(),
            time.saturating_add(5000),
            time,
        )?;
        let manager = state.manager.as_ref().ok_or("plugin manager unavailable")?;
        let report =
            state
                .pool
                .run_task(manager, &mut state.host, &session, &invocation, || {
                    now(start)
                })?;
        if report.execution.outcome != Ok(0)
            || report.execution.host_calls != 1
            || report.output.is_some()
            || report.failure.is_some()
        {
            return Err("card task outcome unconfirmed; look up its operation ID".into());
        }
        Ok(report.response.ok_or("missing card task receipt")?)
    })();
    let closed = state.pool.close(&mut state.host, &session);
    let response = result?;
    closed?;
    if response.request_id != command.request_id() {
        return Err("card task receipt belongs to a different request".into());
    }
    out.set_card(command.card_id());
    out.set_request_id(command.request_id());
    let reviewed_card = command.card_id();
    match (&command, response.outcome) {
        (Command::CreateContent(_) | Command::EditContent(_), Outcome::ContentCommitted(r))
            if r.card_id == reviewed_card && r.operation_id == command.request_id() =>
        {
            committed(&mut out, &r)
        }
        (Command::ReadContent(read), Outcome::ContentChunk(chunk))
            if chunk.card_id == reviewed_card
                && chunk.revision == read.expected_revision
                && chunk.offset == read.offset =>
        {
            out.set_status(wire::PluginContentStatus::Chunk);
            out.set_revision(chunk.revision);
            out.set_offset(chunk.offset);
            out.set_total_length(chunk.total_length);
            out.set_digest(&chunk.body_sha256);
            out.set_bytes(&chunk.bytes);
        }
        (
            Command::QueryOperation {
                operation_id: expected,
                ..
            },
            Outcome::OperationResult {
                card_id,
                operation_id,
                result,
            },
        ) if card_id == reviewed_card && &operation_id == expected => match result {
            Lookup::Committed(r) => committed(&mut out, &r),
            Lookup::Absent => {
                out.set_status(wire::PluginContentStatus::Absent);
                out.set_operation(operation_id.as_str());
            }
        },
        (_, Outcome::Rejected(failure)) => {
            out.set_status(wire::PluginContentStatus::Rejected);
            out.set_failure(format!("{failure:?}").as_str());
        }
        _ => return Err("unexpected card task response".into()),
    }
    Ok(())
}
