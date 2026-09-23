//! Canonical bounded wire codec for the mixed-format query guest.
use crate::{
    query_v2::{self, Candidate, Conditions, Request, Response, SortKey},
    query_v2_capnp as wire,
};
use capnp::{
    message::{Builder, ReaderOptions},
    serialize,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

const MAX_BYTES: usize = 65536;
pub fn digest() -> [u8; 32] {
    Sha256::digest(
        include_str!("../schemas/query_v2.capnp")
            .replace("\r\n", "\n")
            .as_bytes(),
    )
    .into()
}
fn text(value: capnp::Result<capnp::text::Reader<'_>>) -> Result<String, &'static str> {
    value
        .map_err(|_| "query text")?
        .to_str()
        .map(str::to_owned)
        .map_err(|_| "query UTF8")
}
fn frame(
    bytes: &[u8],
) -> Result<capnp::message::Reader<capnp::serialize::OwnedSegments>, &'static str> {
    if bytes.len() > MAX_BYTES {
        return Err("query frame budget");
    }
    let mut cursor = std::io::Cursor::new(bytes);
    let message = serialize::read_message(
        &mut cursor,
        ReaderOptions {
            traversal_limit_in_words: Some(16384),
            nesting_limit: 16,
        },
    )
    .map_err(|_| "query frame")?;
    if cursor.position() != bytes.len() as u64 {
        return Err("trailing query bytes");
    }
    Ok(message)
}
pub fn encode_request(request: &Request) -> Result<Vec<u8>, &'static str> {
    query_v2::validate_request(request)?;
    let mut message = Builder::new_default();
    {
        let mut out = message.init_root::<wire::request::Builder>();
        out.set_version(2);
        out.set_digest(&digest());
        match request {
            Request::Filter {
                conditions,
                candidates,
            } => {
                out.set_action(wire::Action::Filter);
                let mut c = out.reborrow().init_conditions();
                c.set_section(&conditions.section);
                c.set_filter(&conditions.filter);
                c.set_text(&conditions.text);
                c.set_sort(&conditions.sort);
                let mut list = out.init_candidates(candidates.len() as u32);
                for (i, candidate) in candidates.iter().enumerate() {
                    let mut item = list.reborrow().get(i as u32);
                    item.set_id(&candidate.id);
                    item.set_title(&candidate.title);
                    item.set_format_version(candidate.format_version);
                    item.set_properties(&candidate.properties);
                }
            }
            Request::Sort { sort, keys } => {
                out.set_action(wire::Action::Sort);
                out.set_sort(sort);
                let mut list = out.init_keys(keys.len() as u32);
                for (i, key) in keys.iter().enumerate() {
                    let mut item = list.reborrow().get(i as u32);
                    item.set_id(&key.id);
                    item.set_title(&key.title);
                    item.set_favorite(key.favorite);
                }
            }
        }
    }
    let bytes = serialize::write_message_to_words(&message);
    if bytes.len() > MAX_BYTES {
        return Err("query request budget");
    }
    Ok(bytes)
}
pub fn decode_request(bytes: &[u8]) -> Result<Request, &'static str> {
    let message = frame(bytes)?;
    let r = message
        .get_root::<wire::request::Reader>()
        .map_err(|_| "query request")?;
    if r.get_version() != 2 || r.get_digest().map_err(|_| "query digest")? != digest() {
        return Err("query contract mismatch");
    }
    let request = match r.get_action().map_err(|_| "query action")? {
        wire::Action::Filter => {
            let c = r.get_conditions().map_err(|_| "query conditions")?;
            let list = r.get_candidates().map_err(|_| "query candidates")?;
            if list.len() as usize > query_v2::MAX_ITEMS {
                return Err("query item budget");
            }
            let mut candidates = Vec::with_capacity(list.len() as usize);
            for item in list.iter() {
                let properties = item.get_properties().map_err(|_| "query properties")?;
                if properties.len() > MAX_BYTES {
                    return Err("query properties budget");
                }
                candidates.push(Candidate {
                    id: text(item.get_id())?,
                    title: text(item.get_title())?,
                    format_version: item.get_format_version(),
                    properties: properties.to_vec(),
                });
            }
            Request::Filter {
                conditions: Conditions {
                    section: text(c.get_section())?,
                    filter: text(c.get_filter())?,
                    text: text(c.get_text())?,
                    sort: text(c.get_sort())?,
                },
                candidates,
            }
        }
        wire::Action::Sort => {
            let list = r.get_keys().map_err(|_| "query keys")?;
            if list.len() as usize > query_v2::MAX_ITEMS {
                return Err("query item budget");
            }
            let mut keys = Vec::with_capacity(list.len() as usize);
            for item in list.iter() {
                keys.push(SortKey {
                    id: text(item.get_id())?,
                    title: text(item.get_title())?,
                    favorite: item.get_favorite(),
                });
            }
            Request::Sort {
                sort: text(r.get_sort())?,
                keys,
            }
        }
    };
    if encode_request(&request)? != bytes {
        return Err("noncanonical query request");
    }
    Ok(request)
}
pub fn encode_response(response: &Response) -> Result<Vec<u8>, &'static str> {
    if response.ids.len() > query_v2::MAX_ITEMS {
        return Err("query response item budget");
    }
    let mut seen = BTreeSet::new();
    for id in &response.ids {
        if !crate::valid_id(id) || !seen.insert(id) {
            return Err("invalid query response ID");
        }
    }
    let mut message = Builder::new_default();
    {
        let mut r = message.init_root::<wire::response::Builder>();
        r.set_version(2);
        r.set_digest(&digest());
        let mut ids = r.init_ids(response.ids.len() as u32);
        for (i, id) in response.ids.iter().enumerate() {
            ids.set(i as u32, id);
        }
    }
    let bytes = serialize::write_message_to_words(&message);
    if bytes.len() > MAX_BYTES {
        return Err("query response budget");
    }
    Ok(bytes)
}
pub fn decode_response(bytes: &[u8]) -> Result<Response, &'static str> {
    let message = frame(bytes)?;
    let r = message
        .get_root::<wire::response::Reader>()
        .map_err(|_| "query response")?;
    if r.get_version() != 2 || r.get_digest().map_err(|_| "query digest")? != digest() {
        return Err("query contract mismatch");
    }
    let list = r.get_ids().map_err(|_| "query IDs")?;
    if list.len() as usize > query_v2::MAX_ITEMS {
        return Err("query response item budget");
    }
    let response = Response {
        ids: list.iter().map(text).collect::<Result<_, _>>()?,
    };
    if encode_response(&response)? != bytes {
        return Err("noncanonical query response");
    }
    Ok(response)
}
pub fn process(bytes: &[u8]) -> Result<Vec<u8>, &'static str> {
    let request = decode_request(bytes)?;
    encode_response(&query_v2::execute(request)?)
}
