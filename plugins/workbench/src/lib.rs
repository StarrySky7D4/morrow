//! test.1 workspace business rules, executed by the ordinary Rust Wasm guest.
#[cfg(target_arch = "wasm32")]
use morrow_plugin_sdk::{task::FailureCode, wasm};
use sha2::{Digest, Sha256};
pub mod capture;
pub mod codec;
pub mod persistence;
pub mod preferences;
pub mod ui;
#[allow(clippy::all)]
pub mod capture_capnp {
    include!(concat!(env!("OUT_DIR"), "/capture_capnp.rs"));
}
pub mod services;
#[allow(clippy::all)]
pub mod studio_capnp {
    include!(concat!(env!("OUT_DIR"), "/studio_capnp.rs"));
}
#[allow(clippy::all)]
pub mod workbench_capnp {
    include!(concat!(env!("OUT_DIR"), "/workbench_capnp.rs"));
}
pub use workbench_capnp::Action;
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Asset {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub bytes: u64,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Idea {
    pub id: String,
    pub title: String,
    pub description: String,
    pub category: String,
    pub stage: String,
    pub hypothesis: String,
    pub conclusion: String,
    pub favorite: bool,
    pub todos: Vec<String>,
    pub completed: Vec<String>,
    pub assets: Vec<Asset>,
    pub icon: u16,
    pub color: u32,
    pub deleted: bool,
    pub deleted_at: u64,
}
#[derive(Clone, Debug)]
pub struct Request {
    pub action: Action,
    pub current: Idea,
    pub proposed: Idea,
    pub text: String,
    pub flag: bool,
    pub now_ms: u64,
    pub ideas: Vec<Idea>,
    pub section: String,
    pub filter: String,
    pub sort: String,
}
#[derive(Clone, Debug, Default)]
pub struct Response {
    pub idea: Idea,
    pub ids: Vec<String>,
}
pub fn digest() -> [u8; 32] {
    Sha256::digest(
        include_str!("../schemas/workbench.capnp")
            .replace("\r\n", "\n")
            .as_bytes(),
    )
    .into()
}
fn valid_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 256
        && !s
            .chars()
            .any(|c| c.is_control() || matches!(c, '/' | '\\' | ':'))
}
fn stages(category: &str) -> &'static [&'static str] {
    match category {
        "进行中" => &["计划中", "推进中", "已完成"],
        "实验" => &["待验证", "验证中", "已记录"],
        _ => &["待整理", "已整理"],
    }
}
impl Idea {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !valid_id(&self.id)
            || self.icon > 3
            || !matches!(self.category.as_str(), "灵感" | "进行中" | "实验")
            || !stages(&self.category).contains(&self.stage.as_str())
        {
            return Err("invalid idea identity or category");
        }
        if self.title.len() > 16384
            || self.description.len() > 65536
            || self.hypothesis.len() > 16384
            || self.conclusion.len() > 16384
            || self.todos.len() > 128
            || self.assets.len() > 20
        {
            return Err("idea budget exceeded");
        }
        if self.title.trim().is_empty() && self.assets.is_empty() {
            return Err("title or attachment required");
        }
        if self.todos.iter().any(|t| t.is_empty() || t.len() > 2048)
            || self.completed.iter().any(|t| !self.todos.contains(t))
        {
            return Err("invalid checklist");
        }
        let mut ids = std::collections::BTreeSet::new();
        for a in &self.assets {
            if !valid_id(&a.id)
                || a.name.len() > 4096
                || a.bytes > 200 * 1024 * 1024
                || !matches!(
                    a.kind.as_str(),
                    "image" | "gif" | "video" | "audio" | "file"
                )
                || !ids.insert(&a.id)
            {
                return Err("invalid attachment");
            }
        }
        Ok(())
    }
    pub fn project_stage(&self) -> &str {
        if !self.todos.is_empty() && self.todos.iter().all(|t| self.completed.contains(t)) {
            "已完成"
        } else if stages("进行中").contains(&self.stage.as_str()) {
            &self.stage
        } else {
            "推进中"
        }
    }
}
pub fn execute(r: Request) -> Result<Response, &'static str> {
    if r.action == Action::Query {
        if r.ideas.len() > 128 || r.text.len() > 16384 {
            return Err("query budget exceeded");
        }
        let q = r.text.to_lowercase();
        let mut matches = Vec::new();
        for idea in &r.ideas {
            idea.validate()?;
            if idea.deleted {
                continue;
            }
            let section = match r.section.as_str() {
                "灵感收件箱" => idea.category == "灵感",
                "小项目" => idea.category == "进行中",
                "实验室" => idea.category == "实验",
                "已收藏" => idea.favorite,
                "概览" => true,
                _ => return Err("unknown section"),
            };
            let filter = match r.filter.as_str() {
                "全部" => true,
                "有待办" => idea.todos.iter().any(|t| !idea.completed.contains(t)),
                "含附件" => !idea.assets.is_empty(),
                "仅收藏" => idea.favorite,
                "图像" => idea
                    .assets
                    .iter()
                    .any(|a| matches!(a.kind.as_str(), "image" | "gif")),
                "音视频" => idea
                    .assets
                    .iter()
                    .any(|a| matches!(a.kind.as_str(), "audio" | "video")),
                "文件" => idea.assets.iter().any(|a| a.kind == "file"),
                "文字" => idea.assets.is_empty(),
                stage => {
                    (if r.section == "小项目" {
                        idea.project_stage()
                    } else {
                        &idea.stage
                    }) == stage
                }
            };
            let searchable = format!(
                "{} {} {} {} {}",
                idea.title,
                idea.description,
                idea.hypothesis,
                idea.conclusion,
                idea.assets
                    .iter()
                    .map(|a| a.name.as_str())
                    .collect::<Vec<_>>()
                    .join(" ")
            )
            .to_lowercase();
            if section && filter && searchable.contains(&q) {
                matches.push(idea);
            }
        }
        match r.sort.as_str() {
            "标题排序" => {
                matches.sort_by(|a, b| a.title.encode_utf16().cmp(b.title.encode_utf16()))
            }
            "收藏优先" => matches.sort_by_key(|i| !i.favorite),
            "最近添加" => {}
            _ => return Err("unknown sort"),
        }
        return Ok(Response {
            idea: Idea::default(),
            ids: matches.iter().map(|i| i.id.clone()).collect(),
        });
    }
    if r.action == Action::Create {
        let mut idea = r.proposed;
        if idea.stage.is_empty() {
            idea.stage = if idea.category == "进行中" {
                "推进中"
            } else if idea.category == "实验" {
                "待验证"
            } else {
                "待整理"
            }
            .into();
        }
        idea.deleted = false;
        idea.deleted_at = 0;
        idea.validate()?;
        return Ok(Response { idea, ids: vec![] });
    }
    r.current.validate()?;
    if r.current.deleted && r.action != Action::Restore {
        return Err("idea deleted");
    }
    let mut next = r.current.clone();
    match r.action {
        Action::Edit => {
            if r.proposed.id != next.id {
                return Err("edit identity mismatch");
            }
            next = r.proposed;
            next.favorite = r.current.favorite;
            next.completed = r
                .current
                .completed
                .into_iter()
                .filter(|t| next.todos.contains(t))
                .collect();
            next.deleted = false;
            next.deleted_at = 0;
            if !stages(&next.category).contains(&next.stage.as_str()) {
                next.stage = stages(&next.category)[0].into();
            }
        }
        Action::Favorite => next.favorite = r.flag,
        Action::Todo => {
            if !next.todos.contains(&r.text) {
                return Err("unknown checklist item");
            }
            next.completed.retain(|t| t != &r.text);
            if r.flag {
                next.completed.push(r.text);
            } else if next.category == "进行中" && next.stage == "已完成" {
                next.stage = "推进中".into();
            }
        }
        Action::Stage => {
            if !stages(&next.category).contains(&r.text.as_str()) {
                return Err("invalid stage");
            }
            next.stage = r.text;
            if next.category == "进行中" {
                if next.stage == "已完成" {
                    next.completed = next.todos.clone();
                } else if !next.todos.is_empty()
                    && next.todos.iter().all(|t| next.completed.contains(t))
                {
                    let last = next.todos.last().unwrap();
                    next.completed.retain(|t| t != last);
                }
            }
        }
        Action::ToProject => {
            next.category = "进行中".into();
            next.stage = "计划中".into();
        }
        Action::Delete => {
            if r.now_ms == 0 {
                return Err("missing host clock");
            }
            next.deleted = true;
            next.deleted_at = r.now_ms;
        }
        Action::Restore => {
            if !next.deleted || r.now_ms < next.deleted_at || r.now_ms - next.deleted_at >= 8000 {
                return Err("undo expired");
            }
            next.deleted = false;
            next.deleted_at = 0;
        }
        _ => return Err("invalid action"),
    }
    next.validate()?;
    Ok(Response {
        idea: next,
        ids: vec![],
    })
}
#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn morrow_run() -> i32 {
    let Ok(task) = wasm::read_task() else {
        return -1;
    };
    let Some(t) = task.transform() else {
        return -1;
    };
    let result = if t.output_type == "morrow.ui.document.v1"
        && (t.handler == "ui.form" || t.handler == "ui.edit")
    {
        ui::process(&t.handler, &t.input_type, &t.input)
    } else if t.handler == "workbench.command"
        && t.input_type == "morrow.workbench.request.v1"
        && t.output_type == "morrow.workbench.response.v1"
    {
        codec::decode_request(&t.input)
            .and_then(execute)
            .and_then(|r| codec::encode_response(&r))
    } else if t.handler == "studio.command"
        && t.input_type == "morrow.studio.request.v1"
        && t.output_type == "morrow.studio.response.v1"
    {
        services::process(&t.input)
    } else if t.handler == "studio.preferences"
        && t.input_type == "morrow.studio.preferences.v1"
        && t.output_type == "morrow.studio.preferences.v1"
    {
        preferences::decode_wire(&t.input).and_then(|v| preferences::encode_wire(&v))
    } else if t.handler == "capture.convert"
        && t.input_type == "morrow.capture.request.v1"
        && t.output_type == "morrow.capture.response.v1"
    {
        capture::process(&t.input)
    } else {
        Err("unregistered business handler")
    };
    let done = match result {
        Ok(bytes) => wasm::complete_output(&task, &bytes),
        Err(message) => wasm::complete_failure(&task, FailureCode::InvalidInput, message),
    };
    if done.is_ok() { 0 } else { -1 }
}
