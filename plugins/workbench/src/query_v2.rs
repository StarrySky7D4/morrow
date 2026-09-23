//! Pure mixed-format query semantics. Input order is the owner's snapshot order.
use crate::{persistence, tasks_v2};
use std::collections::BTreeSet;

pub const MAX_ITEMS: usize = 128;
pub const MAX_BYTES: usize = 65536;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conditions {
    pub section: String,
    pub filter: String,
    pub text: String,
    pub sort: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub id: String,
    pub title: String,
    pub format_version: u32,
    pub properties: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SortKey {
    pub id: String,
    pub title: String,
    pub favorite: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    Filter {
        conditions: Conditions,
        candidates: Vec<Candidate>,
    },
    Sort {
        sort: String,
        keys: Vec<SortKey>,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Response {
    pub ids: Vec<String>,
}

pub(crate) fn input_budget(request: &Request) -> Result<(), &'static str> {
    let size = match request {
        Request::Filter {
            conditions,
            candidates,
        } => {
            if candidates.len() > MAX_ITEMS {
                return Err("query item budget");
            }
            candidates.iter().fold(
                conditions
                    .section
                    .len()
                    .saturating_add(conditions.filter.len())
                    .saturating_add(conditions.text.len())
                    .saturating_add(conditions.sort.len()),
                |total, candidate| {
                    total
                        .saturating_add(candidate.id.len())
                        .saturating_add(candidate.title.len())
                        .saturating_add(candidate.properties.len())
                },
            )
        }
        Request::Sort { sort, keys } => {
            if keys.len() > MAX_ITEMS {
                return Err("query item budget");
            }
            keys.iter().fold(sort.len(), |total, key| {
                total
                    .saturating_add(key.id.len())
                    .saturating_add(key.title.len())
            })
        }
    };
    if size > MAX_BYTES {
        Err("query input budget")
    } else {
        Ok(())
    }
}
fn valid_sort(sort: &str) -> Result<(), &'static str> {
    if matches!(sort, "标题排序" | "收藏优先" | "最近添加") {
        Ok(())
    } else {
        Err("unknown query sort")
    }
}
fn valid_conditions(c: &Conditions) -> Result<(), &'static str> {
    if c.text.len() > 16384
        || c.section.len() > 256
        || c.filter.len() > 256
        || !matches!(
            c.section.as_str(),
            "灵感收件箱" | "小项目" | "实验室" | "已收藏" | "概览"
        )
    {
        return Err("invalid query conditions");
    }
    valid_sort(&c.sort)
}
fn valid_key(key: &SortKey) -> Result<(), &'static str> {
    if !crate::valid_id(&key.id) || key.title.len() > 16384 {
        Err("invalid query key")
    } else {
        Ok(())
    }
}
fn unique_ids<'a>(ids: impl Iterator<Item = &'a str>) -> Result<(), &'static str> {
    let mut seen = BTreeSet::new();
    for id in ids {
        if !seen.insert(id) {
            return Err("duplicate query identity");
        }
    }
    Ok(())
}
pub fn sort_key(candidate: &Candidate) -> Result<SortKey, &'static str> {
    if !crate::valid_id(&candidate.id)
        || candidate.title.len() > 16384
        || candidate.properties.len() > MAX_BYTES
    {
        return Err("invalid query candidate");
    }
    let favorite = match candidate.format_version {
        1 => persistence::decode(&candidate.id, &candidate.title, &candidate.properties)?.favorite,
        2 => tasks_v2::decode(&candidate.id, &candidate.title, &candidate.properties)?.favorite,
        _ => return Err("unsupported query format"),
    };
    Ok(SortKey {
        id: candidate.id.clone(),
        title: candidate.title.clone(),
        favorite,
    })
}
fn v1_matches(c: &Conditions, candidate: &Candidate) -> Result<bool, &'static str> {
    let idea = persistence::decode(&candidate.id, &candidate.title, &candidate.properties)?;
    let response = crate::execute(crate::Request {
        action: crate::Action::Query,
        current: Default::default(),
        proposed: Default::default(),
        text: c.text.clone(),
        flag: false,
        now_ms: 0,
        ideas: vec![idea],
        section: c.section.clone(),
        filter: c.filter.clone(),
        sort: "最近添加".into(),
    })?;
    Ok(response.ids.len() == 1)
}
fn v2_matches(c: &Conditions, candidate: &Candidate) -> Result<bool, &'static str> {
    let p = tasks_v2::decode(&candidate.id, &candidate.title, &candidate.properties)?;
    if p.deleted {
        return Ok(false);
    }
    let section = match c.section.as_str() {
        "灵感收件箱" => p.category == "灵感",
        "小项目" => p.category == "进行中",
        "实验室" => p.category == "实验",
        "已收藏" => p.favorite,
        "概览" => true,
        _ => return Err("unknown section"),
    };
    let projection = tasks_v2::project_validated(&p);
    let filter = match c.filter.as_str() {
        "全部" => true,
        "有待办" => projection.incomplete > 0 || projection.ambiguous > 0,
        "含附件" => !p.assets.is_empty(),
        "仅收藏" => p.favorite,
        "图像" => p
            .assets
            .iter()
            .any(|a| matches!(a.kind.as_str(), "image" | "gif")),
        "音视频" => p
            .assets
            .iter()
            .any(|a| matches!(a.kind.as_str(), "audio" | "video")),
        "文件" => p.assets.iter().any(|a| a.kind == "file"),
        "文字" => p.assets.is_empty(),
        stage => p.stage == stage, // V2 never infers stage from legacy checklist text.
    };
    let searchable = format!(
        "{} {} {} {} {}",
        candidate.title,
        p.description,
        p.hypothesis,
        p.conclusion,
        p.assets
            .iter()
            .map(|a| a.name.as_str())
            .collect::<Vec<_>>()
            .join(" "),
    )
    .to_lowercase();
    Ok(section && filter && searchable.contains(&c.text.to_lowercase()))
}
fn sort_keys(sort: &str, keys: &mut [SortKey]) -> Result<(), &'static str> {
    valid_sort(sort)?;
    match sort {
        "标题排序" => keys.sort_by(|a, b| a.title.encode_utf16().cmp(b.title.encode_utf16())),
        "收藏优先" => keys.sort_by_key(|k| !k.favorite),
        "最近添加" => {}
        _ => unreachable!(),
    }
    Ok(())
}
/// Validate a request before encoding it, without running filtering or sorting.
/// Host page fitting calls this repeatedly, so keep business execution separate.
pub fn validate_request(request: &Request) -> Result<(), &'static str> {
    input_budget(request)?;
    match request {
        Request::Filter {
            conditions,
            candidates,
        } => {
            valid_conditions(conditions)?;
            unique_ids(candidates.iter().map(|candidate| candidate.id.as_str()))?;
            for candidate in candidates {
                sort_key(candidate)?;
            }
        }
        Request::Sort { sort, keys } => {
            valid_sort(sort)?;
            for key in keys {
                valid_key(key)?;
            }
            unique_ids(keys.iter().map(|key| key.id.as_str()))?;
        }
    }
    Ok(())
}
pub fn execute(request: Request) -> Result<Response, &'static str> {
    input_budget(&request)?;
    match request {
        Request::Filter {
            conditions,
            candidates,
        } => {
            valid_conditions(&conditions)?;
            if candidates.len() > MAX_ITEMS {
                return Err("query item budget");
            }
            unique_ids(candidates.iter().map(|c| c.id.as_str()))?;
            let mut selected = Vec::new();
            for candidate in &candidates {
                let key = sort_key(candidate)?;
                let matches = match candidate.format_version {
                    1 => v1_matches(&conditions, candidate)?,
                    2 => v2_matches(&conditions, candidate)?,
                    _ => return Err("unsupported query format"),
                };
                if matches {
                    selected.push(key);
                }
            }
            sort_keys(&conditions.sort, &mut selected)?;
            Ok(Response {
                ids: selected.into_iter().map(|k| k.id).collect(),
            })
        }
        Request::Sort { sort, mut keys } => {
            if keys.len() > MAX_ITEMS {
                return Err("query item budget");
            }
            for key in &keys {
                valid_key(key)?;
            }
            unique_ids(keys.iter().map(|k| k.id.as_str()))?;
            sort_keys(&sort, &mut keys)?;
            Ok(Response {
                ids: keys.into_iter().map(|k| k.id).collect(),
            })
        }
    }
}
