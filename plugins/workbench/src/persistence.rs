//! Persistence adapter used by the trusted host at the storage boundary only.
use crate::{Asset, Idea};
use prost::Message;
use prost_reflect::{DescriptorPool, DynamicMessage, Value};
use std::sync::OnceLock;
mod proto {
    include!(concat!(env!("OUT_DIR"), "/morrow.workbench.v1.rs"));
}
fn descriptor() -> prost_reflect::MessageDescriptor {
    static POOL: OnceLock<DescriptorPool> = OnceLock::new();
    POOL.get_or_init(|| {
        DescriptorPool::decode(
            include_bytes!(concat!(env!("OUT_DIR"), "/properties.descriptor.bin")).as_slice(),
        )
        .unwrap()
    })
    .get_message_by_name("morrow.workbench.v1.Properties")
    .unwrap()
}
pub fn decode(id: &str, title: &str, bytes: &[u8]) -> Result<Idea, &'static str> {
    if bytes.len() > 65536 {
        return Err("properties budget");
    }
    let p = proto::Properties::decode(bytes).map_err(|_| "properties")?;
    if p.version != 1 {
        return Err("properties version");
    }
    let idea = Idea {
        id: id.into(),
        title: title.into(),
        description: p.description,
        category: p.category,
        stage: p.stage,
        hypothesis: p.hypothesis,
        conclusion: p.conclusion,
        favorite: p.favorite,
        todos: p.todos,
        completed: p.completed,
        assets: p
            .assets
            .into_iter()
            .map(|a| Asset {
                id: a.id,
                name: a.name,
                kind: a.kind,
                bytes: a.bytes,
            })
            .collect(),
        icon: u16::try_from(p.icon).map_err(|_| "icon")?,
        color: p.color,
        deleted: p.deleted,
        deleted_at: p.deleted_at,
    };
    idea.validate()?;
    Ok(idea)
}
pub fn encode(idea: &Idea, previous: Option<&[u8]>) -> Result<Vec<u8>, &'static str> {
    idea.validate()?;
    let p = proto::Properties {
        version: 1,
        description: idea.description.clone(),
        category: idea.category.clone(),
        stage: idea.stage.clone(),
        hypothesis: idea.hypothesis.clone(),
        conclusion: idea.conclusion.clone(),
        favorite: idea.favorite,
        todos: idea.todos.clone(),
        completed: idea.completed.clone(),
        assets: idea
            .assets
            .iter()
            .map(|a| proto::AssetInfo {
                id: a.id.clone(),
                name: a.name.clone(),
                kind: a.kind.clone(),
                bytes: a.bytes,
            })
            .collect(),
        icon: idea.icon.into(),
        color: idea.color,
        deleted: idea.deleted,
        deleted_at: idea.deleted_at,
    };
    let d = descriptor();
    let next = DynamicMessage::decode(d.clone(), p.encode_to_vec().as_slice())
        .map_err(|_| "properties")?;
    let mut result = if let Some(bytes) = previous {
        decode(&idea.id, &idea.title, bytes)?;
        DynamicMessage::decode(d.clone(), bytes).map_err(|_| "properties")?
    } else {
        DynamicMessage::new(d.clone())
    };
    let old_assets = result.get_field_by_name("assets").unwrap().into_owned();
    for field in d.fields() {
        result.set_field(&field, next.get_field(&field).into_owned());
    }
    // Preserve extension fields on retained asset identities as well as top level.
    let mut merged = Vec::new();
    for item in next.get_field_by_name("assets").unwrap().as_list().unwrap() {
        let asset = item.as_message().unwrap();
        let id = asset.get_field_by_name("id").unwrap().into_owned();
        let old = old_assets
            .as_list()
            .unwrap()
            .iter()
            .filter_map(Value::as_message)
            .find(|v| v.get_field_by_name("id").unwrap().as_ref() == &id);
        let mut out = old.cloned().unwrap_or_else(|| asset.clone());
        use prost_reflect::ReflectMessage;
        for field in asset.descriptor().fields() {
            out.set_field(&field, asset.get_field(&field).into_owned());
        }
        merged.push(Value::Message(out));
    }
    result.set_field_by_name("assets", Value::List(merged));
    let raw = result.encode_to_vec();
    if raw.len() > 65536 {
        return Err("properties budget");
    }
    Ok(raw)
}
