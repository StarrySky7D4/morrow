//! Format-2 query scheduling. V1 remains frozen in query_plan.
//! This proves ordered task correspondence, not source completeness, permission,
//! or that a recorded response came from a real guest execution.
use crate::Result;
use morrow_workbench_plugin::{
    query_v2::{self, Candidate, Request, Response, SortKey},
    query_v2_codec,
};
use std::collections::{BTreeMap, BTreeSet};

pub use morrow_workbench_plugin::query_v2::Conditions;
pub const VERSION: u32 = 2;
const MAX_RUN: usize = 128;
const MAX_PREFIX: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Filter,
    SortRun,
    Merge,
}
pub trait Backend {
    /// Ascending unique IDs. None is final, including after skipped content types.
    fn next_candidate(&mut self) -> Result<Option<Candidate>>;
    fn invoke(&mut self, phase: Phase, request: Request) -> Result<Response>;
}
fn input_ids(request: &Request) -> Vec<String> {
    match request {
        Request::Filter { candidates, .. } => {
            candidates.iter().map(|item| item.id.clone()).collect()
        }
        Request::Sort { keys, .. } => keys.iter().map(|item| item.id.clone()).collect(),
    }
}
fn invoke(backend: &mut impl Backend, phase: Phase, request: Request) -> Result<Vec<String>> {
    match (&request, phase) {
        (Request::Filter { .. }, Phase::Filter)
        | (Request::Sort { .. }, Phase::SortRun | Phase::Merge) => {}
        _ => return Err("query phase or request mismatch".into()),
    }
    query_v2_codec::encode_request(&request)?;
    let input = input_ids(&request);
    let positions: BTreeMap<_, _> = input
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();
    if positions.len() != input.len() {
        return Err("duplicate query input".into());
    }
    let response = backend.invoke(phase, request)?;
    let mut seen = BTreeSet::new();
    let mut previous = None;
    for id in &response.ids {
        let position = *positions
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
fn count(request: &Request) -> usize {
    match request {
        Request::Filter { candidates, .. } => candidates.len(),
        Request::Sort { keys, .. } => keys.len(),
    }
}
fn fits(request: &Request) -> bool {
    count(request) <= MAX_RUN && query_v2_codec::encode_request(request).is_ok()
}
fn merge(
    backend: &mut impl Backend,
    sort: &str,
    keys: &BTreeMap<String, SortKey>,
    left: Vec<String>,
    right: Vec<String>,
) -> Result<Vec<String>> {
    let (mut i, mut j) = (0, 0);
    let mut merged = Vec::with_capacity(left.len() + right.len());
    while i < left.len() && j < right.len() {
        let mut selected = vec![keys[&left[i]].clone(), keys[&right[j]].clone()];
        let mut request = Request::Sort {
            sort: sort.into(),
            keys: selected.clone(),
        };
        if !fits(&request) {
            return Err("query merge key budget".into());
        }
        let (mut a, mut b) = (1, 1);
        // Left before right is the stable tie order across sorted runs.
        while a < MAX_PREFIX && i + a < left.len() {
            selected.insert(a, keys[&left[i + a]].clone());
            request = Request::Sort {
                sort: sort.into(),
                keys: selected.clone(),
            };
            if !fits(&request) {
                selected.remove(a);
                break;
            }
            a += 1;
        }
        while b < MAX_PREFIX && j + b < right.len() {
            selected.push(keys[&right[j + b]].clone());
            request = Request::Sort {
                sort: sort.into(),
                keys: selected.clone(),
            };
            if !fits(&request) {
                selected.pop();
                break;
            }
            b += 1;
        }
        let ids = invoke(
            backend,
            Phase::Merge,
            Request::Sort {
                sort: sort.into(),
                keys: selected,
            },
        )?;
        // Validate the entire permutation, including the unconsumed suffix.
        // Otherwise a response could reorder a run after the emitted prefix.
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
    // Validate every condition before touching the source, even for an empty library.
    query_v2::execute(Request::Filter {
        conditions: conditions.clone(),
        candidates: vec![],
    })?;
    let mut filter_conditions = conditions.clone();
    filter_conditions.sort = "最近添加".into();
    let mut request = Request::Filter {
        conditions: filter_conditions,
        candidates: vec![],
    };
    let mut keys = BTreeMap::new();
    let mut previous: Option<String> = None;
    let mut found = Vec::new();
    let mut filtered = false;
    while let Some(candidate) = backend.next_candidate()? {
        let key = query_v2::sort_key(&candidate)?;
        if previous.as_ref().is_some_and(|id| id >= &candidate.id) {
            return Err("query candidates are not unique ascending IDs".into());
        }
        previous = Some(candidate.id.clone());
        let Request::Filter { candidates, .. } = &mut request else {
            unreachable!()
        };
        candidates.push(candidate.clone());
        if !fits(&request) {
            let Request::Filter { candidates, .. } = &mut request else {
                unreachable!()
            };
            candidates.pop();
            if candidates.is_empty() {
                return Err("card exceeds query request budget".into());
            }
            found.extend(invoke(backend, Phase::Filter, request.clone())?);
            filtered = true;
            request = Request::Filter {
                conditions: match request {
                    Request::Filter { conditions, .. } => conditions,
                    _ => unreachable!(),
                },
                candidates: vec![candidate.clone()],
            };
            if !fits(&request) {
                return Err("card exceeds query request budget".into());
            }
        }
        keys.insert(candidate.id, key);
    }
    if count(&request) != 0 || !filtered {
        found.extend(invoke(backend, Phase::Filter, request)?);
    }
    found.reverse();
    if conditions.sort == "最近添加" {
        return Ok(found);
    }
    let mut runs = Vec::new();
    let mut selected = Vec::new();
    for id in found {
        let key = keys.get(&id).ok_or("missing query sort key")?.clone();
        selected.push(key.clone());
        let request = Request::Sort {
            sort: conditions.sort.clone(),
            keys: selected.clone(),
        };
        if !fits(&request) {
            selected.pop();
            if selected.is_empty() {
                return Err("query sort key budget".into());
            }
            runs.push(invoke(
                backend,
                Phase::SortRun,
                Request::Sort {
                    sort: conditions.sort.clone(),
                    keys: selected.clone(),
                },
            )?);
            selected = vec![key];
            if !fits(&Request::Sort {
                sort: conditions.sort.clone(),
                keys: selected.clone(),
            }) {
                return Err("query sort key budget".into());
            }
        }
    }
    if !selected.is_empty() {
        runs.push(invoke(
            backend,
            Phase::SortRun,
            Request::Sort {
                sort: conditions.sort.clone(),
                keys: selected,
            },
        )?);
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

/// Original encoded task observations. The caller must authenticate execution and source.
pub struct Observation {
    pub phase: Phase,
    pub request: Vec<u8>,
    pub response: Vec<u8>,
}
pub fn replay(
    version: u32,
    conditions: &Conditions,
    candidates: impl IntoIterator<Item = Candidate>,
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
    impl<I: Iterator<Item = Candidate>> Backend for Recorded<'_, I> {
        fn next_candidate(&mut self) -> Result<Option<Candidate>> {
            Ok(self.candidates.next())
        }
        fn invoke(&mut self, phase: Phase, request: Request) -> Result<Response> {
            let original = self
                .observations
                .get(self.offset)
                .ok_or("missing query observation")?;
            if original.phase != phase
                || original.request != query_v2_codec::encode_request(&request)?
            {
                return Err("query observation input mismatch".into());
            }
            self.offset += 1;
            Ok(query_v2_codec::decode_response(&original.response)?)
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
