//! Versioned guest boundary; every result fits the existing 64 KiB value budget.
use crate::{
    tasks_capnp as wire,
    tasks_v2::{self as v2, Baseline, Command},
};
use capnp::{
    message::{Builder, ReaderOptions},
    serialize,
};
use sha2::{Digest, Sha256};
pub fn digest() -> [u8; 32] {
    Sha256::digest(
        include_str!("../schemas/tasks.capnp")
            .replace("\r\n", "\n")
            .as_bytes(),
    )
    .into()
}
fn text(value: capnp::Result<capnp::text::Reader<'_>>) -> Result<String, &'static str> {
    value
        .map_err(|_| "text")?
        .to_str()
        .map(str::to_owned)
        .map_err(|_| "UTF8")
}
pub fn process(bytes: &[u8]) -> Result<Vec<u8>, &'static str> {
    if bytes.len() > 65536 {
        return Err("task request budget");
    }
    let mut cursor = std::io::Cursor::new(bytes);
    let message = serialize::read_message(
        &mut cursor,
        ReaderOptions {
            traversal_limit_in_words: Some(16384),
            nesting_limit: 16,
        },
    )
    .map_err(|_| "task frame")?;
    if cursor.position() != bytes.len() as u64 {
        return Err("trailing task bytes");
    }
    let r = message
        .get_root::<wire::request::Reader>()
        .map_err(|_| "task request")?;
    if r.get_version() != 2 || r.get_digest().map_err(|_| "digest")? != digest() {
        return Err("task contract mismatch");
    }
    let id = text(r.get_card_id())?;
    let title = text(r.get_title())?;
    let previous = r.get_properties().map_err(|_| "properties")?;
    let action = r.get_action().map_err(|_| "unknown task action")?;
    let command = match action {
        wire::Action::SetCompletion => Some(Command::SetCompletion {
            id: text(r.get_task_id())?,
            complete: r.get_complete(),
        }),
        wire::Action::Rename => Some(Command::Rename {
            id: text(r.get_task_id())?,
            text: text(r.get_text())?,
        }),
        wire::Action::SetStage => Some(Command::SetStage(text(r.get_text())?)),
        wire::Action::CompleteAllAndSetStage => {
            Some(Command::CompleteAllAndSetStage(text(r.get_text())?))
        }
        wire::Action::Add => Some(Command::Add {
            id: text(r.get_task_id())?,
            text: text(r.get_text())?,
        }),
        wire::Action::Remove => Some(Command::Remove(text(r.get_task_id())?)),
        wire::Action::Reorder => {
            let order = r.get_order().map_err(|_| "order")?;
            if order.len() > 128 {
                return Err("task order budget");
            }
            Some(Command::Reorder(
                order.iter().map(text).collect::<Result<_, _>>()?,
            ))
        }
        wire::Action::Migrate | wire::Action::Project => None,
    };
    let (body, properties) = if action == wire::Action::Migrate {
        let base = Baseline {
            card_id: id.clone(),
            revision: r.get_base_revision(),
            sha256: r
                .get_source_digest()
                .map_err(|_| "source digest")?
                .try_into()
                .map_err(|_| "source digest")?,
        };
        v2::migrate_with_view(&base, &id, r.get_revision(), &title, previous)?
    } else if let Some(command) = command {
        v2::apply_with_view(&id, &title, previous, command)?
    } else {
        (previous.to_vec(), v2::decode(&id, &title, previous)?)
    };
    let projection = v2::project_validated(&properties);
    let mut message = Builder::new_default();
    {
        let mut out = message.init_root::<wire::response::Builder>();
        out.set_version(2);
        out.set_digest(&digest());
        out.set_properties(&body);
        out.set_stage(projection.stage.as_str());
        out.set_complete(projection.complete);
        out.set_incomplete(projection.incomplete);
        out.set_ambiguous(projection.ambiguous);
        let mut tasks = out.init_tasks(properties.tasks.len() as u32);
        for (index, t) in properties.tasks.iter().enumerate() {
            let mut task = tasks.reborrow().get(index as u32);
            task.set_id(t.id.as_str());
            task.set_text(t.text.as_str());
            task.set_completion(t.completion as u16);
        }
    }
    let bytes = serialize::write_message_to_words(&message);
    if bytes.len() > 65536 {
        return Err("task response budget");
    }
    Ok(bytes)
}
