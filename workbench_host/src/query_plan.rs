//! Versioned host query scheduling, independent of storage and authority.
//! V1 preserves reverse-ID recency and guest-defined stable UTF-16/title or favorite sorting.
//! Every filter/run/merge goes through Backend::invoke. No observations may be omitted when
//! adapting this plan to durable evidence. Candidate completeness and authorization belong to
//! the source adapter; a successful replay here proves scheduling, not an authentic read set.
use crate::{Result, command};
use morrow_workbench_plugin::{Action, Asset, Idea, Request, Response, codec};
use std::collections::{BTreeMap, BTreeSet};

pub const VERSION: u32 = 1;
const MAX_RUN: usize = 128;
const MAX_PREFIX: usize = 64;

#[derive(Clone, Debug)]
pub struct Conditions {
    pub section: String,
    pub filter: String,
    pub text: String,
    pub sort: String,
}
impl Conditions {
    fn validate(&self) -> Result<()> {
        if !matches!(
            self.section.as_str(),
            "概览" | "灵感收件箱" | "小项目" | "实验室" | "已收藏"
        ) {
            return Err("unknown query section".into());
        }
        if !matches!(self.sort.as_str(), "最近添加" | "标题排序" | "收藏优先") {
            return Err("unknown query sort".into());
        }
        if self.text.len() > 16384 {
            return Err("query text budget".into());
        }
        // Filter remains open to stage names, as in the original guest contract.
        let mut request = command(Action::Query);
        request.section = self.section.clone();
        request.filter = self.filter.clone();
        request.text = self.text.clone();
        codec::encode_request(&request)?;
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Filter,
    SortRun,
    Merge,
}
pub trait Backend {
    /// Ascending, unique IDs; None is a final end marker, including after skipped content types.
    fn next_candidate(&mut self) -> Result<Option<Idea>>;
    fn invoke(&mut self, phase: Phase, request: Request) -> Result<Response>;
}

fn invoke(backend: &mut impl Backend, phase: Phase, request: Request) -> Result<Vec<String>> {
    codec::encode_request(&request)?;
    let input: Vec<_> = request.ideas.iter().map(|idea| idea.id.clone()).collect();
    let response = backend.invoke(phase, request)?;
    if response.idea != Idea::default() {
        return Err("query returned content instead of an ID projection".into());
    }
    let expected: BTreeMap<_, _> = input
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();
    if expected.len() != input.len() {
        return Err("duplicate query input".into());
    }
    let mut seen = BTreeSet::new();
    let mut previous = None;
    for id in &response.ids {
        let position = *expected
            .get(id.as_str())
            .ok_or("query returned an unknown ID")?;
        if !seen.insert(id) {
            return Err("query returned a duplicate ID".into());
        }
        if phase == Phase::Filter && previous.is_some_and(|old| position <= old) {
            return Err("query filter reordered candidates".into());
        }
        previous = Some(position);
    }
    if phase != Phase::Filter && response.ids.len() != input.len() {
        return Err("query sort omitted an ID".into());
    }
    Ok(response.ids)
}
fn fits(request: &Request) -> bool {
    request.ideas.len() <= MAX_RUN && codec::encode_request(request).is_ok()
}
fn compact(mut idea: Idea) -> Idea {
    idea.description.clear();
    idea.hypothesis.clear();
    idea.conclusion.clear();
    idea.assets.clear();
    idea.todos.clear();
    idea.completed.clear();
    if idea.title.trim().is_empty() {
        idea.assets.push(Asset {
            id: "sort-key".into(),
            kind: "file".into(),
            ..Default::default()
        });
    }
    idea
}
fn merge(
    backend: &mut impl Backend,
    sort: &str,
    keys: &BTreeMap<String, Idea>,
    left: Vec<String>,
    right: Vec<String>,
) -> Result<Vec<String>> {
    let (mut i, mut j) = (0, 0);
    let mut merged = Vec::with_capacity(left.len() + right.len());
    while i < left.len() && j < right.len() {
        let mut request = command(Action::Query);
        request.sort = sort.into();
        request.ideas = vec![keys[&left[i]].clone(), keys[&right[j]].clone()];
        if !fits(&request) {
            return Err("query merge key budget".into());
        }
        let (mut a, mut b) = (1, 1);
        // Always keep all left keys before right keys for stable cross-run ties.
        while a < MAX_PREFIX && i + a < left.len() {
            request.ideas.insert(a, keys[&left[i + a]].clone());
            if !fits(&request) {
                request.ideas.remove(a);
                break;
            }
            a += 1;
        }
        while b < MAX_PREFIX && j + b < right.len() {
            request.ideas.push(keys[&right[j + b]].clone());
            if !fits(&request) {
                request.ideas.pop();
                break;
            }
            b += 1;
        }
        let ids = invoke(backend, Phase::Merge, request)?;
        // A permutation alone is insufficient: advancing prefix cursors would lose/repeat IDs
        // if a malicious guest changed either side's internal order. Check the ENTIRE response,
        // including the suffix which is deliberately not emitted in this iteration.
        let left_ids: BTreeSet<_> = left[i..i + a].iter().map(String::as_str).collect();
        let (mut used_a, mut used_b) = (0, 0);
        let mut safe = None;
        for (n, id) in ids.iter().enumerate() {
            if left_ids.contains(id.as_str()) {
                if left.get(i + used_a) != Some(id) {
                    return Err("query merge reordered left run".into());
                }
                used_a += 1;
            } else {
                if right.get(j + used_b) != Some(id) {
                    return Err("query merge reordered right run".into());
                }
                used_b += 1;
            }
            if safe.is_none() && (used_a == a || used_b == b) {
                safe = Some((n + 1, used_a, used_b));
            }
        }
        let (count, consumed_a, consumed_b) = safe.ok_or("query merge made no progress")?;
        merged.extend(ids.into_iter().take(count));
        i += consumed_a;
        j += consumed_b;
    }
    merged.extend_from_slice(&left[i..]);
    merged.extend_from_slice(&right[j..]);
    Ok(merged)
}

pub fn execute(conditions: &Conditions, backend: &mut impl Backend) -> Result<Vec<String>> {
    conditions.validate()?;
    let mut request = command(Action::Query);
    request.section = conditions.section.clone();
    request.filter = conditions.filter.clone();
    request.text = conditions.text.clone();
    let mut keys = BTreeMap::new();
    let mut previous: Option<String> = None;
    let mut found = Vec::new();
    let mut filtered = false;
    while let Some(idea) = backend.next_candidate()? {
        idea.validate()?;
        if previous.as_ref().is_some_and(|id| id >= &idea.id) {
            return Err("query candidates are not unique ascending IDs".into());
        }
        previous = Some(idea.id.clone());
        request.ideas.push(idea.clone());
        if !fits(&request) {
            request.ideas.pop();
            if request.ideas.is_empty() {
                return Err("card exceeds query message budget".into());
            }
            found.extend(invoke(backend, Phase::Filter, request.clone())?);
            filtered = true;
            request.ideas = vec![idea.clone()];
            if !fits(&request) {
                return Err("card exceeds query message budget".into());
            }
        }
        keys.insert(idea.id.clone(), compact(idea));
    }
    // Even an empty library performs an actual task, so missing/disabled plugins cannot
    // silently succeed. Invalid section/sort is rejected above before inspecting candidates.
    if !request.ideas.is_empty() || !filtered {
        found.extend(invoke(backend, Phase::Filter, request)?);
    }
    found.reverse();
    if conditions.sort == "最近添加" {
        return Ok(found);
    }
    let mut runs = Vec::new();
    let mut request = command(Action::Query);
    request.sort = conditions.sort.clone();
    for id in found {
        let key = keys.get(&id).ok_or("missing sort key")?.clone();
        request.ideas.push(key.clone());
        if !fits(&request) {
            request.ideas.pop();
            if request.ideas.is_empty() {
                return Err("sort key budget".into());
            }
            runs.push(invoke(backend, Phase::SortRun, request.clone())?);
            request.ideas = vec![key];
            if !fits(&request) {
                return Err("sort key budget".into());
            }
        }
    }
    if !request.ideas.is_empty() {
        runs.push(invoke(backend, Phase::SortRun, request)?);
    }
    while runs.len() > 1 {
        let mut next = Vec::new();
        let mut input = runs.into_iter();
        while let Some(left) = input.next() {
            next.push(match input.next() {
                Some(right) => merge(backend, &conditions.sort, &keys, left, right)?,
                None => left,
            });
        }
        runs = next;
    }
    Ok(runs.pop().unwrap_or_default())
}

/// Exact encoded task payloads captured by a caller. Not a signature or proof of execution.
pub struct Observation {
    pub phase: Phase,
    pub request: Vec<u8>,
    pub response: Vec<u8>,
}
/// Reconstruct scheduling with original candidates and all original task responses. A durable
/// adapter must also verify each task's actual Evidence and the read source independently.
pub fn replay(
    version: u32,
    conditions: &Conditions,
    candidates: impl IntoIterator<Item = Idea>,
    observations: &[Observation],
) -> Result<Vec<String>> {
    if version != VERSION {
        return Err("unsupported query plan version".into());
    }
    struct Recorded<'a, I> {
        candidates: I,
        observations: &'a [Observation],
        offset: usize,
    }
    impl<I: Iterator<Item = Idea>> Backend for Recorded<'_, I> {
        fn next_candidate(&mut self) -> Result<Option<Idea>> {
            Ok(self.candidates.next())
        }
        fn invoke(&mut self, phase: Phase, request: Request) -> Result<Response> {
            let original = self
                .observations
                .get(self.offset)
                .ok_or("missing query observation")?;
            if original.phase != phase || original.request != codec::encode_request(&request)? {
                return Err("query observation input mismatch".into());
            }
            self.offset += 1;
            Ok(codec::decode_response(&original.response)?)
        }
    }
    let mut backend = Recorded {
        candidates: candidates.into_iter(),
        observations,
        offset: 0,
    };
    let result = execute(conditions, &mut backend)?;
    if backend.offset != observations.len() {
        return Err("extra query observations".into());
    }
    Ok(result)
}
