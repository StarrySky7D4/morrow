use morrow_workbench_host::{
    Result,
    query_plan::{self, Backend, Conditions, Observation, Phase},
};
use morrow_workbench_plugin::{Action, Asset, Idea, Request, Response, codec, execute};

fn conditions(sort: &str) -> Conditions {
    Conditions {
        section: "概览".into(),
        filter: "全部".into(),
        text: String::new(),
        sort: sort.into(),
    }
}
fn request(c: &Conditions, ideas: Vec<Idea>) -> Request {
    Request {
        action: Action::Query,
        current: Idea::default(),
        proposed: Idea::default(),
        text: c.text.clone(),
        flag: false,
        now_ms: 0,
        ideas,
        section: c.section.clone(),
        filter: c.filter.clone(),
        sort: c.sort.clone(),
    }
}
fn ideas(count: usize) -> Vec<Idea> {
    (0..count)
        .map(|i| Idea {
            id: format!("id-{i:05}"),
            title: format!("title-{:03}", (i * 37) % 113),
            category: "灵感".into(),
            stage: "待整理".into(),
            favorite: i % 3 == 0,
            ..Default::default()
        })
        .collect()
}
// This backend runs the actual business implementation and records real request/response bytes.
// It establishes scheduler parity, not a Wasm runtime or authenticated source-read proof.
struct Business {
    candidates: std::vec::IntoIter<Idea>,
    trace: Vec<Observation>,
    fault: Option<(Phase, Damage)>,
    changed: bool,
    reads: usize,
    failed: bool,
}
#[derive(Clone, Copy, Debug)]
enum Damage {
    Foreign,
    Duplicate,
    Missing,
    Content,
    Reverse,
    LeftOrder,
    UnemittedSuffix,
}
impl Business {
    fn new(candidates: Vec<Idea>) -> Self {
        Self {
            candidates: candidates.into_iter(),
            trace: vec![],
            fault: None,
            changed: false,
            reads: 0,
            failed: false,
        }
    }
}
impl Backend for Business {
    fn next_candidate(&mut self) -> Result<Option<Idea>> {
        self.reads += 1;
        Ok(self.candidates.next())
    }
    fn invoke(&mut self, phase: Phase, input: Request) -> Result<Response> {
        if self.failed {
            return Err("synthetic inactive backend".into());
        }
        let raw = codec::encode_request(&input)?;
        let mut output = execute(input)?;
        if let Some((target, damage)) = self.fault
            && !self.changed
            && target == phase
        {
            self.changed = true;
            match damage {
                Damage::Foreign => output.ids.push("foreign-id".into()),
                Damage::Duplicate => output.ids.push(output.ids[0].clone()),
                Damage::Missing => {
                    output.ids.pop();
                }
                Damage::Content => output.idea = ideas(1).remove(0),
                Damage::Reverse => output.ids.reverse(),
                Damage::LeftOrder => output.ids.swap(0, 1),
                Damage::UnemittedSuffix => {
                    let n = output.ids.len();
                    output.ids.swap(n - 2, n - 1);
                }
            }
        }
        self.trace.push(Observation {
            phase,
            request: raw,
            response: codec::encode_response(&output)?,
        });
        Ok(output)
    }
}
fn copy_trace(trace: &[Observation]) -> Vec<Observation> {
    trace
        .iter()
        .map(|o| Observation {
            phase: o.phase,
            request: o.request.clone(),
            response: o.response.clone(),
        })
        .collect()
}
fn reference(c: &Conditions, source: &[Idea]) -> Vec<String> {
    // Independent baseline: real one-record filtering, then a single global stable sort.
    // This intentionally does not reproduce the scheduler's pages, runs, or merge algorithm.
    let mut filter = c.clone();
    filter.sort = "最近添加".into();
    let mut selected: Vec<&Idea> = source
        .iter()
        .filter(|idea| {
            !execute(request(&filter, vec![(*idea).clone()]))
                .unwrap()
                .ids
                .is_empty()
        })
        .collect();
    selected.reverse();
    match c.sort.as_str() {
        "标题排序" => {
            selected.sort_by(|a, b| a.title.encode_utf16().cmp(b.title.encode_utf16()))
        }
        "收藏优先" => selected.sort_by_key(|idea| !idea.favorite),
        "最近添加" => {}
        _ => panic!("invalid reference sort"),
    }
    selected.into_iter().map(|idea| idea.id.clone()).collect()
}
#[test]
fn four_thousand_ninety_six_results_preserve_reverse_id_and_stable_utf16_order() {
    let mut source = ideas(4096);
    let titles = ["same", "\u{10000}", "\u{e000}", "😀", "中文", "same"];
    for (n, idea) in source.iter_mut().enumerate() {
        idea.title = titles[n % titles.len()].into();
    }
    // U+10000 sorts before U+E000 under UTF-16, unlike scalar or UTF-8 ordering.
    assert!(
        "\u{10000}"
            .encode_utf16()
            .cmp("\u{e000}".encode_utf16())
            .is_lt()
    );
    for sort in ["最近添加", "标题排序", "收藏优先"] {
        let c = conditions(sort);
        let mut backend = Business::new(source.clone());
        let found = query_plan::execute(&c, &mut backend).unwrap();
        assert_eq!(found.len(), 4096);
        assert_eq!(found, reference(&c, &source), "{sort}");
        assert_eq!(
            query_plan::replay(1, &c, source.clone(), &backend.trace).unwrap(),
            found
        );
        if sort != "最近添加" {
            assert!(backend.trace.iter().any(|o| o.phase == Phase::Merge));
        }
    }
}
#[test]
fn more_than_result_limit_candidates_are_scanned_and_search_uses_all_business_fields() {
    let mut source = ideas(5000);
    source[4900].description = "Needle".into();
    source[4999].hypothesis = "NEEDLE".into();
    source[2000].conclusion = "needle".into();
    source[3000].assets = vec![Asset {
        id: "asset".into(),
        name: "needle.xlsx".into(),
        kind: "file".into(),
        bytes: 1,
    }];
    source[4980].description = "needle".into();
    source[4980].deleted = true;
    let mut c = conditions("标题排序");
    c.text = "nEeDlE".into();
    let mut backend = Business::new(source.clone());
    let result = query_plan::execute(&c, &mut backend).unwrap();
    assert_eq!(result.len(), 4);
    assert_eq!(result, reference(&c, &source));
    assert_eq!(backend.reads, 5001);
    assert_eq!(
        query_plan::replay(1, &c, source, &backend.trace).unwrap(),
        result
    );
}
#[test]
fn maximum_legal_titles_force_small_merge_windows_without_losing_equal_key_stability() {
    let mut source = ideas(192);
    for (i, idea) in source.iter_mut().enumerate() {
        idea.title = format!("{:02}{}", i % 7, "x".repeat(16382));
    }
    let c = conditions("标题排序");
    let mut backend = Business::new(source.clone());
    let result = query_plan::execute(&c, &mut backend).unwrap();
    assert_eq!(result, reference(&c, &source));
    let merges: Vec<_> = backend
        .trace
        .iter()
        .filter(|o| o.phase == Phase::Merge)
        .collect();
    assert!(
        merges.len() > 100,
        "long keys must exercise repeated small windows"
    );
    assert!(
        merges
            .iter()
            .all(|o| codec::decode_request(&o.request).unwrap().ideas.len() <= 3)
    );
    assert_eq!(
        query_plan::replay(1, &c, source, &backend.trace).unwrap(),
        result
    );
}
#[test]
fn attachment_only_titles_and_stage_filters_keep_business_semantics() {
    let mut source = ideas(300);
    for (i, idea) in source.iter_mut().enumerate() {
        if i % 5 == 0 {
            idea.title.clear();
            idea.assets = vec![Asset {
                id: "attachment".into(),
                kind: "file".into(),
                ..Default::default()
            }];
        }
        if i % 3 == 0 {
            idea.category = "进行中".into();
            idea.stage = "计划中".into();
            idea.todos = vec!["task".into()];
            idea.completed = vec!["task".into()];
        }
    }
    for (section, filter) in [
        ("概览", "含附件"),
        ("小项目", "已完成"),
        ("概览", "文字"),
        ("已收藏", "全部"),
    ] {
        let mut c = conditions("标题排序");
        c.section = section.into();
        c.filter = filter.into();
        let mut backend = Business::new(source.clone());
        assert_eq!(
            query_plan::execute(&c, &mut backend).unwrap(),
            reference(&c, &source)
        );
    }
}
#[test]
fn empty_source_still_invokes_and_invalid_conditions_fail_before_source_access() {
    for sort in ["最近添加", "标题排序", "收藏优先"] {
        let mut backend = Business::new(vec![]);
        let c = conditions(sort);
        assert!(query_plan::execute(&c, &mut backend).unwrap().is_empty());
        assert_eq!(backend.trace.len(), 1);
        assert_eq!(backend.trace[0].phase, Phase::Filter);
        assert!(
            codec::decode_request(&backend.trace[0].request)
                .unwrap()
                .ideas
                .is_empty()
        );
        let mut denied = Business::new(vec![]);
        denied.failed = true;
        assert!(query_plan::execute(&c, &mut denied).is_err());
    }
    for field in 0..4 {
        let mut c = conditions("最近添加");
        match field {
            0 => c.section = "unknown".into(),
            1 => c.sort = "unknown".into(),
            2 => c.text = "x".repeat(16385),
            _ => c.filter = "x".repeat(65536),
        }
        let mut backend = Business::new(vec![]);
        assert!(query_plan::execute(&c, &mut backend).is_err());
        assert_eq!(backend.reads, 0);
        assert!(backend.trace.is_empty());
    }
}
#[test]
fn duplicate_reversed_or_invalid_candidates_are_rejected() {
    for damage in 0..4 {
        let mut source = ideas(3);
        match damage {
            0 => source.swap(0, 1),
            1 => source[1].id = source[0].id.clone(),
            2 => source[0].title.clear(),
            _ => source[0].description = "x".repeat(65536),
        }
        let mut backend = Business::new(source);
        assert!(
            query_plan::execute(&conditions("最近添加"), &mut backend).is_err(),
            "damage {damage}"
        );
    }
}
#[test]
fn malicious_projection_and_sort_responses_are_rejected_in_every_phase() {
    let mut source = ideas(300);
    for idea in &mut source {
        idea.title = "same".into();
    }
    for phase in [Phase::Filter, Phase::SortRun, Phase::Merge] {
        let mut damages = vec![Damage::Foreign, Damage::Duplicate, Damage::Content];
        if phase == Phase::Filter {
            damages.push(Damage::Reverse);
        } else {
            damages.push(Damage::Missing);
        }
        if phase == Phase::Merge {
            damages.extend([Damage::LeftOrder, Damage::UnemittedSuffix]);
        }
        for damage in damages {
            let mut backend = Business::new(source.clone());
            backend.fault = Some((phase, damage));
            assert!(
                query_plan::execute(&conditions("标题排序"), &mut backend).is_err(),
                "{phase:?} {damage:?}"
            );
            assert!(backend.changed);
        }
    }
}
#[test]
fn filtering_may_omit_candidates_but_replay_does_not_claim_response_authenticity() {
    let source = ideas(5);
    let c = conditions("最近添加");
    let mut backend = Business::new(source.clone());
    backend.fault = Some((Phase::Filter, Damage::Missing));
    let result = query_plan::execute(&c, &mut backend).unwrap();
    assert_eq!(result.len(), 4);
    assert_eq!(
        query_plan::replay(1, &c, source, &backend.trace).unwrap(),
        result
    );
    // Actual task evidence must authenticate the original execution separately. A scheduler
    // accepts an ordered filtered subset; it cannot prove that the guest omitted no match.
}
#[test]
fn replay_requires_entire_ordered_trace_exact_requests_candidates_and_known_version() {
    let source = ideas(300);
    let c = conditions("标题排序");
    let mut backend = Business::new(source.clone());
    let original = query_plan::execute(&c, &mut backend).unwrap();
    assert_eq!(
        query_plan::replay(1, &c, source.clone(), &backend.trace).unwrap(),
        original
    );
    assert!(query_plan::replay(2, &c, source.clone(), &backend.trace).is_err());
    for damage in 0..6 {
        let mut trace = copy_trace(&backend.trace);
        match damage {
            0 => {
                trace.remove(1);
            }
            1 => {
                let extra = copy_trace(&trace[..1]).remove(0);
                trace.push(extra);
            }
            2 => trace.swap(0, 1),
            3 => {
                let mut request = codec::decode_request(&trace[0].request).unwrap();
                request.text = "other".into();
                trace[0].request = codec::encode_request(&request).unwrap();
            }
            4 => trace[0].phase = Phase::Merge,
            _ => {
                let mut response = codec::decode_response(&trace[0].response).unwrap();
                response.ids.push("foreign".into());
                trace[0].response = codec::encode_response(&response).unwrap();
            }
        }
        assert!(
            query_plan::replay(1, &c, source.clone(), &trace).is_err(),
            "trace damage {damage}"
        );
    }
    for damage in 0..3 {
        let mut changed = source.clone();
        match damage {
            0 => changed[0].description.push('x'),
            1 => {
                changed.remove(0);
            }
            _ => {
                let mut extra = ideas(1).remove(0);
                extra.id = "zzz-last".into();
                changed.push(extra);
            }
        }
        assert!(
            query_plan::replay(1, &c, changed, &backend.trace).is_err(),
            "candidate damage {damage}"
        );
    }
}
