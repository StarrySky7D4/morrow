//! Private versioned-content UI boundary. Outer host fields are authoritative.
use crate::{
    Result, WorkbenchState, cards_content::CardAction, content_api_capnp as wire,
    host_capnp as host_wire, versioned_record::VersionedRecord,
};
use capnp::{
    message::{Builder, ReaderOptions},
    serialize,
};
use morrow_workbench_plugin::{
    Asset, cards_v2::Fields, query_v2::Conditions, tasks_v2::Command as TaskCommand,
};
use sha2::{Digest, Sha256};
use std::io::Cursor;

const MAX_FRAME: usize = 128 * 1024;
pub fn digest() -> [u8; 32] {
    Sha256::digest(
        include_str!("../schemas/content_api.capnp")
            .replace("\r\n", "\n")
            .as_bytes(),
    )
    .into()
}
fn text(value: capnp::Result<capnp::text::Reader<'_>>) -> Result<String> {
    Ok(value?.to_str()?.to_owned())
}
fn frame(bytes: &[u8]) -> Result<capnp::message::Reader<serialize::OwnedSegments>> {
    if bytes.len() > MAX_FRAME {
        return Err("content API frame budget".into());
    }
    let mut cursor = Cursor::new(bytes);
    let message = serialize::read_message(
        &mut cursor,
        ReaderOptions {
            traversal_limit_in_words: Some(32768),
            nesting_limit: 16,
        },
    )?;
    if cursor.position() != bytes.len() as u64 {
        return Err("trailing content API bytes".into());
    }
    Ok(message)
}
fn version(version: u16, actual: capnp::Result<capnp::data::Reader<'_>>) -> Result<()> {
    if version != 1 || actual? != digest() {
        return Err("content API contract mismatch".into());
    }
    Ok(())
}
fn bounded(value: &str, max: usize) -> Result<()> {
    if value.len() > max {
        return Err("content API text budget".into());
    }
    Ok(())
}
pub(crate) fn encode_task_edit(command: &TaskCommand) -> Vec<u8> {
    let mut message = Builder::new_default();
    {
        let mut r = message.init_root::<wire::task_edit::Builder>();
        r.set_version(1);
        r.set_digest(&digest());
        match command {
            TaskCommand::SetCompletion { id, complete } => {
                r.set_action(wire::TaskAction::SetCompletion);
                r.set_task_id(id);
                r.set_complete(*complete);
            }
            TaskCommand::Rename { id, text } => {
                r.set_action(wire::TaskAction::Rename);
                r.set_task_id(id);
                r.set_text(text);
            }
            TaskCommand::Reorder(order) => {
                r.set_action(wire::TaskAction::Reorder);
                let mut list = r.init_order(order.len() as u32);
                for (i, id) in order.iter().enumerate() {
                    list.set(i as u32, id);
                }
            }
            TaskCommand::SetStage(stage) => {
                r.set_action(wire::TaskAction::SetStage);
                r.set_text(stage);
            }
            TaskCommand::CompleteAllAndSetStage(stage) => {
                r.set_action(wire::TaskAction::CompleteAllAndSetStage);
                r.set_text(stage);
            }
            TaskCommand::Add { id, text } => {
                r.set_action(wire::TaskAction::Add);
                r.set_task_id(id);
                r.set_text(text);
            }
            TaskCommand::Remove(id) => {
                r.set_action(wire::TaskAction::Remove);
                r.set_task_id(id);
            }
        }
    }
    serialize::write_message_to_words(&message)
}
fn task_edit(bytes: &[u8]) -> Result<TaskCommand> {
    let message = frame(bytes)?;
    let r = message.get_root::<wire::task_edit::Reader>()?;
    version(r.get_version(), r.get_digest())?;
    let id = text(r.get_task_id())?;
    let value = text(r.get_text())?;
    let list = r.get_order()?;
    if list.len() > 128 {
        return Err("TaskId order budget".into());
    }
    let order = list.iter().map(text).collect::<Result<Vec<_>>>()?;
    bounded(&id, 256)?;
    bounded(&value, 2048)?;
    for item in &order {
        bounded(item, 256)?;
    }
    let action = r.get_action()?;
    let uses_id = matches!(
        action,
        wire::TaskAction::SetCompletion
            | wire::TaskAction::Rename
            | wire::TaskAction::Add
            | wire::TaskAction::Remove
    );
    let uses_text = matches!(
        action,
        wire::TaskAction::Rename
            | wire::TaskAction::SetStage
            | wire::TaskAction::CompleteAllAndSetStage
            | wire::TaskAction::Add
    );
    if (!uses_id && r.has_task_id())
        || (!uses_text && r.has_text())
        || (!matches!(action, wire::TaskAction::Reorder) && r.has_order())
        || (!matches!(action, wire::TaskAction::SetCompletion) && r.get_complete())
    {
        return Err("irrelevant TaskId action field".into());
    }
    let command = match action {
        wire::TaskAction::SetCompletion if !id.is_empty() => TaskCommand::SetCompletion {
            id,
            complete: r.get_complete(),
        },
        wire::TaskAction::Rename if !id.is_empty() && !value.is_empty() => {
            TaskCommand::Rename { id, text: value }
        }
        wire::TaskAction::Reorder => TaskCommand::Reorder(order),
        wire::TaskAction::SetStage if !value.is_empty() => TaskCommand::SetStage(value),
        wire::TaskAction::CompleteAllAndSetStage if !value.is_empty() => {
            TaskCommand::CompleteAllAndSetStage(value)
        }
        wire::TaskAction::Add if !id.is_empty() && !value.is_empty() => {
            TaskCommand::Add { id, text: value }
        }
        wire::TaskAction::Remove if !id.is_empty() => TaskCommand::Remove(id),
        _ => return Err("invalid TaskId action".into()),
    };
    Ok(command)
}
fn fields_to_wire(mut out: wire::card_fields::Builder<'_>, fields: &Fields) {
    out.set_title(&fields.title);
    out.set_description(&fields.description);
    out.set_hypothesis(&fields.hypothesis);
    out.set_conclusion(&fields.conclusion);
    out.set_icon(fields.icon);
    out.set_color(fields.color);
    let mut assets = out.init_assets(fields.assets.len() as u32);
    for (i, asset) in fields.assets.iter().enumerate() {
        let mut item = assets.reborrow().get(i as u32);
        item.set_id(&asset.id);
        item.set_name(&asset.name);
        item.set_kind(&asset.kind);
        item.set_bytes(asset.bytes);
    }
}
pub(crate) fn encode_card_edit(action: &CardAction) -> Vec<u8> {
    let mut message = Builder::new_default();
    {
        let mut r = message.init_root::<wire::card_edit::Builder>();
        r.set_version(1);
        r.set_digest(&digest());
        match action {
            CardAction::Edit(fields) => {
                r.set_action(wire::CardAction::Edit);
                fields_to_wire(r.init_fields(), fields);
            }
            CardAction::SetFavorite(favorite) => {
                r.set_action(wire::CardAction::SetFavorite);
                r.set_favorite(*favorite);
            }
            CardAction::SetCategory { category, stage } => {
                r.set_action(wire::CardAction::SetCategory);
                r.set_category(category);
                r.set_stage(stage);
            }
            CardAction::Delete => r.set_action(wire::CardAction::Delete),
            CardAction::Restore => r.set_action(wire::CardAction::Restore),
        }
    }
    serialize::write_message_to_words(&message)
}
pub(crate) fn card_edit(bytes: &[u8]) -> Result<CardAction> {
    let message = frame(bytes)?;
    let r = message.get_root::<wire::card_edit::Reader>()?;
    version(r.get_version(), r.get_digest())?;
    let action_kind = r.get_action()?;
    if (!matches!(action_kind, wire::CardAction::Edit) && r.has_fields())
        || (!matches!(action_kind, wire::CardAction::SetFavorite) && r.get_favorite())
        || (!matches!(action_kind, wire::CardAction::SetCategory)
            && (r.has_category() || r.has_stage()))
    {
        return Err("irrelevant card action field".into());
    }
    let action = match action_kind {
        wire::CardAction::Edit => {
            let f = r.get_fields()?;
            let list = f.get_assets()?;
            if list.len() > 20 {
                return Err("card asset budget".into());
            }
            let mut assets = Vec::with_capacity(list.len() as usize);
            for a in list.iter() {
                let asset = Asset {
                    id: text(a.get_id())?,
                    name: text(a.get_name())?,
                    kind: text(a.get_kind())?,
                    bytes: a.get_bytes(),
                };
                bounded(&asset.id, 256)?;
                bounded(&asset.name, 4096)?;
                bounded(&asset.kind, 16)?;
                assets.push(asset);
            }
            let fields = Fields {
                title: text(f.get_title())?,
                description: text(f.get_description())?,
                hypothesis: text(f.get_hypothesis())?,
                conclusion: text(f.get_conclusion())?,
                icon: f.get_icon(),
                color: f.get_color(),
                assets,
            };
            bounded(&fields.title, 16384)?;
            bounded(&fields.description, 65536)?;
            bounded(&fields.hypothesis, 16384)?;
            bounded(&fields.conclusion, 16384)?;
            CardAction::Edit(fields)
        }
        wire::CardAction::SetFavorite => CardAction::SetFavorite(r.get_favorite()),
        wire::CardAction::SetCategory => {
            let category = text(r.get_category())?;
            let stage = text(r.get_stage())?;
            bounded(&category, 256)?;
            bounded(&stage, 256)?;
            if category.is_empty() || stage.is_empty() {
                return Err("invalid category/stage".into());
            }
            CardAction::SetCategory { category, stage }
        }
        wire::CardAction::Delete => CardAction::Delete,
        wire::CardAction::Restore => CardAction::Restore,
    };
    Ok(action)
}
pub(crate) fn encode_query(c: &Conditions) -> Vec<u8> {
    let mut message = Builder::new_default();
    {
        let mut r = message.init_root::<wire::query::Builder>();
        r.set_version(1);
        r.set_digest(&digest());
        r.set_section(&c.section);
        r.set_filter(&c.filter);
        r.set_text(&c.text);
        r.set_sort(&c.sort);
    }
    serialize::write_message_to_words(&message)
}
fn query(bytes: &[u8]) -> Result<Conditions> {
    let message = frame(bytes)?;
    let r = message.get_root::<wire::query::Reader>()?;
    version(r.get_version(), r.get_digest())?;
    let conditions = Conditions {
        section: text(r.get_section())?,
        filter: text(r.get_filter())?,
        text: text(r.get_text())?,
        sort: text(r.get_sort())?,
    };
    morrow_workbench_plugin::query_v2::validate_request(
        &morrow_workbench_plugin::query_v2::Request::Filter {
            conditions: conditions.clone(),
            candidates: vec![],
        },
    )?;
    Ok(conditions)
}

fn write_assets(mut card: wire::card::Builder<'_>, values: &[Asset]) {
    let mut list = card.reborrow().init_assets(values.len() as u32);
    for (i, value) in values.iter().enumerate() {
        let mut item = list.reborrow().get(i as u32);
        item.set_id(&value.id);
        item.set_name(&value.name);
        item.set_kind(&value.kind);
        item.set_bytes(value.bytes);
    }
}
fn write_texts(mut card: wire::card::Builder<'_>, values: &[String], field: u8) {
    let mut list = match field {
        0 => card.reborrow().init_todos(values.len() as u32),
        1 => card.reborrow().init_completed(values.len() as u32),
        2 => card.reborrow().init_retired_task_ids(values.len() as u32),
        _ => unreachable!(),
    };
    for (i, value) in values.iter().enumerate() {
        list.set(i as u32, value);
    }
}
fn fill_card(mut card: wire::card::Builder<'_>, record: &VersionedRecord) -> Result<()> {
    match record {
        VersionedRecord::Legacy(record) => {
            let idea = &record.idea;
            card.set_id(&idea.id);
            card.set_title(&idea.title);
            card.set_revision(record.revision);
            card.set_format_version(1);
            card.set_description(&idea.description);
            card.set_category(&idea.category);
            card.set_stage(&idea.stage);
            card.set_hypothesis(&idea.hypothesis);
            card.set_conclusion(&idea.conclusion);
            card.set_favorite(idea.favorite);
            card.set_icon(idea.icon);
            card.set_color(idea.color);
            card.set_deleted(idea.deleted);
            card.set_deleted_at(idea.deleted_at);
            card.set_projected_stage(idea.project_stage());
            let complete = idea
                .todos
                .iter()
                .filter(|todo| idea.completed.contains(todo))
                .count();
            card.set_complete_count(u32::try_from(complete)?);
            card.set_incomplete_count(u32::try_from(idea.todos.len() - complete)?);
            write_assets(card.reborrow(), &idea.assets);
            write_texts(card.reborrow(), &idea.todos, 0);
            write_texts(card.reborrow(), &idea.completed, 1);
        }
        VersionedRecord::Tasks(record) => {
            let p = &record.properties;
            card.set_id(&record.id);
            card.set_title(&record.title);
            card.set_revision(record.revision);
            card.set_format_version(2);
            card.set_description(&p.description);
            card.set_category(&p.category);
            card.set_stage(&p.stage);
            card.set_hypothesis(&p.hypothesis);
            card.set_conclusion(&p.conclusion);
            card.set_favorite(p.favorite);
            card.set_icon(u16::try_from(p.icon)?);
            card.set_color(p.color);
            card.set_deleted(p.deleted);
            card.set_deleted_at(p.deleted_at);
            card.set_projected_stage(&record.projection.stage);
            card.set_complete_count(record.projection.complete);
            card.set_incomplete_count(record.projection.incomplete);
            card.set_ambiguous_count(record.projection.ambiguous);
            let assets: Vec<Asset> = p
                .assets
                .iter()
                .map(|a| Asset {
                    id: a.id.clone(),
                    name: a.name.clone(),
                    kind: a.kind.clone(),
                    bytes: a.bytes,
                })
                .collect();
            write_assets(card.reborrow(), &assets);
            write_texts(card.reborrow(), &p.retired_task_ids, 2);
            {
                let mut tasks = card.reborrow().init_tasks(p.tasks.len() as u32);
                for (i, task) in p.tasks.iter().enumerate() {
                    let mut item = tasks.reborrow().get(i as u32);
                    item.set_id(&task.id);
                    item.set_text(&task.text);
                    item.set_completion(u16::try_from(task.completion)?);
                    item.set_legacy_completed(task.legacy_completed);
                    item.set_legacy_duplicates(task.legacy_duplicates);
                }
            }
            if let Some(source) = &p.origin {
                let mut origin = card.reborrow().init_origin();
                origin.set_card_id(&source.card_id);
                origin.set_source_revision(source.source_revision);
                origin.set_source_sha256(&source.source_sha256);
                origin.set_migrator_version(source.migrator_version);
                origin.set_target_version(source.target_version);
                origin.set_original_properties(&source.original_properties);
                origin.set_historical_project_stage(&source.historical_project_stage);
                origin.set_original_title(&source.original_title);
                let mut mapping = origin.init_mapping(source.mapping.len() as u32);
                for (i, value) in source.mapping.iter().enumerate() {
                    let mut item = mapping.reborrow().get(i as u32);
                    item.set_source_index(value.source_index);
                    item.set_task_id(&value.task_id);
                }
            }
        }
    }
    Ok(())
}
pub(crate) fn encode_envelope(
    kind: wire::EnvelopeKind,
    id: &str,
    operation: &str,
    source_revision: u64,
    revision: u64,
    repeated: bool,
    record: Option<&VersionedRecord>,
) -> Result<Vec<u8>> {
    let mut message = Builder::new_default();
    {
        let mut out = message.init_root::<wire::envelope::Builder>();
        out.set_version(1);
        out.set_digest(&digest());
        out.set_kind(kind);
        out.set_id(id);
        out.set_operation(operation);
        out.set_source_revision(source_revision);
        out.set_revision(revision);
        out.set_repeated(repeated);
        if let Some(record) = record {
            fill_card(out.init_record(), record)?;
        }
    }
    let bytes = serialize::write_message_to_words(&message);
    if bytes.len() > MAX_FRAME {
        return Err("content API envelope budget".into());
    }
    Ok(bytes)
}
fn require_empty(bytes: &[u8]) -> Result<()> {
    if !bytes.is_empty() {
        return Err("unexpected content API payload".into());
    }
    Ok(())
}
pub(crate) fn handle(
    host: &mut WorkbenchState,
    r: host_wire::request::Reader<'_>,
    mut out: host_wire::response::Builder<'_>,
) -> Result<()> {
    let action = r.get_action()?;
    let id = text(r.get_id())?;
    let operation = text(r.get_operation())?;
    let payload = match r.get_payload() {
        Ok(payload) => payload,
        Err(error)
            if matches!(
                action,
                host_wire::Action::EditTasks | host_wire::Action::EditCard
            ) =>
        {
            return Err(host.no_commit_if_absent(&id, &operation, Box::new(error)));
        }
        Err(error) => return Err(Box::new(error)),
    };
    match action {
        host_wire::Action::ReadVersioned => {
            require_empty(payload)?;
            let record = host.read_versioned(&id)?;
            let revision = match &record {
                VersionedRecord::Legacy(v) => v.revision,
                VersionedRecord::Tasks(v) => v.revision,
            };
            out.set_payload(&encode_envelope(
                wire::EnvelopeKind::Record,
                &id,
                "",
                revision,
                revision,
                false,
                Some(&record),
            )?);
            out.set_revision(revision);
        }
        host_wire::Action::PageVersioned => {
            require_empty(payload)?;
            let requested = r.get_limit();
            if requested > 4096 {
                return Err("invalid versioned page limit".into());
            }
            let limit = if requested == 0 {
                128
            } else {
                requested.min(128)
            };
            let (records, cursor) = host.page_versioned(&text(r.get_cursor())?, limit)?;
            let mut ids = out.reborrow().init_ids(records.len() as u32);
            for (i, record) in records.iter().enumerate() {
                let id = match record {
                    VersionedRecord::Legacy(v) => &v.idea.id,
                    VersionedRecord::Tasks(v) => &v.id,
                };
                ids.set(i as u32, id);
            }
            out.set_cursor(&cursor);
        }
        host_wire::Action::PlanTasksMigration => {
            require_empty(payload)?;
            let plan = host.plan_tasks_migration(&id)?;
            out.set_payload(&encode_envelope(
                wire::EnvelopeKind::Plan,
                &plan.card_id,
                &plan.operation,
                plan.source_revision,
                plan.source_revision,
                false,
                None,
            )?);
            out.set_revision(plan.source_revision);
        }
        host_wire::Action::MigrateTasks => {
            require_empty(payload)?;
            let result = host.migrate_tasks(&operation, &id, r.get_revision())?;
            let record = VersionedRecord::Tasks(result.committed);
            out.set_payload(&encode_envelope(
                wire::EnvelopeKind::Commit,
                &id,
                &operation,
                r.get_revision(),
                result.receipt.revision,
                result.repeated,
                Some(&record),
            )?);
            out.set_revision(result.receipt.revision);
        }
        host_wire::Action::EditTasks => {
            let command =
                task_edit(payload).map_err(|e| host.no_commit_if_absent(&id, &operation, e))?;
            let result = host.edit_tasks(&operation, &id, r.get_revision(), &command)?;
            let record = VersionedRecord::Tasks(result.committed);
            out.set_payload(&encode_envelope(
                wire::EnvelopeKind::Commit,
                &id,
                &operation,
                r.get_revision(),
                result.receipt.revision,
                result.repeated,
                Some(&record),
            )?);
            out.set_revision(result.receipt.revision);
        }
        host_wire::Action::EditCard => {
            let command =
                card_edit(payload).map_err(|e| host.no_commit_if_absent(&id, &operation, e))?;
            let result = host.edit_card(&operation, &id, r.get_revision(), &command)?;
            let record = VersionedRecord::Tasks(result.committed);
            out.set_payload(&encode_envelope(
                wire::EnvelopeKind::Commit,
                &id,
                &operation,
                r.get_revision(),
                result.receipt.revision,
                result.repeated,
                Some(&record),
            )?);
            out.set_revision(result.receipt.revision);
        }
        host_wire::Action::QueryVersioned => {
            let conditions = query(payload)?;
            let canonical_query = encode_query(&conditions);
            let offset = cursor_offset(&text(r.get_cursor())?, &operation, &canonical_query)?;
            validate_query_page_request(host, &operation, offset, r.get_limit())?;
            // Ready queries are delivered from the saved result. A new query runs
            // only for an operation not yet present in the read-capture store.
            let ids = host.query_versioned_with_operation(
                &operation,
                &conditions.section,
                &conditions.filter,
                &conditions.text,
                &conditions.sort,
            )?;
            let (page, next) = query_page(
                &ids,
                offset,
                r.get_limit(),
                &operation,
                &canonical_query,
                !host.writable(),
                host.maintenance_warning(),
            )?;
            let mut list = out.reborrow().init_ids(page.len() as u32);
            for (i, id) in page.iter().enumerate() {
                list.set(i as u32, id);
            }
            out.set_cursor(&next);
        }
        _ => return Err("not a content API action".into()),
    }
    Ok(())
}

#[cfg(test)]
mod tests;

fn cursor_token(operation: &str, query: &[u8], offset: usize) -> String {
    let mut hash = Sha256::new();
    hash.update(b"morrow.content.query.cursor.v1\0");
    hash.update((operation.len() as u64).to_le_bytes());
    hash.update(operation.as_bytes());
    hash.update(Sha256::digest(query));
    hash.update((offset as u64).to_le_bytes());
    let mut hex = String::with_capacity(64);
    const DIGITS: &[u8] = b"0123456789abcdef";
    for byte in hash.finalize() {
        hex.push(DIGITS[(byte >> 4) as usize] as char);
        hex.push(DIGITS[(byte & 15) as usize] as char);
    }
    format!("v1:{offset}:{hex}")
}
fn cursor_offset(cursor: &str, operation: &str, query: &[u8]) -> Result<usize> {
    if cursor.is_empty() {
        return Ok(0);
    }
    if cursor.len() > 80 {
        return Err("query cursor budget".into());
    }
    let parts: Vec<_> = cursor.split(':').collect();
    if parts.len() != 3 || parts[0] != "v1" {
        return Err("invalid query cursor".into());
    }
    let offset: usize = parts[1].parse()?;
    if offset == 0 || cursor_token(operation, query, offset) != cursor {
        return Err("query cursor does not match operation and conditions".into());
    }
    Ok(offset)
}
fn outer_page_size(ids: &[String], cursor: &str, read_only: bool, warning: Option<&str>) -> usize {
    let mut message = Builder::new_default();
    {
        let mut out = message.init_root::<host_wire::response::Builder>();
        out.set_version(1);
        out.set_digest(&crate::protocol::digest());
        let mut list = out.reborrow().init_ids(ids.len() as u32);
        for (i, id) in ids.iter().enumerate() {
            list.set(i as u32, id);
        }
        out.set_cursor(cursor);
        out.set_read_only(read_only);
        if let Some(warning) = warning {
            out.set_maintenance_warning(warning);
        }
    }
    serialize::write_message_to_words(&message).len()
}
fn query_page<'a>(
    ids: &'a [String],
    offset: usize,
    requested: u32,
    operation: &str,
    query: &[u8],
    read_only: bool,
    warning: Option<&str>,
) -> Result<(&'a [String], String)> {
    if ids.len() > 4096
        || requested > 4096
        || offset > ids.len()
        || (offset == ids.len() && offset != 0)
    {
        return Err("invalid query page boundary".into());
    }
    let limit = if requested == 0 {
        128
    } else {
        requested as usize
    };
    if offset == ids.len() {
        return Ok((&[], String::new()));
    }
    let maximum = limit.min(ids.len() - offset);
    let fits = |count: usize| {
        let end = offset + count;
        let next = if end < ids.len() {
            cursor_token(operation, query, end)
        } else {
            String::new()
        };
        outer_page_size(&ids[offset..end], &next, read_only, warning) <= MAX_FRAME
    };
    if !fits(1) {
        return Err("query ID exceeds response frame budget".into());
    }
    if fits(maximum) {
        let end = offset + maximum;
        let next = if end < ids.len() {
            cursor_token(operation, query, end)
        } else {
            String::new()
        };
        return Ok((&ids[offset..end], next));
    }
    let (mut low, mut high) = (1, maximum);
    while low < high {
        let middle = low + (high - low + 1) / 2;
        if fits(middle) {
            low = middle;
        } else {
            high = middle - 1;
        }
    }
    let end = offset + low;
    let next = if end < ids.len() {
        cursor_token(operation, query, end)
    } else {
        String::new()
    };
    Ok((&ids[offset..end], next))
}

// Pagination is result delivery, never permission to create a new query.
// Validate caller-controlled limits before snapshot/guest execution.
fn validate_query_page_request(
    host: &WorkbenchState,
    operation: &str,
    offset: usize,
    requested: u32,
) -> Result<()> {
    if requested > 4096 || offset >= 4096 {
        return Err("invalid query page boundary".into());
    }
    if offset != 0 {
        let state = host
            .host
            .store_local()
            .lookup_read_capture(crate::query_capture_v2::SUBJECT, operation)?;
        if !state.is_some_and(|state| state.phase() == morrow_core::read_capture::Phase::Ready) {
            return Err("query continuation requires an existing ready result".into());
        }
    }
    Ok(())
}
