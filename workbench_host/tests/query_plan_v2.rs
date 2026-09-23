use morrow_workbench_host::{
    Result,
    query_plan_v2::{self, Backend, Conditions, Observation, Phase},
};
use morrow_workbench_plugin::{
    Idea, persistence,
    query_v2::{self, Candidate, Request, Response},
    query_v2_codec,
    tasks_v2::{self, Baseline},
};

fn conditions(sort: &str) -> Conditions {
    Conditions {
        section: "概览".into(),
        filter: "全部".into(),
        text: String::new(),
        sort: sort.into(),
    }
}
fn candidate(idea: Idea, v2: bool) -> Candidate {
    let body = persistence::encode(&idea, None).unwrap();
    Candidate {
        id: idea.id.clone(),
        title: idea.title.clone(),
        format_version: if v2 { 2 } else { 1 },
        properties: if v2 {
            let base = Baseline::capture(&idea.id, 1, &body).unwrap();
            tasks_v2::migrate(&base, &idea.id, 1, &idea.title, &body).unwrap()
        } else {
            body
        },
    }
}
fn source(count: usize) -> Vec<Candidate> {
    let titles = ["same", "\u{10000}", "\u{e000}", "😀", "中文", "same"];
    (0..count)
        .map(|i| {
            let idea = Idea {
                id: format!("id-{i:05}"),
                title: titles[i % titles.len()].into(),
                description: if i % 29 == 0 {
                    "needle".into()
                } else {
                    String::new()
                },
                category: if i % 7 == 0 {
                    "进行中".into()
                } else {
                    "灵感".into()
                },
                stage: if i % 7 == 0 {
                    "计划中".into()
                } else {
                    "待整理".into()
                },
                favorite: i % 3 == 0,
                todos: if i % 11 == 0 {
                    vec!["same".into(), "same".into()]
                } else {
                    vec![]
                },
                completed: if i % 11 == 0 {
                    vec!["same".into()]
                } else {
                    vec![]
                },
                ..Default::default()
            };
            candidate(idea, i % 2 == 0)
        })
        .collect()
}
fn reference(conditions: &Conditions, source: &[Candidate]) -> Vec<String> {
    let mut filter = conditions.clone();
    filter.sort = "最近添加".into();
    let mut selected: Vec<_> = source
        .iter()
        .filter(|item| {
            !query_v2::execute(Request::Filter {
                conditions: filter.clone(),
                candidates: vec![(*item).clone()],
            })
            .unwrap()
            .ids
            .is_empty()
        })
        .map(|item| query_v2::sort_key(item).unwrap())
        .collect();
    selected.reverse();
    match conditions.sort.as_str() {
        "标题排序" => {
            selected.sort_by(|a, b| a.title.encode_utf16().cmp(b.title.encode_utf16()))
        }
        "收藏优先" => selected.sort_by_key(|key| !key.favorite),
        "最近添加" => {}
        _ => panic!("invalid reference sort"),
    }
    selected.into_iter().map(|item| item.id).collect()
}
#[derive(Clone, Copy, Debug)]
enum Damage {
    Foreign,
    Duplicate,
    Missing,
    Reverse,
    LeftOrder,
    UnemittedSuffix,
}
struct Business {
    candidates: std::vec::IntoIter<Candidate>,
    trace: Vec<Observation>,
    fault: Option<(Phase, Damage)>,
    changed: bool,
    reads: usize,
    calls: usize,
}
impl Business {
    fn new(source: Vec<Candidate>) -> Self {
        Self {
            candidates: source.into_iter(),
            trace: vec![],
            fault: None,
            changed: false,
            reads: 0,
            calls: 0,
        }
    }
}
impl Backend for Business {
    fn next_candidate(&mut self) -> Result<Option<Candidate>> {
        self.reads += 1;
        Ok(self.candidates.next())
    }
    fn invoke(&mut self, phase: Phase, request: Request) -> Result<Response> {
        self.calls += 1;
        let raw = query_v2_codec::encode_request(&request)?;
        let mut response = query_v2::execute(request)?;
        if let Some((target, damage)) = self.fault
            && target == phase
            && !self.changed
        {
            self.changed = true;
            match damage {
                Damage::Foreign => response.ids.push("foreign-id".into()),
                Damage::Duplicate => response.ids.push(response.ids[0].clone()),
                Damage::Missing => {
                    response.ids.pop();
                }
                Damage::Reverse => response.ids.reverse(),
                Damage::LeftOrder => response.ids.swap(0, 1),
                Damage::UnemittedSuffix => {
                    let n = response.ids.len();
                    response.ids.swap(n - 2, n - 1);
                }
            }
        }
        if let Ok(encoded) = query_v2_codec::encode_response(&response) {
            self.trace.push(Observation {
                phase,
                request: raw,
                response: encoded,
            });
        }
        Ok(response)
    }
}
fn copy_trace(trace: &[Observation]) -> Vec<Observation> {
    trace
        .iter()
        .map(|item| Observation {
            phase: item.phase,
            request: item.request.clone(),
            response: item.response.clone(),
        })
        .collect()
}

#[test]
fn mixed_v1_v2_preserve_recency_utf16_and_favorite_stability_across_runs() {
    let source = source(360);
    assert!(
        "\u{10000}"
            .encode_utf16()
            .cmp("\u{e000}".encode_utf16())
            .is_lt()
    );
    for sort in ["最近添加", "标题排序", "收藏优先"] {
        let c = conditions(sort);
        let mut backend = Business::new(source.clone());
        let actual = query_plan_v2::execute(&c, &mut backend).unwrap();
        assert_eq!(actual, reference(&c, &source), "{sort}");
        assert_eq!(backend.reads, source.len() + 1);
        assert_eq!(
            query_plan_v2::replay(2, &c, source.clone(), &backend.trace).unwrap(),
            actual
        );
        assert!(backend.trace.iter().all(|item| {
            match query_v2_codec::decode_request(&item.request).unwrap() {
                Request::Filter {
                    conditions,
                    candidates,
                } => conditions.sort == "最近添加" && candidates.len() <= 128,
                Request::Sort { keys, .. } => keys.len() <= 128,
            }
        }));
        if sort != "最近添加" {
            let runs: Vec<Vec<String>> = backend
                .trace
                .iter()
                .filter(|item| item.phase == Phase::SortRun)
                .map(|item| query_v2_codec::decode_response(&item.response).unwrap().ids)
                .collect();
            let first_merge = backend
                .trace
                .iter()
                .find(|item| item.phase == Phase::Merge)
                .unwrap();
            let Request::Sort { keys, .. } =
                query_v2_codec::decode_request(&first_merge.request).unwrap()
            else {
                panic!("merge must be a sort request");
            };
            let actual_ids: Vec<&str> = keys.iter().map(|key| key.id.as_str()).collect();
            assert_eq!(actual_ids.len(), 128);
            assert_eq!(
                actual_ids[..64],
                runs[0][..64].iter().map(String::as_str).collect::<Vec<_>>()
            );
            assert_eq!(
                actual_ids[64..],
                runs[1][..64].iter().map(String::as_str).collect::<Vec<_>>()
            );
        }
    }
}
#[test]
fn v2_stage_and_ambiguous_task_filter_do_not_reinfer_legacy_completion() {
    let old = Idea {
        id: "id-0".into(),
        title: "same".into(),
        category: "进行中".into(),
        stage: "计划中".into(),
        todos: vec!["same".into(), "same".into()],
        completed: vec!["same".into()],
        ..Default::default()
    };
    let v1 = candidate(old.clone(), false);
    let mut next = old;
    next.id = "id-1".into();
    let v2 = candidate(next, true);
    let mut c = conditions("最近添加");
    c.filter = "有待办".into();
    let source = vec![v1.clone(), v2.clone()];
    let mut backend = Business::new(source.clone());
    assert_eq!(query_plan_v2::execute(&c, &mut backend).unwrap(), ["id-1"]);
    c.section = "小项目".into();
    c.filter = "已完成".into();
    let mut backend = Business::new(source);
    assert_eq!(query_plan_v2::execute(&c, &mut backend).unwrap(), ["id-0"]);
}
#[test]
fn empty_source_invokes_and_bad_conditions_fail_before_reading() {
    for sort in ["最近添加", "标题排序", "收藏优先"] {
        let mut backend = Business::new(vec![]);
        let c = conditions(sort);
        assert!(query_plan_v2::execute(&c, &mut backend).unwrap().is_empty());
        assert_eq!(backend.calls, 1);
        assert_eq!(backend.trace[0].phase, Phase::Filter);
        assert!(matches!(
            query_v2_codec::decode_request(&backend.trace[0].request).unwrap(),
            Request::Filter { candidates, .. } if candidates.is_empty()
        ));
    }
    for damage in 0..4 {
        let mut c = conditions("最近添加");
        match damage {
            0 => c.section = "unknown".into(),
            1 => c.sort = "unknown".into(),
            2 => c.text = "x".repeat(16385),
            _ => c.filter = "x".repeat(257),
        }
        let mut backend = Business::new(vec![]);
        assert!(query_plan_v2::execute(&c, &mut backend).is_err());
        assert_eq!((backend.reads, backend.calls), (0, 0));
    }
}
#[test]
fn candidates_must_be_ascending_valid_and_fit_the_request() {
    let base = source(3);
    for damage in 0..4 {
        let mut changed = base.clone();
        match damage {
            0 => changed.swap(0, 1),
            1 => changed[1].id = changed[0].id.clone(),
            2 => changed[0].format_version = 3,
            _ => changed[0].properties = vec![0xff],
        }
        assert!(
            query_plan_v2::execute(&conditions("最近添加"), &mut Business::new(changed)).is_err()
        );
    }
    let big = candidate(
        Idea {
            id: "oversize".into(),
            title: "t".repeat(16384),
            description: "d".repeat(50000),
            category: "灵感".into(),
            stage: "待整理".into(),
            ..Default::default()
        },
        false,
    );
    assert!(big.properties.len() <= 65536);
    assert!(query_v2::sort_key(&big).is_ok());
    assert!(
        query_v2_codec::encode_request(&Request::Filter {
            conditions: conditions("最近添加"),
            candidates: vec![big.clone()]
        })
        .is_err()
    );
    let mut backend = Business::new(vec![big]);
    assert!(query_plan_v2::execute(&conditions("最近添加"), &mut backend).is_err());
    assert_eq!(backend.calls, 0);
}
#[test]
fn malicious_responses_are_rejected_in_all_phases_and_merge_suffix() {
    let source = source(300);
    for phase in [Phase::Filter, Phase::SortRun, Phase::Merge] {
        let damages = match phase {
            Phase::Filter => vec![Damage::Foreign, Damage::Duplicate, Damage::Reverse],
            Phase::SortRun => vec![Damage::Foreign, Damage::Duplicate, Damage::Missing],
            Phase::Merge => vec![
                Damage::Foreign,
                Damage::Duplicate,
                Damage::Missing,
                Damage::LeftOrder,
                Damage::UnemittedSuffix,
            ],
        };
        for damage in damages {
            let mut backend = Business::new(source.clone());
            backend.fault = Some((phase, damage));
            assert!(
                query_plan_v2::execute(&conditions("标题排序"), &mut backend).is_err(),
                "{phase:?} {damage:?}"
            );
            assert!(backend.changed);
        }
    }
}
#[test]
fn replay_requires_exact_version_candidates_requests_and_all_observations() {
    let source = source(300);
    let c = conditions("标题排序");
    let mut backend = Business::new(source.clone());
    let expected = query_plan_v2::execute(&c, &mut backend).unwrap();
    assert_eq!(
        query_plan_v2::replay(2, &c, source.clone(), &backend.trace).unwrap(),
        expected
    );
    assert!(query_plan_v2::replay(1, &c, source.clone(), &backend.trace).is_err());
    for damage in 0..5 {
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
            3 => trace[0].phase = Phase::Merge,
            _ => {
                let mut request = query_v2_codec::decode_request(&trace[0].request).unwrap();
                if let Request::Filter { conditions, .. } = &mut request {
                    conditions.text = "other".into();
                }
                trace[0].request = query_v2_codec::encode_request(&request).unwrap();
            }
        }
        assert!(
            query_plan_v2::replay(2, &c, source.clone(), &trace).is_err(),
            "trace damage {damage}"
        );
    }
    for damage in 0..3 {
        let mut changed = source.clone();
        match damage {
            0 => {
                let idea = &changed[1];
                let mut decoded =
                    persistence::decode(&idea.id, &idea.title, &idea.properties).unwrap();
                decoded.description.push('x');
                changed[1].properties =
                    persistence::encode(&decoded, Some(&idea.properties)).unwrap();
            }
            1 => {
                changed.remove(0);
            }
            _ => {
                let extra = candidate(
                    Idea {
                        id: "zzz-last".into(),
                        title: "late".into(),
                        category: "灵感".into(),
                        stage: "待整理".into(),
                        ..Default::default()
                    },
                    false,
                );
                changed.push(extra);
            }
        }
        assert!(
            query_plan_v2::replay(2, &c, changed, &backend.trace).is_err(),
            "candidate damage {damage}"
        );
    }
}

#[test]
fn filter_omission_is_allowed_but_replay_does_not_authenticate_source() {
    let source = source(5);
    let c = conditions("最近添加");
    let mut backend = Business::new(source.clone());
    backend.fault = Some((Phase::Filter, Damage::Missing));
    let found = query_plan_v2::execute(&c, &mut backend).unwrap();
    assert_eq!(found.len(), source.len() - 1);
    assert_eq!(
        query_plan_v2::replay(2, &c, source, &backend.trace).unwrap(),
        found
    );
}
