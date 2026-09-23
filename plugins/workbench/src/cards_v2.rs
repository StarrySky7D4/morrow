//! Pure format-2 edits for common card fields. The owner binds this projection
//! to a source revision, attachments and an authorized package before commit.
use crate::{Asset, tasks_v2};
use prost::encoding::{
    DecodeContext, WireType, decode_key, decode_varint, encode_key, encode_varint, skip_field,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fields {
    pub title: String,
    pub description: String,
    pub hypothesis: String,
    pub conclusion: String,
    pub icon: u16,
    pub color: u32,
    pub assets: Vec<Asset>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Edit(Fields),
    SetFavorite(bool),
    SetCategory { category: String, stage: String },
    Delete { now_ms: u64 },
    Restore { now_ms: u64 },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Output {
    pub title: String,
    pub properties: Vec<u8>,
}
fn fields(mut bytes: &[u8]) -> Result<Vec<(u32, &[u8], &[u8])>, &'static str> {
    let mut result = Vec::new();
    while !bytes.is_empty() {
        let start = bytes;
        let (tag, wire) = decode_key(&mut bytes).map_err(|_| "field key")?;
        let after_key = bytes;
        skip_field(wire, tag, &mut bytes, DecodeContext::default()).map_err(|_| "field value")?;
        let raw = &start[..start.len() - bytes.len()];
        let mut payload = &after_key[..after_key.len() - bytes.len()];
        if wire == WireType::LengthDelimited {
            let length = decode_varint(&mut payload).map_err(|_| "field length")?;
            if length != payload.len() as u64 {
                return Err("field length mismatch");
            }
        }
        result.push((tag, raw, payload));
    }
    Ok(result)
}
fn length_field(tag: u32, value: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(value.len() + 8);
    encode_key(tag, WireType::LengthDelimited, &mut out);
    encode_varint(value.len() as u64, &mut out);
    out.extend_from_slice(value);
    out
}
fn varint_field(tag: u32, value: u64) -> Vec<u8> {
    let mut out = Vec::new();
    encode_key(tag, WireType::Varint, &mut out);
    encode_varint(value, &mut out);
    out
}
fn replace_many(
    bytes: &[u8],
    replacements: &[(u32, Vec<Vec<u8>>)],
) -> Result<Vec<u8>, &'static str> {
    let mut out = Vec::with_capacity(bytes.len());
    for (tag, raw, _) in fields(bytes)? {
        if !replacements.iter().any(|(target, _)| *target == tag) {
            out.extend_from_slice(raw);
        }
    }
    for (_, values) in replacements {
        for value in values {
            out.extend_from_slice(value);
        }
    }
    Ok(out)
}
fn asset_bytes(asset: &Asset, existing: Option<&[u8]>) -> Result<Vec<u8>, &'static str> {
    let replacements = [
        (1, vec![length_field(1, asset.id.as_bytes())]),
        (2, vec![length_field(2, asset.name.as_bytes())]),
        (3, vec![length_field(3, asset.kind.as_bytes())]),
        (4, vec![varint_field(4, asset.bytes)]),
    ];
    if let Some(existing) = existing {
        replace_many(existing, &replacements)
    } else {
        Ok(replacements
            .into_iter()
            .flat_map(|(_, values)| values.into_iter().flatten())
            .collect())
    }
}
pub fn apply(
    id: &str,
    title: &str,
    properties: &[u8],
    command: &Command,
) -> Result<Output, &'static str> {
    let previous = tasks_v2::decode(id, title, properties)?;
    if previous.deleted && !matches!(command, Command::Restore { .. }) {
        return Err("deleted card requires restore");
    }
    let mut next_title = title.to_owned();
    let replacements = match command {
        Command::Edit(next) => {
            next_title = next.title.clone();
            let old_assets: Vec<_> = fields(properties)?
                .into_iter()
                .filter(|f| f.0 == 10)
                .map(|f| f.2)
                .collect();
            if old_assets.len() != previous.assets.len() {
                return Err("asset field count");
            }
            let mut asset_fields = Vec::with_capacity(next.assets.len());
            for asset in &next.assets {
                let old = previous.assets.iter().position(|a| a.id == asset.id);
                let bytes = if let Some(index) = old {
                    let prior = &previous.assets[index];
                    if prior.name == asset.name
                        && prior.kind == asset.kind
                        && prior.bytes == asset.bytes
                    {
                        old_assets[index].to_vec()
                    } else {
                        asset_bytes(asset, Some(old_assets[index]))?
                    }
                } else {
                    asset_bytes(asset, None)?
                };
                asset_fields.push(length_field(10, &bytes));
            }
            vec![
                (2, vec![length_field(2, next.description.as_bytes())]),
                (5, vec![length_field(5, next.hypothesis.as_bytes())]),
                (6, vec![length_field(6, next.conclusion.as_bytes())]),
                (10, asset_fields),
                (11, vec![varint_field(11, next.icon as u64)]),
                (12, vec![varint_field(12, next.color as u64)]),
            ]
        }
        Command::SetFavorite(value) => vec![(7, vec![varint_field(7, *value as u64)])],
        Command::SetCategory { category, stage } => {
            if !matches!(category.as_str(), "灵感" | "进行中" | "实验")
                || !crate::stages(category).contains(&stage.as_str())
            {
                return Err("invalid category or stage");
            }
            vec![
                (3, vec![length_field(3, category.as_bytes())]),
                (4, vec![length_field(4, stage.as_bytes())]),
            ]
        }
        Command::Delete { now_ms } => {
            if *now_ms == 0 {
                return Err("invalid delete time");
            }
            vec![
                (13, vec![varint_field(13, 1)]),
                (14, vec![varint_field(14, *now_ms)]),
            ]
        }
        Command::Restore { now_ms } => {
            if !previous.deleted
                || *now_ms < previous.deleted_at
                || *now_ms - previous.deleted_at >= 8000
            {
                return Err("restore window expired");
            }
            vec![
                (13, vec![varint_field(13, 0)]),
                (14, vec![varint_field(14, 0)]),
            ]
        }
    };
    let body = replace_many(properties, &replacements)?;
    if body.len() > tasks_v2::MAX_BYTES {
        return Err("V2 properties budget");
    }
    tasks_v2::decode(id, &next_title, &body)?;
    Ok(Output {
        title: next_title,
        properties: body,
    })
}
