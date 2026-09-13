//! Windows qualification using the actual bundled Rust guest and an owned temporary workbench.
//! Compares physical format-8 storage against the exact equivalent format-7 container payloads.
#[cfg(target_os = "windows")]
mod windows {
    use morrow_core::{plugin_package::catalog, task_evidence::Evidence};
    use morrow_plugin_runtime::{Limits, replay};
    use morrow_workbench_host::{Mutation, Workbench};
    use morrow_workbench_plugin::{Action, Idea};
    use rusqlite::{Connection, OpenFlags};
    use std::path::Path;

    const CREATES: usize = 30;
    type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
    struct Original {
        card: String,
        operation: String,
        evidence: Evidence,
    }
    #[derive(Debug, PartialEq)]
    struct StorageCounts {
        recipes: u64,
        recipe_bytes: u64,
        chunks: u64,
        chunk_bytes: u64,
        chunk_references: u64,
        operations: u64,
        cards: u64,
        pending: u64,
    }
    fn original(h: &Workbench, card: &str, operation: &str) -> Result<Original> {
        let mut evidence = h.operation_evidence(card, operation)?;
        assert_eq!(evidence.len(), 1);
        Ok(Original {
            card: card.into(),
            operation: operation.into(),
            evidence: evidence.remove(0),
        })
    }
    fn assert_original(h: &Workbench, item: &Original) -> Result<()> {
        let stored = original(h, &item.card, &item.operation)?;
        assert_eq!(stored.evidence.digest(), item.evidence.digest());
        assert_eq!(stored.evidence.raw(), item.evidence.raw());
        assert_eq!(stored.evidence.container(), item.evidence.container());
        Ok(())
    }
    fn number(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<u64> {
        let value: i64 = row.get(index)?;
        u64::try_from(value).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                index,
                rusqlite::types::Type::Integer,
                Box::new(error),
            )
        })
    }
    fn stats(path: &Path) -> Result<(StorageCounts, Vec<Vec<u8>>, u64)> {
        // Only this example's own, already-closed temporary database may reach this helper.
        let db = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        let version: i64 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        assert_eq!(
            version, 8,
            "physical sharing must be the real production format"
        );
        let (recipes, recipe_bytes) = db.query_row(
            "SELECT count(*),coalesce(sum(length(payload)),0) FROM task_evidence",
            [],
            |r| Ok((number(r, 0)?, number(r, 1)?)),
        )?;
        let (chunks, chunk_bytes) = db.query_row(
            "SELECT count(*),coalesce(sum(length(payload)),0) FROM evidence_chunks",
            [],
            |r| Ok((number(r, 0)?, number(r, 1)?)),
        )?;
        let counts = StorageCounts {
            recipes,
            recipe_bytes,
            chunks,
            chunk_bytes,
            chunk_references: db.query_row(
                "SELECT count(*) FROM task_evidence_chunks",
                [],
                |r| number(r, 0),
            )?,
            operations: db.query_row("SELECT count(*) FROM operations", [], |r| number(r, 0))?,
            cards: db.query_row("SELECT count(*) FROM cards", [], |r| number(r, 0))?,
            pending: db.query_row("SELECT count(*) FROM outbox", [], |r| number(r, 0))?,
        };
        let mut statement =
            db.prepare("SELECT payload FROM sealed_segments ORDER BY segment_index")?;
        let seals = statement
            .query_map([], |r| r.get(0))?
            .collect::<std::result::Result<Vec<Vec<u8>>, _>>()?;
        drop(statement);
        drop(db);
        Ok((counts, seals, std::fs::metadata(path)?.len()))
    }
    fn draft(n: usize) -> Idea {
        Idea {
            id: format!("shared-evidence-card-{n}"),
            title: format!("Actual workbench idea {n}"),
            description: format!("Distinct creation {n}: Markdown **body** and Unicode 灵感."),
            category: "灵感".into(),
            stage: "待整理".into(),
            ..Default::default()
        }
    }
    pub fn run() -> Result<()> {
        let args: Vec<_> = std::env::args_os().skip(1).collect();
        if args.len() != 1 {
            return Err("usage: qualify_shared_evidence ACTUAL_WORKBENCH.morrowplugin".into());
        }
        let package = catalog::read_file(Path::new(&args[0]))?;
        assert_eq!(package.manifest().package_id, "org.morrow.workbench");
        println!(
            "PACKAGE version={} archive_bytes={} wasm_bytes={}",
            package.manifest().package_version,
            package.archive().len(),
            package.module().len()
        );
        let temporary = tempfile::tempdir()?;
        let database = temporary.path().join("workbench.db");
        let mut originals = Vec::with_capacity(CREATES + 1);
        {
            let mut h = Workbench::open(
                &database,
                Some(morrow_core::plugin_package::Package::decode(
                    package.archive(),
                )?),
            )?;
            assert!(h.writable());
            for n in 0..CREATES {
                let item = draft(n);
                let operation = format!("shared-create-{n}");
                let saved = h.create(&operation, item.clone())?;
                assert_eq!(saved.revision, 1);
                assert_eq!(saved.idea.id, item.id);
                originals.push(original(&h, &item.id, &operation)?);
            }
            let first = draft(0);
            let changed = h.apply(Mutation {
                operation: "shared-favorite-0",
                id: &first.id,
                revision: 1,
                action: Action::Favorite,
                proposed: None,
                text: "",
                flag: true,
            })?;
            assert_eq!(changed.revision, 2);
            assert!(changed.idea.favorite);
            originals.push(original(&h, &first.id, "shared-favorite-0")?);
            h.finish()?;
        }
        let (before, signed_before, initial_file_bytes) = stats(&database)?;
        assert_eq!(before.cards, CREATES as u64);
        assert_eq!(before.operations, (CREATES + 1) as u64);
        assert_eq!(before.recipes, (CREATES + 1) as u64);
        assert_eq!(before.pending, 0);
        assert!(
            !signed_before.is_empty(),
            "normal workbench finish must seal the original commit references"
        );
        {
            // The actual Workbench reopen verifies the protected key, audit chain and content.
            let mut h = Workbench::open(&database, Some(package))?;
            for item in &originals {
                assert_original(&h, item)?;
            }
            for n in 0..CREATES {
                let saved = h.create(&format!("shared-create-{n}"), draft(n))?;
                assert_eq!(
                    saved.revision, 1,
                    "historical retry returns its original result"
                );
                assert!(!saved.idea.favorite);
            }
            assert_eq!(h.read(&draft(0).id)?.revision, 2);
            assert!(h.read(&draft(0).id)?.idea.favorite);
            let mut conflict = draft(0);
            conflict.title = "a different user intent".into();
            assert!(h.create("shared-create-0", conflict).is_err());
            for item in &originals {
                assert_original(&h, item)?;
            }
            h.finish()?;
        }
        let (after, signed_after, file_bytes) = stats(&database)?;
        assert_eq!(
            before, after,
            "historical retry must not create recipes, chunks or operations"
        );
        assert_eq!(
            signed_before, signed_after,
            "original signed commit containers remain byte-identical"
        );
        let legacy_payload_bytes: usize =
            originals.iter().map(|o| o.evidence.container().len()).sum();
        let physical_payload_bytes = after.recipe_bytes + after.chunk_bytes;
        assert!(physical_payload_bytes < u64::try_from(legacy_payload_bytes)?);
        println!(
            "COUNTS creates={CREATES} edits=1 recipes={} unique_chunks={} chunk_references={} operations={} pending={}",
            after.recipes, after.chunks, after.chunk_references, after.operations, after.pending
        );
        println!(
            "PAYLOADS format7_equivalent_original_containers={} recipe_pb_lz4={} chunk_pb_lz4={} physical_total={} saved_percent={:.2}",
            legacy_payload_bytes,
            after.recipe_bytes,
            after.chunk_bytes,
            physical_payload_bytes,
            100.0 * (1.0 - physical_payload_bytes as f64 / legacy_payload_bytes as f64)
        );
        println!(
            "SQLITE actual_file_bytes={} initial_closed_file_bytes={} (whole_database_including_content_indexes_and_signed_segments)",
            file_bytes, initial_file_bytes
        );
        // Remove the actual source DB, package catalog and generated test key before replay.
        temporary.close()?;
        for item in &originals {
            let result = replay::replay(&item.evidence, Limits::default())?;
            assert!(result.matches);
            assert_eq!(result.report.execution.host_calls, 0);
        }
        println!(
            "PASS_SCOPED actual default Rust Workbench: 30 creates + 1 edit; exact raw/container/digest recovery; audited reopen; historical retries unchanged; all 31 observations replay after source host/database/catalog deletion."
        );
        println!(
            "LIMITS: measured temporary Windows database; format7 comparison is exact logical container payload, not a fabricated old DB; no host projection reconstruction or user-library migration claim."
        );
        Ok(())
    }
}
#[cfg(target_os = "windows")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    windows::run()
}
#[cfg(not(target_os = "windows"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    Err("Unsupported: protected Workbench qualification requires Windows".into())
}
