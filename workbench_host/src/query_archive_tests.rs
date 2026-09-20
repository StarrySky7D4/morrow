//! Qualification adapter only. The normal query transport does not persist these archives.
use crate::test_common as common;
use crate::{Result, Workbench, command, now, query_plan};
use morrow_core::{
    content::CardRecord,
    lifecycle::{GrantKind, InstancePhase},
    plugin_package::Package,
    read_archive::{Budget, Finish, Plan},
    store::{CardReadSnapshot, FrozenCard},
    task::{Invocation, Transform},
    task_evidence,
};
use morrow_plugin_runtime::{Limits, replay};
use morrow_workbench_plugin::{Action, Idea, Request, Response, codec, persistence};
use prost::Message;
use sha2::{Digest, Sha256};
mod proto {
    include!(concat!(
        env!("OUT_DIR"),
        "/morrow.workbench.query_capture.v1.rs"
    ));
}
const SUBJECT: &str = "test.query-capture";
const OPERATION: &str = "query-archive";
const TOTAL_FUEL: u64 = 1_000_000_000;
fn part_type(phase: query_plan::Phase) -> &'static str {
    match phase {
        query_plan::Phase::Filter => "test.query.filter.v2",
        query_plan::Phase::SortRun => "test.query.sort.v2",
        query_plan::Phase::Merge => "test.query.merge.v2",
    }
}
struct Capture<'a> {
    host: &'a mut Workbench,
    snapshot: CardReadSnapshot,
    pending: std::vec::IntoIter<FrozenCard>,
    eof: bool,
    ordinal: u32,
    observations: u64,
    remaining: u64,
}
impl Capture<'_> {
    fn append(&mut self, kind: &str, raw: &[u8]) -> Result<()> {
        self.host.host.store_local_mut().append_read_archive(
            SUBJECT,
            OPERATION,
            self.ordinal,
            kind,
            raw,
        )?;
        self.ordinal += 1;
        Ok(())
    }
}
impl query_plan::Backend for Capture<'_> {
    fn next_candidate(&mut self) -> Result<Option<Idea>> {
        loop {
            if let Some(entry) = self.pending.next() {
                let idea = if entry.card().summary().type_id == "org.morrow.idea" {
                    self.host.grant(entry.id(), GrantKind::ReadContent)?;
                    let start = self.host.start;
                    let result = self.host.host.read_snapshot_content(
                        self.host
                            .pool
                            .root(self.host.plugin.as_ref().unwrap())?
                            .connection(),
                        &self.snapshot,
                        &entry,
                        || now(start),
                    );
                    self.host.revoke(entry.id(), GrantKind::ReadContent)?;
                    Some(Workbench::decode(&result?)?.idea)
                } else {
                    None
                };
                self.append(
                    "test.query.source-fact.v1",
                    &proto::SourceFact {
                        operation_id: entry.latest_operation_id().into(),
                        sequence: entry.latest_sequence(),
                        commit_sha256: entry.latest_commit_sha256().to_vec(),
                    }
                    .encode_to_vec(),
                )?;
                // Keep an 8 MiB CardRecord within one bounded part; its facts are separate.
                self.append("test.query.card.v1", entry.original_bytes())?;
                if idea.is_some() {
                    return Ok(idea);
                }
                continue;
            }
            if self.eof {
                return Ok(None);
            }
            let page = self.snapshot.next_page(32, 16 * 1024 * 1024)?;
            self.eof = page.done;
            self.pending = page.entries.into_iter();
        }
    }
    fn invoke(&mut self, phase: query_plan::Phase, request: Request) -> Result<Response> {
        let input = Invocation::new_transform(
            &format!("captured-{}", self.observations),
            Transform {
                handler: "workbench.command".into(),
                input_type: "morrow.workbench.request.v1".into(),
                output_type: "morrow.workbench.response.v1".into(),
                input: codec::encode_request(&request)?,
            },
        )?;
        let captured = self.host.pool.record_transform_observation(
            self.host.manager.as_ref().unwrap(),
            &mut self.host.host,
            self.host.plugin.as_ref().unwrap(),
            &input,
            self.remaining,
        )?;
        assert_eq!(
            captured.package_digest(),
            self.host.bundle.as_ref().unwrap().digest()
        );
        let budget = captured.observation().data().budget.as_ref().unwrap();
        self.remaining = self
            .remaining
            .checked_sub(budget.fuel - captured.report().execution.fuel_remaining)
            .ok_or("query total fuel exhausted")?;
        self.append(part_type(phase), captured.observation().raw())?;
        self.observations += 1;
        Ok(codec::decode_response(
            &captured.report().output.as_ref().unwrap().bytes,
        )?)
    }
}
fn feed_text(hash: &mut Sha256, text: &str) {
    hash.update((text.len() as u32).to_le_bytes());
    hash.update(text.as_bytes());
}
fn replay_restored(store: &morrow_core::store::Store) -> Result<(Vec<String>, u64)> {
    let observation = store.lookup_read(SUBJECT, OPERATION)?.unwrap();
    // Pin comes from the retained observation, never from an unverified manifest.
    let expected_root: [u8; 32] = observation.data().archive_sha256.as_slice().try_into()?;
    let mut cursor = store.open_read_archive_cursor(SUBJECT, OPERATION, expected_root)?;
    store.validate_read_archive_cursor(&cursor)?;
    let manifest = cursor.manifest().clone();
    assert_eq!(expected_root, manifest.digest());
    let end = proto::CaptureEnd::decode(manifest.metadata().unwrap())?;
    assert_eq!(end.schema_version, 1);
    assert_eq!(end.fuel_budget, TOTAL_FUEL);
    assert_eq!(manifest.metadata_type(), Some("test.query.end.v1"));
    assert_eq!(
        manifest.status().plan.request_type,
        "morrow.workbench.request.v1"
    );
    assert_eq!(manifest.status().plan.response_type, "test.query.result.v1");
    assert_eq!(end.database_version, morrow_core::store::SCHEMA_VERSION as u32);
    let request = codec::decode_request(&manifest.status().plan.request)?;
    let conditions = query_plan::Conditions {
        section: request.section,
        filter: request.filter,
        text: request.text,
        sort: request.sort,
    };
    assert!(cursor.finish().is_err(), "no success before archive EOF");
    let first_page = cursor.next_page(1, 32 * 1024 * 1024)?;
    assert_eq!(first_page.parts.len(), 1);
    let mut done = first_page.done;
    let first = &first_page.parts[0];
    assert_eq!(first.ordinal(), 0);
    assert_eq!(first.type_id(), "test.query.package.v1");
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
                "test.query.source-fact.v1" => {
                    assert!(fact.is_none());
                    fact = Some(proto::SourceFact::decode(part.data())?);
                }
                "test.query.card.v1" => {
                    let fact = fact.take().ok_or("missing source fact")?;
                    let card = CardRecord::decode(part.data())?;
                    let s = card.summary();
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
                        candidates.push(Workbench::decode(&card)?.idea);
                    }
                }
                kind => {
                    assert!(fact.is_none());
                    let phase = match kind {
                        "test.query.filter.v2" => query_plan::Phase::Filter,
                        "test.query.sort.v2" => query_plan::Phase::SortRun,
                        "test.query.merge.v2" => query_plan::Phase::Merge,
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
    assert_eq!(source.finalize().as_slice(), end.source_sha256);
    assert_eq!(observations.len() as u64, end.observation_count);
    let ids = query_plan::replay(
        end.plan_version,
        &conditions,
        candidates.clone(),
        &observations,
    )?;
    let result = proto::QueryResult::decode(observation.data().response.as_slice())?;
    assert_eq!(result.schema_version, 1);
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
fn qualify(count: usize) {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    std::fs::create_dir(&source).unwrap();
    let db = source.join("workbench.db");
    let mut host = Workbench::open(&db, Some(common::package())).unwrap();
    for i in 0..count {
        let mut idea = common::idea(&format!("card-{i:03}"));
        idea.title = format!("Title {:03}", count - i);
        host.host
            .store_local_mut()
            .create_local(
                &format!("seed-{i}"),
                &CardRecord::new(
                    &idea.id,
                    "org.morrow.idea",
                    1,
                    &idea.title,
                    persistence::encode(&idea, None).unwrap(),
                )
                .unwrap(),
            )
            .unwrap();
    }
    host.host.flush_pending().unwrap();
    let snapshot = host.host.store_local().open_card_snapshot().unwrap();
    let point = snapshot.readpoint().clone();
    let mut request = command(Action::Query);
    request.section = "概览".into();
    request.filter = "全部".into();
    request.sort = "标题排序".into();
    let conditions = query_plan::Conditions {
        section: request.section.clone(),
        filter: request.filter.clone(),
        text: request.text.clone(),
        sort: request.sort.clone(),
    };
    host.host
        .store_local_mut()
        .begin_read_archive(&Plan {
            operation_id: OPERATION.into(),
            subject: SUBJECT.into(),
            request_type: "morrow.workbench.request.v1".into(),
            request: codec::encode_request(&request).unwrap(),
            response_type: "test.query.result.v1".into(),
            budget: Budget {
                max_parts: 2048,
                max_bytes: 128 * 1024 * 1024,
            },
        })
        .unwrap();
    let package = host.bundle.as_ref().unwrap().archive().to_vec();
    let mut capture = Capture {
        host: &mut host,
        snapshot,
        pending: vec![].into_iter(),
        eof: false,
        ordinal: 0,
        observations: 0,
        remaining: TOTAL_FUEL,
    };
    capture.append("test.query.package.v1", &package).unwrap();
    let ids = query_plan::execute(&conditions, &mut capture).unwrap();
    let census = capture.snapshot.finish().unwrap();
    let observations = capture.observations;
    capture.snapshot.close().unwrap();
    assert_eq!(ids.len(), count);
    if count > 128 {
        assert!(observations >= 5);
    } else if count == 0 {
        assert_eq!(
            observations, 1,
            "empty queries execute the actual Filter guest"
        );
    }
    let status = host
        .host
        .store_local()
        .lookup_read_archive(SUBJECT, OPERATION)
        .unwrap()
        .unwrap();
    assert!(status.root.is_none());
    assert!(
        host.host
            .store_local()
            .lookup_read(SUBJECT, OPERATION)
            .unwrap()
            .is_none()
    );
    let digest = host.bundle.as_ref().unwrap().digest();
    let end = proto::CaptureEnd {
        schema_version: 1,
        plan_version: query_plan::VERSION,
        database_version: point.database_version,
        operation_sequence: point.operation_sequence,
        operation_sha256: point.operation_sha256.map(|v| v.to_vec()),
        source_count: census.count,
        source_sha256: census.sha256.to_vec(),
        package_sha256: digest.to_vec(),
        fuel_budget: TOTAL_FUEL,
        observation_count: observations,
    };
    let connection = host
        .pool
        .root(host.plugin.as_ref().unwrap())
        .unwrap()
        .connection();
    assert_eq!(
        host.host.connection_phase(connection).unwrap(),
        InstancePhase::Ready
    );
    let manager = host.manager.as_ref().unwrap();
    let pool = &host.pool;
    let session = host.plugin.as_ref().unwrap();
    host.host
        .store_local_mut()
        .finish_read_archive_local_authorized(
            SUBJECT,
            OPERATION,
            &Finish {
                response: proto::QueryResult {
                    schema_version: 1,
                    ids: ids.clone(),
                }
                .encode_to_vec(),
                part_count: status.count,
                logical_bytes: status.logical_bytes,
                chain_sha256: status.chain_sha256,
                metadata_type: "test.query.end.v1".into(),
                metadata: end.encode_to_vec(),
            },
            &[],
            || {
                let selected = manager
                    .selection("org.morrow.workbench")
                    .ok_or(morrow_core::Error::Integrity)?;
                if !selected.enabled
                    || selected.digest != digest
                    || !selected.approved.contains(&GrantKind::ReadContent)
                    || pool
                        .root(session)
                        .map_err(|_| morrow_core::Error::Integrity)?
                        .package()
                        .package()
                        .digest()
                        != digest
                {
                    return Err(morrow_core::Error::Integrity);
                }
                Ok(())
            },
        )
        .unwrap();
    host.host.flush_pending().unwrap();
    let backup = dir.path().join("query.morrowbackup");
    host.backup_snapshot(&backup).unwrap();
    let original = host
        .host
        .store_local()
        .lookup_read(SUBJECT, OPERATION)
        .unwrap()
        .unwrap();
    host.finish().unwrap();
    drop(host);
    // Only this test's validated temporary source is removed, including its own credentials.
    assert!(source.starts_with(dir.path()) && source.file_name().unwrap() == "source");
    std::fs::remove_dir_all(&source).unwrap();
    let destination = dir.path().join("restored");
    morrow_audit::snapshot::restore(&backup, &destination).unwrap();
    let restored = morrow_audit::session::Session::open(
        &destination.join("workbench.db"),
        Default::default(),
        morrow_audit::session::OpenMode::Existing,
    )
    .unwrap();
    assert_eq!(
        restored
            .store()
            .lookup_read(SUBJECT, OPERATION)
            .unwrap()
            .unwrap()
            .container(),
        original.container()
    );
    assert_eq!(restored.store().pending_usage().unwrap().0, 0);
    assert_eq!(
        replay_restored(restored.store()).unwrap(),
        (ids, observations)
    );
}
#[test]
fn actual_rust_multiphase_query_archive_survives_source_removal_and_replays() {
    qualify(130);
}
#[test]
fn actual_empty_query_archive_contains_and_replays_filter_call() {
    qualify(0);
}
