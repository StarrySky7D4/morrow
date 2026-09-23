#![cfg(target_os = "windows")]
use super::{CONTEXT, QueryFailure, RESULT, SUBJECT, TOTAL_FUEL, proto};
use crate::test_common as common;
use crate::{Result, Workbench, query_plan_v2 as query_plan};
use morrow_core::{
    content::CardRecord,
    content_change::ContentChange,
    lifecycle::GrantKind,
    plugin_package::Package,
    read_archive::{Budget, Plan},
    read_capture::{Phase, State},
    store::{Census, Store},
    task::Invocation,
    task_evidence,
};
use morrow_plugin_runtime::{Limits, replay};
use morrow_workbench_plugin::{
    Idea, persistence,
    query_v2::{Candidate, Conditions, Request},
    query_v2_codec as codec,
};
use prost::Message;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

fn package() -> Package {
    let old = common::package();
    let mut manifest = old.manifest().clone();
    manifest.package_version = morrow_workbench_plugin::PACKAGE_VERSION.into();
    for (handler, input, output) in [
        (
            "workbench.query.v2",
            "morrow.workbench.query.request.v2",
            "morrow.workbench.query.response.v2",
        ),
        (
            "workbench.tasks.v2",
            "morrow.workbench.tasks.request.v2",
            "morrow.workbench.tasks.response.v2",
        ),
        (
            "workbench.cards.v2",
            "morrow.workbench.cards.request.v2",
            "morrow.workbench.cards.response.v2",
        ),
    ] {
        manifest
            .transform_handlers
            .push(morrow_core::plugin_package::proto::TransformHandler {
                handler: handler.into(),
                input_type: input.into(),
                output_type: output.into(),
                max_input_bytes: 65536,
                max_output_bytes: 65536,
            });
    }
    Package::build(manifest, old.module()).unwrap()
}
fn migrate(host: &mut Workbench, id: &str) {
    let plan = host.plan_tasks_migration(id).unwrap();
    host.migrate_tasks(&plan.operation, id, plan.source_revision)
        .unwrap();
}

fn seed(host: &mut Workbench, id: &str, title: &str, description: &str) {
    let host = host.local_state_mut().unwrap();
    let mut idea = common::idea(id);
    idea.title = title.into();
    idea.description = description.into();
    host.host
        .store_local_mut()
        .create_local(
            &format!("seed-{id}"),
            &CardRecord::new(
                id,
                "org.morrow.idea",
                1,
                title,
                persistence::encode(&idea, None).unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
}
fn replace(host: &mut Workbench, operation: &str, idea: Idea) {
    let host = host.local_state_mut().unwrap();
    let prior = host.host.store_local().card(&idea.id).unwrap().unwrap();
    let change = ContentChange {
        operation_id: operation.into(),
        card_id: idea.id.clone(),
        expected_revision: prior.summary().revision,
        title: idea.title.clone(),
        body: persistence::encode(&idea, Some(&prior.body())).unwrap(),
        preview_text: String::new(),
        attachments: None,
    };
    host.grant(&idea.id, GrantKind::EditContent).unwrap();
    let connection = host
        .pool
        .root(host.plugin.as_ref().unwrap())
        .unwrap()
        .connection();
    let start = host.start;
    host.host
        .edit_content(connection, &change, || crate::now(start))
        .unwrap();
    host.revoke(&idea.id, GrantKind::EditContent).unwrap();
}
fn query(host: &mut Workbench, operation: &str) -> Result<Vec<String>> {
    host.query_versioned_with_operation(operation, "概览", "全部", "", "标题排序")
}
fn state(host: &Workbench, operation: &str) -> State {
    let host = host.local_state().unwrap();
    host.host
        .store_local()
        .lookup_read_capture(SUBJECT, operation)
        .unwrap()
        .unwrap()
}
fn terminal(error: &(dyn std::error::Error + 'static)) {
    assert!(error.downcast_ref::<QueryFailure>().unwrap().terminal);
}
fn prepare(host: &mut Workbench, operation: &str) -> State {
    let host = host.local_state_mut().unwrap();
    let snapshot = host.host.store_local().open_card_snapshot().unwrap();
    let point = snapshot.readpoint().clone();
    snapshot.close().unwrap();
    let request = Request::Filter {
        conditions: Conditions {
            section: "概览".into(),
            filter: "全部".into(),
            text: String::new(),
            sort: "标题排序".into(),
        },
        candidates: vec![],
    };
    let context = proto::CaptureContext {
        schema_version: 2,
        plan_version: query_plan::VERSION,
        database_version: point.database_version,
        operation_sequence: point.operation_sequence,
        operation_sha256: point.operation_sha256.map(|v| v.to_vec()),
        package_sha256: host.bundle.as_ref().unwrap().digest().to_vec(),
        fuel_budget: TOTAL_FUEL,
    };
    host.host
        .store_local_mut()
        .begin_read_capture(
            &Plan {
                operation_id: operation.into(),
                subject: SUBJECT.into(),
                request_type: "morrow.workbench.query.request.v2".into(),
                request: codec::encode_request(&request).unwrap(),
                response_type: RESULT.into(),
                budget: Budget {
                    max_parts: 100,
                    max_bytes: 16 * 1024 * 1024,
                },
            },
            CONTEXT,
            &context.encode_to_vec(),
            host.query_owner,
        )
        .unwrap()
}

fn feed_text(hash: &mut Sha256, text: &str) {
    hash.update((text.len() as u32).to_le_bytes());
    hash.update(text.as_bytes());
}
fn replay_production(
    store: &Store,
    operation: &str,
    census: &Census,
    originals: &BTreeMap<String, Vec<u8>>,
) -> Result<(Vec<String>, u64)> {
    let observation = store.lookup_read(SUBJECT, operation)?.unwrap();
    // Pin comes from the retained observation, never from an unverified manifest.
    let expected_root: [u8; 32] = observation.data().archive_sha256.as_slice().try_into()?;
    let mut cursor = store.open_read_archive_cursor(SUBJECT, operation, expected_root)?;
    store.validate_read_archive_cursor(&cursor)?;
    let manifest = cursor.manifest().clone();
    assert_eq!(expected_root, manifest.digest());
    let end = proto::CaptureEnd::decode(manifest.metadata().unwrap())?;
    assert_eq!(end.schema_version, 2);
    assert_eq!(end.fuel_budget, TOTAL_FUEL);
    assert_eq!(manifest.metadata_type(), Some("morrow.query.end.v2"));
    assert_eq!(
        manifest.status().plan.request_type,
        "morrow.workbench.query.request.v2"
    );
    assert_eq!(
        manifest.status().plan.response_type,
        "morrow.query.result.v2"
    );
    let state = store.lookup_read_capture(SUBJECT, operation)?.unwrap();
    assert_eq!(state.phase(), Phase::Ready);
    let context = proto::CaptureContext::decode(state.context())?;
    assert_eq!(state.context_type(), CONTEXT);
    assert_eq!(context.schema_version, 2);
    assert_eq!(context.plan_version, end.plan_version);
    assert_eq!(context.database_version, end.database_version);
    assert_eq!(context.operation_sequence, end.operation_sequence);
    assert_eq!(context.operation_sha256, end.operation_sha256);
    assert_eq!(context.package_sha256, end.package_sha256);
    assert_eq!(context.fuel_budget, TOTAL_FUEL);
    let Request::Filter {
        conditions,
        candidates: empty,
    } = codec::decode_request(&manifest.status().plan.request)?
    else {
        return Err("query archive conditions are not filter".into());
    };
    assert!(empty.is_empty());
    assert!(cursor.finish().is_err(), "no success before archive EOF");
    let first_page = cursor.next_page(1, 32 * 1024 * 1024)?;
    assert_eq!(first_page.parts.len(), 1);
    let mut done = first_page.done;
    let first = &first_page.parts[0];
    assert_eq!(first.ordinal(), 0);
    assert_eq!(first.type_id(), "morrow.query.package.v2");
    let package = Package::decode(first.data())?;
    assert_eq!(end.package_sha256, package.digest());
    let mut source = Sha256::new();
    source.update(b"Morrow/card-census/v1\0");
    let mut source_count = 0u64;
    let mut last_id = String::new();
    let mut fact = None;
    let mut candidates = Vec::new();
    let mut observations = Vec::new();
    let mut remaining = end.fuel_budget;
    let mut ordinal = 1;
    while !done {
        // One owned snapshot supplies the entire transcript; no per-part Store reopen.
        let page = cursor.next_page(7, 32 * 1024 * 1024)?;
        assert!(!page.parts.is_empty());
        done = page.done;
        for part in page.parts {
            assert_eq!(part.ordinal(), ordinal);
            ordinal += 1;
            match part.type_id() {
                "morrow.query.source-fact.v2" => {
                    assert!(fact.is_none());
                    fact = Some(proto::SourceFact::decode(part.data())?);
                }
                "morrow.query.card.v2" => {
                    let fact = fact.take().ok_or("missing source fact")?;
                    let card = CardRecord::decode(part.data())?;
                    let s = card.summary();
                    assert_eq!(originals.get(&s.id).unwrap().as_slice(), part.data());
                    assert!(s.id > last_id);
                    last_id = s.id.clone();
                    assert!(fact.sequence > 0 && fact.sequence <= end.operation_sequence);
                    assert_eq!(fact.commit_sha256.len(), 32);
                    source.update(source_count.to_le_bytes());
                    feed_text(&mut source, &s.id);
                    feed_text(&mut source, &s.type_id);
                    source.update(s.format_version.to_le_bytes());
                    source.update(s.revision.to_le_bytes());
                    source.update((part.data().len() as u64).to_le_bytes());
                    source.update(Sha256::digest(part.data()));
                    feed_text(&mut source, &fact.operation_id);
                    source.update(fact.sequence.to_le_bytes());
                    source.update(fact.commit_sha256);
                    source_count += 1;
                    if s.type_id == "org.morrow.idea" {
                        crate::versioned_record::decode(&card)?;
                        candidates.push(Candidate {
                            id: s.id.clone(),
                            title: s.title.clone(),
                            format_version: s.format_version,
                            properties: card.body(),
                        });
                    }
                }
                kind => {
                    assert!(fact.is_none());
                    let phase = match kind {
                        "morrow.query.filter.v2" => query_plan::Phase::Filter,
                        "morrow.query.sort.v2" => query_plan::Phase::SortRun,
                        "morrow.query.merge.v2" => query_plan::Phase::Merge,
                        _ => return Err("unknown query part".into()),
                    };
                    let record = task_evidence::decode_observation(&package, part.data())?;
                    let budget = record.data().budget.as_ref().unwrap();
                    if budget.fuel > remaining {
                        return Err("replay total fuel exhausted".into());
                    }
                    let replayed = replay::replay_observation(
                        &package,
                        &record,
                        Limits {
                            fuel: 100_000_000,
                            memory_bytes: 64 * 1024 * 1024,
                            host_calls: 1024,
                        },
                    )?;
                    assert!(replayed.matches);
                    remaining = remaining
                        .checked_sub(budget.fuel - replayed.report.execution.fuel_remaining)
                        .ok_or("replay total fuel exhausted")?;
                    let invocation = Invocation::decode(&record.data().invocation)?;
                    let transform = invocation.transform().ok_or("not transform")?;
                    assert_eq!(transform.handler, "workbench.query.v2");
                    assert_eq!(transform.input_type, "morrow.workbench.query.request.v2");
                    assert_eq!(transform.output_type, "morrow.workbench.query.response.v2");
                    observations.push(query_plan::Observation {
                        phase,
                        request: invocation.transform().unwrap().input.clone(),
                        response: replayed.report.output.unwrap().bytes,
                    });
                }
            }
        }
    }
    assert_eq!(ordinal, manifest.status().count);
    cursor.finish()?;
    let exhausted = cursor.next_page(7, 32 * 1024 * 1024)?;
    assert!(exhausted.done && exhausted.parts.is_empty());
    cursor.close()?;
    assert!(fact.is_none());
    source.update(b"\0end");
    source.update(source_count.to_le_bytes());
    assert_eq!(source_count, end.source_count);
    assert_eq!(source_count, census.count);
    assert_eq!(source_count as usize, originals.len());
    assert_eq!(end.source_sha256.as_slice(), census.sha256.as_slice());
    assert_eq!(source.finalize().as_slice(), end.source_sha256);
    assert_eq!(observations.len() as u64, end.observation_count);
    let ids = query_plan::replay(
        end.plan_version,
        &conditions,
        candidates.clone(),
        &observations,
    )?;
    let result = proto::QueryResult::decode(observation.data().response.as_slice())?;
    assert_eq!(result.schema_version, 2);
    assert!(result.ids.len() <= 4096);
    assert_eq!(result.ids, ids);
    if !observations.is_empty() {
        assert!(
            query_plan::replay(
                end.plan_version,
                &conditions,
                candidates,
                &observations[..observations.len() - 1]
            )
            .is_err()
        );
    }
    Ok((ids, end.observation_count))
}

#[test]
fn same_operation_retries_original_results_without_guest_runs_and_survives_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("workbench.db");
    let mut host = Workbench::open(&db, Some(package())).unwrap();
    seed(&mut host, "a", "Z", "original");
    seed(&mut host, "b", "A", "original");
    assert_eq!(query(&mut host, "fixed").unwrap(), ["b", "a"]);
    let original = host
        .local_state()
        .unwrap()
        .host
        .store_local()
        .lookup_read(SUBJECT, "fixed")
        .unwrap()
        .unwrap();
    let original_state = state(&host, "fixed");
    let executions = host.local_state().unwrap().counter;
    let mut changed = host.read("a").unwrap().idea;
    changed.title = "0".into();
    replace(&mut host, "rename", changed);
    seed(&mut host, "c", "M", "new");
    assert_eq!(query(&mut host, "fixed").unwrap(), ["b", "a"]);
    assert_eq!(
        host.local_state().unwrap().counter,
        executions,
        "a retry must not execute another guest task"
    );
    let error = host
        .query_versioned_with_operation("fixed", "概览", "全部", "other intent", "标题排序")
        .unwrap_err();
    terminal(error.as_ref());
    assert_eq!(
        state(&host, "fixed").container(),
        original_state.container()
    );
    assert_eq!(query(&mut host, "fresh").unwrap(), ["a", "b", "c"]);
    host.finish().unwrap();
    drop(host);
    let mut reopened = Workbench::open(&db, Some(package())).unwrap();
    let before = reopened.local_state().unwrap().counter;
    assert_eq!(query(&mut reopened, "fixed").unwrap(), ["b", "a"]);
    assert_eq!(reopened.local_state().unwrap().counter, before);
    assert_eq!(
        reopened
            .local_state()
            .unwrap()
            .host
            .store_local()
            .lookup_read(SUBJECT, "fixed")
            .unwrap()
            .unwrap()
            .container(),
        original.container()
    );
    assert_eq!(
        query(&mut reopened, "after-reopen").unwrap(),
        ["a", "b", "c"]
    );
    reopened.finish().unwrap();
}
#[test]
fn persisted_preparation_is_interrupted_on_reopen_and_cancelled_intent_is_not_rebound() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("workbench.db");
    let mut host = Workbench::open(&db, Some(package())).unwrap();
    let pending = prepare(&mut host, "preparing");
    let package_archive = host
        .local_state()
        .unwrap()
        .bundle
        .as_ref()
        .unwrap()
        .archive()
        .to_vec();
    host.local_state_mut()
        .unwrap()
        .host
        .store_local_mut()
        .append_read_capture(
            SUBJECT,
            "preparing",
            &pending.token(),
            0,
            "morrow.query.package.v2",
            &package_archive,
        )
        .unwrap();
    let cancelled = prepare(&mut host, "cancelled");
    host.local_state_mut()
        .unwrap()
        .host
        .store_local_mut()
        .end_read_capture(
            SUBJECT,
            "cancelled",
            &cancelled.token(),
            Phase::Cancelled,
            "explicit_test_cancel",
        )
        .unwrap();
    // A real persistent preparation is left without its original snapshot. This models
    // restart recovery; it is not a claim that this test killed an OS process.
    host.finish().unwrap();
    drop(host);
    let mut reopened = Workbench::open(&db, Some(package())).unwrap();
    assert_eq!(state(&reopened, "preparing").phase(), Phase::Interrupted);
    assert_eq!(
        state(&reopened, "preparing").reason(),
        "host_restarted_snapshot_unavailable"
    );
    assert_eq!(state(&reopened, "cancelled").phase(), Phase::Cancelled);
    let before = reopened.local_state().unwrap().counter;
    for operation in ["preparing", "cancelled"] {
        terminal(query(&mut reopened, operation).unwrap_err().as_ref());
        assert!(
            reopened
                .local_state()
                .unwrap()
                .host
                .store_local()
                .lookup_read(SUBJECT, operation)
                .unwrap()
                .is_none()
        );
    }
    assert_eq!(reopened.local_state().unwrap().counter, before);
    assert!(
        query(&mut reopened, "new-after-interruption")
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        state(&reopened, "new-after-interruption").phase(),
        Phase::Ready
    );
    reopened.finish().unwrap();
}
#[test]
fn ready_result_requires_current_read_approval_without_demoting_its_history() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("workbench.db");
    let mut host = Workbench::open(&db, Some(package())).unwrap();
    seed(&mut host, "a", "A", "body");
    query(&mut host, "ready").unwrap();
    let original = state(&host, "ready");
    let digest = host
        .local_state()
        .unwrap()
        .bundle
        .as_ref()
        .unwrap()
        .digest();
    let manager = host.local_state_mut().unwrap().manager.as_mut().unwrap();
    let mut approved = crate::default_approval();
    approved.remove(&GrantKind::ReadContent);
    manager
        .approve("org.morrow.workbench", digest, approved, manager.revision())
        .unwrap();
    assert!(query(&mut host, "ready").is_err());
    assert_eq!(state(&host, "ready").container(), original.container());
    let status = host.plugin_status().unwrap();
    host.configure_plugin(status.revision, &status.digest, true)
        .unwrap();
    let before = host.local_state().unwrap().counter;
    assert_eq!(query(&mut host, "ready").unwrap(), ["a"]);
    assert_eq!(host.local_state().unwrap().counter, before);
    host.finish().unwrap();
}
#[test]
fn capacity_failure_is_terminal_without_partial_observation_and_requires_a_fresh_id() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("workbench.db");
    let mut host = Workbench::open(&db, Some(package())).unwrap();
    seed(&mut host, "a", "A", "body");
    seed(
        &mut host,
        "oversized",
        &"Large".repeat(200),
        &"x".repeat(65000),
    );
    terminal(query(&mut host, "capacity").unwrap_err().as_ref());
    assert_eq!(state(&host, "capacity").phase(), Phase::Failed);
    assert!(
        host.local_state()
            .unwrap()
            .host
            .store_local()
            .lookup_read(SUBJECT, "capacity")
            .unwrap()
            .is_none()
    );
    assert!(host.writable());
    let mut changed = host.read("oversized").unwrap().idea;
    changed.description = "small".into();
    replace(&mut host, "shrink", changed);
    let before = host.local_state().unwrap().counter;
    terminal(query(&mut host, "capacity").unwrap_err().as_ref());
    assert_eq!(host.local_state().unwrap().counter, before);
    assert_eq!(
        query(&mut host, "capacity-fixed").unwrap(),
        ["a", "oversized"]
    );
    host.create("after-query-failure", common::idea("saved"))
        .unwrap();
    host.finish().unwrap();
}
#[test]
fn production_multiphase_archive_replays_all_originals_after_backup_and_source_removal() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    std::fs::create_dir(&source).unwrap();
    let db = source.join("workbench.db");
    let mut host = Workbench::open(&db, Some(package())).unwrap();
    for i in 0..130 {
        seed(
            &mut host,
            &format!("card-{i:03}"),
            &format!("Title {:03}", 130 - i),
            "original body",
        );
    }
    for i in (0..130).step_by(2) {
        migrate(&mut host, &format!("card-{i:03}"));
    }
    for i in 0..2 {
        host.local_state_mut()
            .unwrap()
            .host
            .store_local_mut()
            .create_local(
                &format!("seed-other-{i}"),
                &CardRecord::new(&format!("other-{i}"), "test.other", 1, "Other", vec![i]).unwrap(),
            )
            .unwrap();
    }
    host.local_state_mut()
        .unwrap()
        .host
        .flush_pending()
        .unwrap();
    let mut baseline = host
        .local_state()
        .unwrap()
        .host
        .store_local()
        .open_card_snapshot()
        .unwrap();
    let mut originals = BTreeMap::new();
    loop {
        let page = baseline.next_page(23, 4 * 1024 * 1024).unwrap();
        for entry in page.entries {
            originals.insert(entry.id().to_owned(), entry.original_bytes().to_vec());
        }
        if page.done {
            break;
        }
    }
    let census = baseline.finish().unwrap();
    baseline.close().unwrap();
    let ids = query(&mut host, "production-archive").unwrap();
    assert_eq!(ids.len(), 130);
    let replayed = replay_production(
        host.local_state().unwrap().host.store_local(),
        "production-archive",
        &census,
        &originals,
    )
    .unwrap();
    assert_eq!(replayed.0, ids);
    assert!(replayed.1 >= 5);
    let original = host
        .local_state()
        .unwrap()
        .host
        .store_local()
        .lookup_read(SUBJECT, "production-archive")
        .unwrap()
        .unwrap();
    let backup = dir.path().join("query.morrowbackup");
    host.backup_snapshot(&backup).unwrap();
    host.finish().unwrap();
    drop(host);
    assert!(source.starts_with(dir.path()) && source.file_name().unwrap() == "source");
    std::fs::remove_dir_all(&source).unwrap();
    let restored = dir.path().join("restored");
    morrow_audit::snapshot::restore(&backup, &restored).unwrap();
    let session = morrow_audit::session::Session::open(
        &restored.join("workbench.db"),
        Default::default(),
        morrow_audit::session::OpenMode::Existing,
    )
    .unwrap();
    assert_eq!(
        session
            .store()
            .lookup_read(SUBJECT, "production-archive")
            .unwrap()
            .unwrap()
            .container(),
        original.container()
    );
    assert_eq!(
        replay_production(session.store(), "production-archive", &census, &originals).unwrap(),
        replayed
    );
    assert_eq!(session.store().pending_usage().unwrap(), (0, 0));
}

#[test]
#[cfg(feature = "fault-injection")]
fn ready_after_seal_failure_retries_original_result_without_another_guest_call() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("workbench.db");
    let mut host = Workbench::open(&db, Some(package())).unwrap();
    seed(&mut host, "a", "A", "body");
    host.local_state_mut()
        .unwrap()
        .host
        .flush_pending()
        .unwrap();
    let executions_before = host.local_state().unwrap().counter;
    host.local_state_mut()
        .unwrap()
        .host
        .fail_next_seal_for_test();
    let error = query(&mut host, "seal-failure").unwrap_err();
    assert!(!error.downcast_ref::<QueryFailure>().unwrap().terminal);
    assert!(host.maintenance_warning().is_some());
    let ready = state(&host, "seal-failure");
    assert_eq!(ready.phase(), Phase::Ready);
    let original = host
        .local_state()
        .unwrap()
        .host
        .store_local()
        .lookup_read(SUBJECT, "seal-failure")
        .unwrap()
        .unwrap();
    assert_eq!(
        host.local_state()
            .unwrap()
            .host
            .store_local()
            .pending_usage()
            .unwrap()
            .0,
        1
    );
    let executed = host.local_state().unwrap().counter;
    assert!(
        executed > executions_before,
        "first attempt really executed the guest"
    );
    assert_eq!(query(&mut host, "seal-failure").unwrap(), ["a"]);
    assert_eq!(
        host.local_state().unwrap().counter,
        executed,
        "Ready retry never re-runs the guest"
    );
    assert_eq!(state(&host, "seal-failure").container(), ready.container());
    assert_eq!(
        host.local_state()
            .unwrap()
            .host
            .store_local()
            .lookup_read(SUBJECT, "seal-failure")
            .unwrap()
            .unwrap()
            .container(),
        original.container()
    );
    // Explicit maintenance can seal the same pending observation after the one-shot
    // failure. Its prior successful commit must not depend on that later maintenance.
    host.refresh_plugin_state().unwrap();
    assert!(host.maintenance_warning().is_none());
    assert_eq!(
        host.local_state()
            .unwrap()
            .host
            .store_local()
            .pending_usage()
            .unwrap(),
        (0, 0)
    );
    assert_eq!(state(&host, "seal-failure").container(), ready.container());
    assert_eq!(
        host.local_state()
            .unwrap()
            .host
            .store_local()
            .lookup_read(SUBJECT, "seal-failure")
            .unwrap()
            .unwrap()
            .container(),
        original.container()
    );
    host.finish().unwrap();
}

#[test]
fn existing_content_operation_cannot_become_a_query_retry_loop() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("workbench.db");
    let mut host = Workbench::open(&db, Some(package())).unwrap();
    seed(&mut host, "a", "original", "body");
    let before = host.local_state().unwrap().counter;
    let error = host
        .query_versioned_with_operation("seed-a", "概览", "全部", "", "最近添加")
        .unwrap_err();
    assert!(error.downcast_ref::<QueryFailure>().unwrap().terminal);
    assert_eq!(host.local_state().unwrap().counter, before);
    assert!(
        host.local_state()
            .unwrap()
            .host
            .store_local()
            .lookup_read_capture(SUBJECT, "seed-a")
            .unwrap()
            .is_none()
    );
    assert_eq!(
        host.query_versioned_with_operation("fresh-query", "概览", "全部", "", "最近添加")
            .unwrap(),
        ["a"]
    );
    host.finish().unwrap();
}

#[test]
fn mixed_formats_keep_legacy_projection_but_v2_explicit_stage_and_ambiguous_tasks() {
    use crate::{cards_content::CardAction, versioned_record::VersionedRecord};
    use morrow_workbench_plugin::tasks_v2::Command;
    let dir = tempfile::tempdir().unwrap();
    let mut host = Workbench::open(&dir.path().join("db"), Some(package())).unwrap();
    for id in ["legacy", "tasks", "done"] {
        let mut idea = common::idea(id);
        idea.category = "进行中".into();
        idea.stage = "计划中".into();
        idea.todos = if id == "tasks" {
            vec!["same".into(), "same".into()]
        } else {
            vec!["same".into()]
        };
        idea.completed = vec!["same".into()];
        host.create(&format!("create-{id}"), idea).unwrap();
    }
    // Preserve an actual old Ready query before any content migrates.
    let old = host
        .query_with_operation("old-ready", "小项目", "已完成", "", "最近添加")
        .unwrap();
    assert_eq!(old, ["tasks", "legacy", "done"]);
    migrate(&mut host, "tasks");
    migrate(&mut host, "done");
    assert_eq!(
        host.query_with_operation("old-ready", "小项目", "已完成", "", "最近添加")
            .unwrap(),
        old
    );
    let conflict = host
        .query_versioned_with_operation("old-ready", "小项目", "已完成", "", "最近添加")
        .unwrap_err();
    terminal(conflict.as_ref());
    assert_eq!(
        host.query_versioned("小项目", "已完成", "", "最近添加")
            .unwrap(),
        ["legacy"]
    );
    assert_eq!(
        host.query_versioned("小项目", "计划中", "", "最近添加")
            .unwrap(),
        ["tasks", "done"]
    );
    assert_eq!(
        host.query_versioned("概览", "有待办", "", "最近添加")
            .unwrap(),
        ["tasks"]
    );
    let VersionedRecord::Tasks(task) = host.read_versioned("tasks").unwrap() else {
        panic!()
    };
    let first = host
        .edit_tasks(
            "confirm-first",
            "tasks",
            2,
            &Command::SetCompletion {
                id: task.properties.tasks[0].id.clone(),
                complete: true,
            },
        )
        .unwrap();
    assert_eq!(first.committed.projection.ambiguous, 1);
    assert_eq!(
        host.query_versioned("概览", "有待办", "", "最近添加")
            .unwrap(),
        ["tasks"]
    );
    host.edit_tasks(
        "confirm-second",
        "tasks",
        3,
        &Command::SetCompletion {
            id: task.properties.tasks[1].id.clone(),
            complete: true,
        },
    )
    .unwrap();
    assert!(
        host.query_versioned("概览", "有待办", "", "最近添加")
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        host.query_versioned("小项目", "计划中", "", "最近添加")
            .unwrap(),
        ["tasks", "done"]
    );
    host.edit_card("delete-tasks", "tasks", 4, &CardAction::Delete)
        .unwrap();
    assert_eq!(
        host.query_versioned("概览", "全部", "", "最近添加")
            .unwrap(),
        ["legacy", "done"]
    );
    host.finish().unwrap();
}
#[test]
fn unsupported_format_is_terminal_and_never_silently_skipped() {
    let dir = tempfile::tempdir().unwrap();
    let mut host = Workbench::open(&dir.path().join("db"), Some(package())).unwrap();
    seed(&mut host, "valid", "Valid", "body");
    host.local_state_mut()
        .unwrap()
        .host
        .store_local_mut()
        .create_local(
            "seed-future",
            &CardRecord::new("future", "org.morrow.idea", 3, "Future", vec![]).unwrap(),
        )
        .unwrap();
    let failure = query(&mut host, "future-query").unwrap_err();
    terminal(failure.as_ref());
    assert_eq!(state(&host, "future-query").phase(), Phase::Failed);
    assert!(
        host.local_state()
            .unwrap()
            .host
            .store_local()
            .lookup_read(SUBJECT, "future-query")
            .unwrap()
            .is_none()
    );
    assert!(host.read("valid").is_ok());
    assert!(host.writable());
    host.finish().unwrap();
}
#[test]
fn v2_query_does_not_accept_properties_with_unselected_outer_attachments() {
    use morrow_workbench_plugin::{
        Asset,
        tasks_v2::{self, Baseline},
    };
    let dir = tempfile::tempdir().unwrap();
    let mut host = Workbench::open(&dir.path().join("db"), Some(package())).unwrap();
    let mut idea = common::idea("bad-asset");
    idea.assets.push(Asset {
        id: "asset".into(),
        name: "unselected".into(),
        kind: "file".into(),
        bytes: 1,
    });
    let old = persistence::encode(&idea, None).unwrap();
    let body = tasks_v2::migrate(
        &Baseline::capture(&idea.id, 1, &old).unwrap(),
        &idea.id,
        1,
        &idea.title,
        &old,
    )
    .unwrap();
    let record = CardRecord::new(&idea.id, "org.morrow.idea", 2, &idea.title, body).unwrap();
    host.local_state_mut()
        .unwrap()
        .host
        .store_local_mut()
        .create_local("seed-bad", &record)
        .unwrap();
    terminal(query(&mut host, "bad-asset-query").unwrap_err().as_ref());
    assert_eq!(state(&host, "bad-asset-query").phase(), Phase::Failed);
    host.finish().unwrap();
}

fn retention_budget(host: &mut Workbench, count: u32, bytes: u64) {
    host.local_state_mut()
        .unwrap()
        .host
        .store_local_mut()
        .set_read_archive_retention_budget(morrow_core::read_archive::RetentionBudget {
            max_archives: count,
            max_bytes: bytes,
        })
        .unwrap();
}
#[test]
fn retained_history_pressure_keeps_ready_but_blocks_fresh_execution() {
    let dir = tempfile::tempdir().unwrap();
    let mut host = Workbench::open(&dir.path().join("db"), Some(package())).unwrap();
    seed(&mut host, "a", "A", "body");
    assert_eq!(query(&mut host, "ready").unwrap(), ["a"]);
    let ready = state(&host, "ready");
    retention_budget(&mut host, 1, morrow_core::read_archive::MAX_RETAINED_BYTES);
    let calls = host.local_state().unwrap().counter;
    let error = query(&mut host, "capacity").unwrap_err();
    let failure = error.downcast_ref::<QueryFailure>().unwrap();
    assert!(failure.terminal && failure.capacity);
    assert_eq!(host.local_state().unwrap().counter, calls);
    assert_eq!(query(&mut host, "ready").unwrap(), ["a"]);
    assert_eq!(state(&host, "ready").container(), ready.container());
    host.finish().unwrap();
}
#[test]
fn cleanup_uncertainty_retains_operation_until_original_capture_is_resolved() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("db");
    let mut host = Workbench::open(&db, Some(package())).unwrap();
    retention_budget(&mut host, 4, 64 * 1024);
    let sql = rusqlite::Connection::open(&db).unwrap();
    sql.execute_batch("CREATE TRIGGER fail_capture_cleanup BEFORE DELETE ON read_archives BEGIN SELECT RAISE(ABORT,'test cleanup unavailable'); END;").unwrap();
    let error = query(&mut host, "uncertain").unwrap_err();
    let failure = error.downcast_ref::<QueryFailure>().unwrap();
    assert!(failure.capacity && !failure.terminal);
    assert_eq!(state(&host, "uncertain").phase(), Phase::Preparing);
    assert!(
        host.local_state()
            .unwrap()
            .host
            .store_local()
            .lookup_read(SUBJECT, "uncertain")
            .unwrap()
            .is_none()
    );
    sql.execute_batch("DROP TRIGGER fail_capture_cleanup;")
        .unwrap();
    terminal(query(&mut host, "uncertain").unwrap_err().as_ref());
    assert_eq!(state(&host, "uncertain").phase(), Phase::Interrupted);
    retention_budget(&mut host, 4, morrow_core::read_archive::MAX_RETAINED_BYTES);
    assert!(query(&mut host, "fresh").unwrap().is_empty());
    host.finish().unwrap();
}
#[test]
fn legacy_source_attachment_mismatch_also_fails_explicitly() {
    let dir = tempfile::tempdir().unwrap();
    let mut host = Workbench::open(&dir.path().join("db"), Some(package())).unwrap();
    let mut idea = common::idea("bad-legacy");
    idea.assets.push(morrow_workbench_plugin::Asset {
        id: "asset".into(),
        name: "missing".into(),
        kind: "file".into(),
        bytes: 1,
    });
    let record = CardRecord::new(
        &idea.id,
        "org.morrow.idea",
        1,
        &idea.title,
        persistence::encode(&idea, None).unwrap(),
    )
    .unwrap();
    host.local_state_mut()
        .unwrap()
        .host
        .store_local_mut()
        .create_local("seed-bad-legacy", &record)
        .unwrap();
    let error = query(&mut host, "mismatch").unwrap_err();
    assert!(error.to_string().contains("attachment metadata mismatch"));
    terminal(error.as_ref());
    assert_eq!(state(&host, "mismatch").phase(), Phase::Failed);
    host.finish().unwrap();
}
