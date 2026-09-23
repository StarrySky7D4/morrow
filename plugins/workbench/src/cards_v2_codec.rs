//! Canonical, bounded Cap'n Proto boundary for pure format-2 common card edits.
use crate::{
    Asset,
    cards_v2::{self, Command, Fields, Output},
    cards_v2_capnp as wire,
};
use capnp::{
    message::{Builder, ReaderOptions},
    serialize,
};
use sha2::{Digest, Sha256};

const MAX_BYTES: usize = 65536;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub id: String,
    pub title: String,
    pub properties: Vec<u8>,
    pub command: Command,
}
pub fn digest() -> [u8; 32] {
    Sha256::digest(
        include_str!("../schemas/cards_v2.capnp")
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
fn frame(
    bytes: &[u8],
) -> Result<capnp::message::Reader<capnp::serialize::OwnedSegments>, &'static str> {
    if bytes.len() > MAX_BYTES {
        return Err("card frame budget");
    }
    let mut cursor = std::io::Cursor::new(bytes);
    let message = serialize::read_message(
        &mut cursor,
        ReaderOptions {
            traversal_limit_in_words: Some(16384),
            nesting_limit: 16,
        },
    )
    .map_err(|_| "card frame")?;
    if cursor.position() != bytes.len() as u64 {
        return Err("trailing card bytes");
    }
    Ok(message)
}
pub fn encode_request(
    id: &str,
    title: &str,
    properties: &[u8],
    command: &Command,
) -> Result<Vec<u8>, &'static str> {
    if properties.len() > MAX_BYTES {
        return Err("card properties budget");
    }
    // Validate both the source and the proposed result before guest execution.
    cards_v2::apply(id, title, properties, command)?;
    let mut message = Builder::new_default();
    {
        let mut r = message.init_root::<wire::request::Builder>();
        r.set_version(2);
        r.set_digest(&digest());
        r.set_card_id(id);
        r.set_title(title);
        r.set_properties(properties);
        match command {
            Command::Edit(f) => {
                r.set_action(wire::Action::Edit);
                r.set_edit_title(&f.title);
                r.set_description(&f.description);
                r.set_hypothesis(&f.hypothesis);
                r.set_conclusion(&f.conclusion);
                r.set_icon(f.icon);
                r.set_color(f.color);
                let mut assets = r.init_assets(f.assets.len() as u32);
                for (i, a) in f.assets.iter().enumerate() {
                    let mut item = assets.reborrow().get(i as u32);
                    item.set_id(&a.id);
                    item.set_name(&a.name);
                    item.set_kind(&a.kind);
                    item.set_bytes(a.bytes);
                }
            }
            Command::SetFavorite(value) => {
                r.set_action(wire::Action::SetFavorite);
                r.set_favorite(*value);
            }
            Command::SetCategory { category, stage } => {
                r.set_action(wire::Action::SetCategory);
                r.set_category(category);
                r.set_stage(stage);
            }
            Command::Delete { now_ms } => {
                r.set_action(wire::Action::Delete);
                r.set_now_ms(*now_ms);
            }
            Command::Restore { now_ms } => {
                r.set_action(wire::Action::Restore);
                r.set_now_ms(*now_ms);
            }
        }
    }
    let bytes = serialize::write_message_to_words(&message);
    if bytes.len() > MAX_BYTES {
        return Err("card request budget");
    }
    Ok(bytes)
}
pub fn decode_request(bytes: &[u8]) -> Result<Request, &'static str> {
    let message = frame(bytes)?;
    let r = message
        .get_root::<wire::request::Reader>()
        .map_err(|_| "card request")?;
    if r.get_version() != 2 || r.get_digest().map_err(|_| "digest")? != digest() {
        return Err("card contract mismatch");
    }
    let id = text(r.get_card_id())?;
    let title = text(r.get_title())?;
    let properties = r.get_properties().map_err(|_| "properties")?.to_vec();
    let command = match r.get_action().map_err(|_| "unknown card action")? {
        wire::Action::Edit => {
            let list = r.get_assets().map_err(|_| "assets")?;
            if list.len() > 20 {
                return Err("asset budget");
            }
            let mut assets = Vec::with_capacity(list.len() as usize);
            for a in list.iter() {
                assets.push(Asset {
                    id: text(a.get_id())?,
                    name: text(a.get_name())?,
                    kind: text(a.get_kind())?,
                    bytes: a.get_bytes(),
                });
            }
            Command::Edit(Fields {
                title: text(r.get_edit_title())?,
                description: text(r.get_description())?,
                hypothesis: text(r.get_hypothesis())?,
                conclusion: text(r.get_conclusion())?,
                icon: r.get_icon(),
                color: r.get_color(),
                assets,
            })
        }
        wire::Action::SetFavorite => Command::SetFavorite(r.get_favorite()),
        wire::Action::SetCategory => Command::SetCategory {
            category: text(r.get_category())?,
            stage: text(r.get_stage())?,
        },
        wire::Action::Delete => Command::Delete {
            now_ms: r.get_now_ms(),
        },
        wire::Action::Restore => Command::Restore {
            now_ms: r.get_now_ms(),
        },
    };
    let request = Request {
        id,
        title,
        properties,
        command,
    };
    if encode_request(
        &request.id,
        &request.title,
        &request.properties,
        &request.command,
    )? != bytes
    {
        return Err("noncanonical card request");
    }
    Ok(request)
}
fn encode_response(output: &Output) -> Result<Vec<u8>, &'static str> {
    if output.properties.len() > MAX_BYTES {
        return Err("card properties budget");
    }
    let mut message = Builder::new_default();
    {
        let mut r = message.init_root::<wire::response::Builder>();
        r.set_version(2);
        r.set_digest(&digest());
        r.set_title(&output.title);
        r.set_properties(&output.properties);
    }
    let bytes = serialize::write_message_to_words(&message);
    if bytes.len() > MAX_BYTES {
        return Err("card response budget");
    }
    Ok(bytes)
}
pub fn process(bytes: &[u8]) -> Result<Vec<u8>, &'static str> {
    let r = decode_request(bytes)?;
    encode_response(&cards_v2::apply(
        &r.id,
        &r.title,
        &r.properties,
        &r.command,
    )?)
}
pub fn decode_response(bytes: &[u8]) -> Result<Output, &'static str> {
    let message = frame(bytes)?;
    let r = message
        .get_root::<wire::response::Reader>()
        .map_err(|_| "card response")?;
    if r.get_version() != 2 || r.get_digest().map_err(|_| "digest")? != digest() {
        return Err("card contract mismatch");
    }
    let output = Output {
        title: text(r.get_title())?,
        properties: r.get_properties().map_err(|_| "properties")?.to_vec(),
    };
    if encode_response(&output)? != bytes {
        return Err("noncanonical card response");
    }
    Ok(output)
}
