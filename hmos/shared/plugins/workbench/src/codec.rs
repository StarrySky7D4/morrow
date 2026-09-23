use crate::{Asset, Idea, Request, Response, digest, workbench_capnp as wire};
use capnp::{
    message::{Builder, ReaderOptions},
    serialize,
};
fn read_text(value: capnp::Result<capnp::text::Reader<'_>>) -> Result<String, &'static str> {
    value
        .map_err(|_| "text")?
        .to_str()
        .map(str::to_owned)
        .map_err(|_| "utf8")
}
fn reader(bytes: &[u8]) -> Result<capnp::message::Reader<serialize::OwnedSegments>, &'static str> {
    if bytes.len() > 65536 {
        return Err("message budget");
    }
    let mut cursor = std::io::Cursor::new(bytes);
    let r = serialize::read_message(
        &mut cursor,
        ReaderOptions {
            traversal_limit_in_words: Some(8192),
            nesting_limit: 16,
        },
    )
    .map_err(|_| "frame")?;
    if cursor.position() != bytes.len() as u64 {
        return Err("trailing bytes");
    }
    Ok(r)
}
fn finish(m: &Builder<capnp::message::HeapAllocator>) -> Result<Vec<u8>, &'static str> {
    let b = serialize::write_message_to_words(m);
    if b.len() > 65536 {
        Err("message budget")
    } else {
        Ok(b)
    }
}
fn read_idea(r: wire::idea::Reader<'_>) -> Result<Idea, &'static str> {
    let mut v = Idea {
        id: read_text(r.get_id())?,
        title: read_text(r.get_title())?,
        description: read_text(r.get_description())?,
        category: read_text(r.get_category())?,
        stage: read_text(r.get_stage())?,
        hypothesis: read_text(r.get_hypothesis())?,
        conclusion: read_text(r.get_conclusion())?,
        favorite: r.get_favorite(),
        icon: r.get_icon(),
        color: r.get_color(),
        deleted: r.get_deleted(),
        deleted_at: r.get_deleted_at(),
        ..Default::default()
    };
    let items = r.get_todos().map_err(|_| "list")?;
    if items.len() > 128 {
        return Err("list budget");
    }
    for item in items {
        v.todos.push(read_text(item)?);
    }
    let items = r.get_completed().map_err(|_| "list")?;
    if items.len() > 128 {
        return Err("list budget");
    }
    for item in items {
        v.completed.push(read_text(item)?);
    }
    let assets = r.get_assets().map_err(|_| "assets")?;
    if assets.len() > 20 {
        return Err("asset budget");
    }
    for a in assets {
        v.assets.push(Asset {
            id: read_text(a.get_id())?,
            name: read_text(a.get_name())?,
            kind: read_text(a.get_kind())?,
            bytes: a.get_bytes(),
        });
    }
    Ok(v)
}
fn write_idea(mut b: wire::idea::Builder<'_>, v: &Idea) {
    b.set_id(v.id.as_str());
    b.set_title(v.title.as_str());
    b.set_description(v.description.as_str());
    b.set_category(v.category.as_str());
    b.set_stage(v.stage.as_str());
    b.set_hypothesis(v.hypothesis.as_str());
    b.set_conclusion(v.conclusion.as_str());
    b.set_favorite(v.favorite);
    b.set_icon(v.icon);
    b.set_color(v.color);
    b.set_deleted(v.deleted);
    b.set_deleted_at(v.deleted_at);
    {
        let mut items = b.reborrow().init_todos(v.todos.len() as u32);
        for (i, t) in v.todos.iter().enumerate() {
            items.set(i as u32, t.as_str());
        }
    }
    {
        let mut items = b.reborrow().init_completed(v.completed.len() as u32);
        for (i, t) in v.completed.iter().enumerate() {
            items.set(i as u32, t.as_str());
        }
    }
    let mut assets = b.init_assets(v.assets.len() as u32);
    for (i, a) in v.assets.iter().enumerate() {
        let mut item = assets.reborrow().get(i as u32);
        item.set_id(a.id.as_str());
        item.set_name(a.name.as_str());
        item.set_kind(a.kind.as_str());
        item.set_bytes(a.bytes);
    }
}
pub fn decode_request(bytes: &[u8]) -> Result<Request, &'static str> {
    let m = reader(bytes)?;
    let r = m
        .get_root::<wire::request::Reader>()
        .map_err(|_| "request")?;
    if r.get_version() != 1 || r.get_digest().map_err(|_| "digest")? != digest() {
        return Err("contract mismatch");
    }
    let ideas = r.get_ideas().map_err(|_| "ideas")?;
    if ideas.len() > 128 {
        return Err("query budget");
    }
    Ok(Request {
        action: r.get_action().map_err(|_| "action")?,
        current: read_idea(r.get_current().map_err(|_| "current")?)?,
        proposed: read_idea(r.get_proposed().map_err(|_| "proposed")?)?,
        text: read_text(r.get_text())?,
        flag: r.get_flag(),
        now_ms: r.get_now_ms(),
        ideas: ideas.iter().map(read_idea).collect::<Result<Vec<_>, _>>()?,
        section: read_text(r.get_section())?,
        filter: read_text(r.get_filter())?,
        sort: read_text(r.get_sort())?,
    })
}
pub fn encode_request(v: &Request) -> Result<Vec<u8>, &'static str> {
    let mut m = Builder::new_default();
    let mut r = m.init_root::<wire::request::Builder>();
    r.set_version(1);
    r.set_digest(&digest());
    r.set_action(v.action);
    write_idea(r.reborrow().init_current(), &v.current);
    write_idea(r.reborrow().init_proposed(), &v.proposed);
    r.set_text(v.text.as_str());
    r.set_flag(v.flag);
    r.set_now_ms(v.now_ms);
    r.set_section(v.section.as_str());
    r.set_filter(v.filter.as_str());
    r.set_sort(v.sort.as_str());
    let mut ideas = r.init_ideas(v.ideas.len() as u32);
    for (i, idea) in v.ideas.iter().enumerate() {
        write_idea(ideas.reborrow().get(i as u32), idea);
    }
    finish(&m)
}
pub fn encode_response(v: &Response) -> Result<Vec<u8>, &'static str> {
    let mut m = Builder::new_default();
    let mut r = m.init_root::<wire::response::Builder>();
    r.set_version(1);
    r.set_digest(&digest());
    write_idea(r.reborrow().init_idea(), &v.idea);
    let mut ids = r.init_ids(v.ids.len() as u32);
    for (i, id) in v.ids.iter().enumerate() {
        ids.set(i as u32, id.as_str());
    }
    finish(&m)
}
pub fn decode_response(bytes: &[u8]) -> Result<Response, &'static str> {
    let m = reader(bytes)?;
    let r = m
        .get_root::<wire::response::Reader>()
        .map_err(|_| "response")?;
    if r.get_version() != 1 || r.get_digest().map_err(|_| "digest")? != digest() {
        return Err("contract mismatch");
    }
    let ids = r.get_ids().map_err(|_| "ids")?;
    if ids.len() > 128 {
        return Err("query budget");
    }
    Ok(Response {
        idea: read_idea(r.get_idea().map_err(|_| "idea")?)?,
        ids: ids.iter().map(read_text).collect::<Result<Vec<_>, _>>()?,
    })
}
