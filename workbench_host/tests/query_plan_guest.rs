#![cfg(target_os = "windows")]
mod common;
use morrow_core::{
    dispatch::HostRuntime,
    lifecycle::GrantKind,
    plugin_package::{Package, catalog::Catalog, proto::Capability, registry::Registry},
    store::{EventBudget, Store},
    task::{Invocation, Transform},
    task_evidence::Evidence,
};
use morrow_plugin_runtime::{
    Limits,
    instance_pool::{Pool, Session},
    manager::Manager,
    replay,
};
use morrow_workbench_host::{
    Result, Workbench,
    query_plan::{self, Backend, Conditions, Observation, Phase},
};
use morrow_workbench_plugin::{Idea, Request, Response, codec};
use std::collections::BTreeSet;

struct Actual<'a> {
    host: &'a mut HostRuntime,
    manager: &'a Manager,
    pool: &'a mut Pool,
    session: &'a Session,
    candidates: std::vec::IntoIter<Idea>,
    observations: Vec<Observation>,
    evidence: Vec<Evidence>,
}
impl Backend for Actual<'_> {
    fn next_candidate(&mut self) -> Result<Option<Idea>> {
        Ok(self.candidates.next())
    }
    fn invoke(&mut self, phase: Phase, request: Request) -> Result<Response> {
        let bytes = codec::encode_request(&request)?;
        let invocation = Invocation::new_transform(
            &format!("query-step-{}", self.observations.len()),
            Transform {
                handler: "workbench.command".into(),
                input_type: "morrow.workbench.request.v1".into(),
                output_type: "morrow.workbench.response.v1".into(),
                input: bytes.clone(),
            },
        )?;
        let (report, evidence) = self
            .pool
            .record_transform(self.manager, self.host, self.session, &invocation)?
            .into_parts();
        assert_eq!(report.execution.outcome, Ok(0));
        assert_eq!(report.execution.host_calls, 0);
        assert!(report.failure.is_none() && report.response.is_none());
        let output = report.output.ok_or("missing result")?;
        assert_eq!(output.type_id, "morrow.workbench.response.v1");
        let decoded = codec::decode_response(&output.bytes)?;
        self.observations.push(Observation {
            phase,
            request: bytes,
            response: output.bytes,
        });
        self.evidence.push(evidence);
        Ok(decoded)
    }
}
#[test]
fn actual_sdk_filter_sort_merge_and_every_recorded_task_replay_without_source() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("query-source");
    std::fs::create_dir(&source).unwrap();
    let store = Store::open(&source.join("db"), EventBudget::default()).unwrap();
    let mut host = HostRuntime::new(store).unwrap();
    let package = common::package();
    let catalog = Catalog::open(&source.join("packages")).unwrap();
    catalog.install(&package).unwrap();
    let mut manager = Manager::new(
        Registry::open(&source.join("registry"), catalog).unwrap(),
        Limits::default(),
    );
    let id = &package.manifest().package_id;
    manager.select(&package, manager.revision()).unwrap();
    manager
        .approve(
            id,
            package.digest(),
            BTreeSet::from([GrantKind::ReadContent]),
            manager.revision(),
        )
        .unwrap();
    manager
        .set_enabled(id, package.digest(), true, manager.revision())
        .unwrap();
    let mut pool = Pool::new(&host, Default::default()).unwrap();
    let revision = manager.revision();
    let session = pool
        .start(&mut manager, &mut host, id, &[], revision)
        .unwrap();
    // Explicit synthetic candidates qualify the scheduler and actual SDK execution. They are
    // not claimed as a complete/authorized database read or a durable query journal.
    let candidates: Vec<Idea> = (0..160)
        .map(|i| {
            let mut idea = common::idea(&format!("card-{i:04}"));
            idea.title = format!(
                "{}-{:02}",
                if i % 3 == 0 { "\u{10000}" } else { "\u{e000}" },
                i % 17
            );
            idea
        })
        .collect();
    let conditions = Conditions {
        section: "概览".into(),
        filter: "全部".into(),
        text: String::new(),
        sort: "标题排序".into(),
    };
    let mut actual = Actual {
        host: &mut host,
        manager: &manager,
        pool: &mut pool,
        session: &session,
        candidates: candidates.clone().into_iter(),
        observations: vec![],
        evidence: vec![],
    };
    let output = query_plan::execute(&conditions, &mut actual).unwrap();
    let observations = std::mem::take(&mut actual.observations);
    let evidence = std::mem::take(&mut actual.evidence);
    drop(actual);
    assert!(observations.iter().any(|v| v.phase == Phase::Filter));
    assert!(observations.iter().any(|v| v.phase == Phase::SortRun));
    assert!(observations.iter().any(|v| v.phase == Phase::Merge));
    assert_eq!(evidence.len(), observations.len());
    let mut expected = candidates.clone();
    expected.reverse();
    expected.sort_by(|a, b| a.title.encode_utf16().cmp(b.title.encode_utf16()));
    assert_eq!(
        output,
        expected.iter().map(|v| v.id.clone()).collect::<Vec<_>>()
    );
    pool.close_all(&mut host).unwrap();
    drop(pool);
    drop(manager);
    drop(host);
    drop(package);
    assert!(source.starts_with(dir.path()) && source.file_name().unwrap() == "query-source");
    std::fs::remove_dir_all(&source).unwrap();
    for (evidence, observation) in evidence.iter().zip(&observations) {
        let invocation = Invocation::decode(&evidence.data().invocation).unwrap();
        assert_eq!(invocation.transform().unwrap().input, observation.request);
        let replayed = replay::replay(evidence, Limits::default()).unwrap();
        assert!(replayed.matches);
        assert_eq!(replayed.report.output.unwrap().bytes, observation.response);
    }
    assert_eq!(
        query_plan::replay(1, &conditions, candidates.clone(), &observations).unwrap(),
        output
    );
    assert!(
        query_plan::replay(
            1,
            &conditions,
            candidates,
            &observations[..observations.len() - 1]
        )
        .is_err()
    );
}

#[test]
fn empty_query_checks_live_plugin_and_read_capability_and_invalid_conditions() {
    let dir = tempfile::tempdir().unwrap();
    let mut missing = Workbench::open(&dir.path().join("missing.db"), None).unwrap();
    assert!(missing.query("概览", "全部", "", "最近添加").is_err());
    let active = dir.path().join("active");
    std::fs::create_dir(&active).unwrap();
    let mut host = Workbench::open(&active.join("db"), Some(common::package())).unwrap();
    assert!(
        host.query("概览", "全部", "", "最近添加")
            .unwrap()
            .is_empty()
    );
    assert!(host.query("not-a-section", "全部", "", "最近添加").is_err());
    assert!(host.query("概览", "全部", "", "not-a-sort").is_err());
    let status = host.plugin_status();
    host.configure_plugin(status.revision, &status.digest, false)
        .unwrap();
    assert!(host.query("概览", "全部", "", "最近添加").is_err());
    let status = host.plugin_status();
    host.configure_plugin(status.revision, &status.digest, true)
        .unwrap();
    assert!(
        host.query("概览", "全部", "", "最近添加")
            .unwrap()
            .is_empty()
    );
    let no_read = dir.path().join("no-read");
    std::fs::create_dir(&no_read).unwrap();
    let original = common::package();
    let mut manifest = original.manifest().clone();
    manifest
        .requested_capabilities
        .retain(|v| *v != Capability::ReadContent as i32);
    let package = Package::build(manifest, original.module()).unwrap();
    let mut host = Workbench::open(&no_read.join("db"), Some(package)).unwrap();
    assert!(host.plugin_status().enabled);
    assert!(host.query("概览", "全部", "", "最近添加").is_err());
}
