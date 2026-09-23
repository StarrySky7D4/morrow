//! Offline, copy-first migration of a protected native Morrow content library.
#[cfg(not(target_os = "windows"))]
fn main() {
    eprintln!("This migration tool currently requires Windows protected libraries.");
    std::process::exit(2);
}

#[cfg(target_os = "windows")]
mod windows {
    use morrow_audit::{keys::Key, session::key_path};
    use morrow_core::{
        plugin_package::catalog,
        store::{SCHEMA_VERSION, Store},
    };
    use morrow_workbench_host::{Workbench, versioned_record};
    use prost::Message;
    use rusqlite::{Connection, OpenFlags};
    use sha2::{Digest, Sha256};
    use std::collections::BTreeMap;
    use std::{
        fs::{self, File},
        io::{self},
        path::{Path, PathBuf},
        time::Duration,
    };

    type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
    const APPLICATION_ID: i64 = 0x4d4f5252;
    mod legacy_json {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/bin/migrate_library/legacy_json.rs"
        ));
    }

    #[derive(Default)]
    struct Args {
        source: Option<PathBuf>,
        package: Option<PathBuf>,
        output: Option<PathBuf>,
        apply: bool,
    }
    fn usage() -> &'static str {
        "用法: morrow-migrate-library --source <旧库目录、workbench.db 或 shared_preferences.json> [--package <workbench.morrowplugin>] [--apply --output <不存在的新目录>]\n默认仅只读预检；--apply 在新目录创建完整副本并逐卡迁移 V1 TaskId。请先关闭正在使用该库的工作台。"
    }
    fn arguments() -> Result<Args> {
        let mut result = Args::default();
        let mut args = std::env::args_os().skip(1);
        while let Some(arg) = args.next() {
            match arg.to_str().ok_or("invalid argument")? {
                "--source" => {
                    result.source =
                        Some(PathBuf::from(args.next().ok_or("missing --source value")?))
                }
                "--package" => {
                    result.package =
                        Some(PathBuf::from(args.next().ok_or("missing --package value")?))
                }
                "--output" => {
                    result.output =
                        Some(PathBuf::from(args.next().ok_or("missing --output value")?))
                }
                "--apply" => result.apply = true,
                "--help" | "-h" => {
                    println!("{}", usage());
                    std::process::exit(0);
                }
                _ => return Err(format!("unknown argument: {arg:?}\n{}", usage()).into()),
            }
        }
        if result.apply && result.package.is_none() {
            let bundled = std::env::current_exe()?
                .parent()
                .ok_or("executable directory unavailable")?
                .join("plugins")
                .join("workbench.morrowplugin");
            if bundled.is_file() {
                result.package = Some(bundled);
            }
        }
        if result.source.is_none()
            || (result.apply && (result.output.is_none() || result.package.is_none()))
        {
            return Err(usage().into());
        }
        if !result.apply && result.output.is_some() {
            return Err("--output requires --apply".into());
        }
        Ok(result)
    }
    fn reject_link(path: &Path) -> Result<()> {
        if fs::symlink_metadata(path)?.file_type().is_symlink() {
            return Err(format!("symbolic link is not accepted: {}", path.display()).into());
        }
        Ok(())
    }
    fn database(source: &Path) -> Result<PathBuf> {
        reject_link(source)?;
        let path = if source.is_dir() {
            source.join("workbench.db")
        } else {
            source.to_path_buf()
        };
        reject_link(&path)?;
        if !path.is_file() {
            return Err("source is not a workbench.db file".into());
        }
        Ok(path.canonicalize()?)
    }
    fn open_read_only(path: &Path) -> Result<Connection> {
        let db = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        db.busy_timeout(Duration::from_secs(5))?;
        db.execute_batch("PRAGMA query_only=ON;")?;
        Ok(db)
    }
    // Only inspect durable liveness flags here. Unknown editor metadata fails closed.
    #[derive(Message)]
    struct ActiveSlot {
        #[prost(uint32, tag = "1")]
        schema_version: u32,
        #[prost(bool, tag = "4")]
        active: bool,
    }
    #[derive(Message)]
    struct RecoverySlot {
        #[prost(uint32, tag = "1")]
        schema_version: u32,
        #[prost(bool, tag = "8")]
        active: bool,
    }
    #[derive(Message)]
    struct StagedEntry {
        #[prost(uint32, tag = "5")]
        phase: u32,
    }
    #[derive(Message)]
    struct StagingSlot {
        #[prost(uint32, tag = "1")]
        schema_version: u32,
        #[prost(message, repeated, tag = "5")]
        entries: Vec<StagedEntry>,
    }
    #[derive(Message)]
    struct PreferencesProposalSlot {
        #[prost(uint32, tag = "1")]
        schema_version: u32,
        #[prost(bool, tag = "6")]
        active: bool,
    }
    #[derive(Message)]
    struct ProposalSlot {
        #[prost(uint32, tag = "1")]
        schema_version: u32,
        #[prost(uint64, tag = "5")]
        revision: u64,
        #[prost(bool, tag = "6")]
        cancelled: bool,
    }
    fn pending_editor(kind: &str, body: &[u8]) -> Result<bool> {
        if kind == "org.morrow.host.preferences-proposal" {
            if body.len() > 4 * 1024 * 1024 + 1024 {
                return Err("preferences proposal exceeds preflight limit".into());
            }
            let slot = PreferencesProposalSlot::decode(body)?;
            if slot.schema_version != 1 {
                return Err("unsupported preferences proposal metadata".into());
            }
            return Ok(slot.active);
        }
        if !kind.starts_with("org.morrow.host.editor-") {
            return Ok(false);
        }
        if body.len() > 1024 * 1024 {
            return Err("editor metadata exceeds preflight limit".into());
        }
        match kind {
            "org.morrow.host.editor-recovery" => {
                let slot = RecoverySlot::decode(body)?;
                if slot.schema_version != 1 {
                    return Err("unsupported editor recovery metadata".into());
                }
                Ok(slot.active)
            }
            "org.morrow.host.editor-draft" => {
                let slot = ActiveSlot::decode(body)?;
                if slot.schema_version != 1 {
                    return Err("unsupported editor draft metadata".into());
                }
                Ok(slot.active)
            }
            "org.morrow.host.editor-draft-imports" => {
                let slot = StagingSlot::decode(body)?;
                if slot.schema_version != 1 || slot.entries.iter().any(|entry| entry.phase > 2) {
                    return Err("unsupported editor import metadata".into());
                }
                Ok(slot.entries.iter().any(|entry| entry.phase != 2))
            }
            "org.morrow.host.editor-draft-handoff-proposal"
            | "org.morrow.host.editor-draft-import-decision" => {
                let slot = ProposalSlot::decode(body)?;
                if slot.schema_version != 1
                    || !matches!(slot.revision, 1 | 2)
                    || slot.cancelled != (slot.revision == 2)
                {
                    return Err("unsupported editor proposal metadata".into());
                }
                Ok(slot.revision == 1)
            }
            _ if kind.starts_with("org.morrow.host.editor-") => Ok(true),
            _ => Ok(false),
        }
    }
    struct Inventory {
        schema: i64,
        ids: Vec<(String, u64)>,
        v1: usize,
        v2: usize,
        other: usize,
        records: BTreeMap<
            String,
            (
                [u8; 32],
                String,
                u32,
                u64,
                Vec<morrow_core::content::Attachment>,
            ),
        >,
        pending: Vec<String>,
        v1_sources: BTreeMap<String, (String, Vec<u8>)>,
        operations: BTreeMap<String, [u8; 32]>,
        other_types: BTreeMap<String, usize>,
    }
    fn inventory(db: &Connection) -> Result<Inventory> {
        let app: i64 = db.query_row("PRAGMA application_id", [], |r| r.get(0))?;
        let schema: i64 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if app != APPLICATION_ID || !(5..=SCHEMA_VERSION).contains(&schema) {
            return Err(format!("unsupported library format: application_id={app}, schema={schema}; expected MORR schema 5..={SCHEMA_VERSION}").into());
        }
        let check: String = db.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
        if check != "ok" {
            return Err(format!("SQLite integrity check failed: {check}").into());
        }
        let mut out = Inventory {
            schema,
            ids: Vec::new(),
            v1: 0,
            v2: 0,
            other: 0,
            records: BTreeMap::new(),
            pending: Vec::new(),
            v1_sources: BTreeMap::new(),
            operations: BTreeMap::new(),
            other_types: BTreeMap::new(),
        };
        let mut query = db.prepare("SELECT id,payload FROM cards ORDER BY id")?;
        let mut rows = query.query([])?;
        while let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            let raw: Vec<u8> = row.get(1)?;
            let card = morrow_core::envelope::decode(&raw)?;
            let summary = card.summary();
            if summary.id != id {
                return Err(format!("card identity mismatch: {id}").into());
            }
            if pending_editor(&summary.type_id, &card.body())? {
                out.pending.push(format!("{} ({})", id, summary.type_id));
            }
            out.records.insert(
                id.clone(),
                (
                    Sha256::digest(&raw).into(),
                    summary.type_id.clone(),
                    summary.format_version,
                    summary.revision,
                    card.attachments(),
                ),
            );
            match (summary.type_id.as_str(), summary.format_version) {
                ("org.morrow.idea", 1) => {
                    versioned_record::decode(&card)?;
                    out.v1 += 1;
                    out.v1_sources
                        .insert(id.clone(), (summary.title, card.body()));
                    out.ids.push((id, summary.revision));
                }
                ("org.morrow.idea", 2) => {
                    versioned_record::decode(&card)?;
                    out.v2 += 1;
                }
                ("org.morrow.idea", future) if future > 2 => {
                    return Err(format!(
                        "future idea format {future} in card {id}; refusing downgrade"
                    )
                    .into());
                }
                _ => {
                    out.other += 1;
                    *out.other_types.entry(summary.type_id).or_insert(0) += 1;
                }
            }
        }
        let mut operations = db.prepare("SELECT id,payload FROM operations ORDER BY id")?;
        let mut rows = operations.query([])?;
        while let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            let payload: Vec<u8> = row.get(1)?;
            out.operations.insert(id, Sha256::digest(payload).into());
        }
        Ok(out)
    }
    fn incomplete(output: &Path) -> Result<PathBuf> {
        let path = output.join("MIGRATION_INCOMPLETE.txt");
        let mut file = File::options().write(true).create_new(true).open(&path)?;
        io::Write::write_all(&mut file, b"Migration is incomplete. Do not open this target as a library. Keep it for inspection and retry into a new directory.
")?;
        file.sync_all()?;
        Ok(path)
    }
    fn report(output: &Path, value: &serde_json::Value) -> Result<()> {
        let path = output.join("migration-report.json");
        let mut file = File::options().write(true).create_new(true).open(path)?;
        io::Write::write_all(&mut file, &serde_json::to_vec_pretty(value)?)?;
        file.sync_all()?;
        Ok(())
    }
    fn copy_key(source: &Path, target: &Path) -> Result<()> {
        reject_link(source)?;
        Key::load(source)?;
        let mut reader = File::open(source)?;
        let mut writer = File::options().write(true).create_new(true).open(target)?;
        io::copy(&mut reader, &mut writer)?;
        writer.sync_all()?;
        Key::load(target)?;
        Ok(())
    }
    fn fingerprint(path: &Path) -> Result<([u8; 32], u64)> {
        fingerprint_bounded(path, u64::MAX)
    }
    fn fingerprint_bounded(path: &Path, max: u64) -> Result<([u8; 32], u64)> {
        let mut source = File::open(path)?;
        let size = source.metadata()?.len();
        if size > max {
            return Err("local media exceeds limit".into());
        }
        let mut hash = Sha256::new();
        let mut buffer = [0u8; 65536];
        let mut read = 0u64;
        loop {
            let amount = io::Read::read(&mut source, &mut buffer)?;
            if amount == 0 {
                break;
            }
            read = read
                .checked_add(amount as u64)
                .ok_or("source size overflow")?;
            if read > max {
                return Err("local media grew beyond limit".into());
            }
            hash.update(&buffer[..amount]);
        }
        if read != size {
            return Err("source changed during copy".into());
        }
        Ok((hash.finalize().into(), size))
    }
    fn frozen_source(
        source: &Path,
        key: &Path,
        temp: &Path,
    ) -> Result<(PathBuf, BTreeMap<PathBuf, ([u8; 32], u64)>)> {
        let mut originals = BTreeMap::new();
        let mut selected = vec![source.to_path_buf(), key.to_path_buf()];
        for suffix in ["-wal", "-shm"] {
            let path = source.with_file_name(format!(
                "{}{}",
                source.file_name().unwrap().to_string_lossy(),
                suffix
            ));
            if path.exists() {
                selected.push(path);
            }
        }
        for path in &selected {
            reject_link(path)?;
            let before = fingerprint(path)?;
            let target = temp.join(path.file_name().ok_or("source filename missing")?);
            fs::copy(path, &target)?;
            if fingerprint(path)? != before || fingerprint(&target)? != before {
                return Err("source changed while preparing read-only copy".into());
            }
            originals.insert(path.clone(), before);
        }
        Ok((temp.join(source.file_name().unwrap()), originals))
    }
    fn verify_source_files(
        source: &Path,
        originals: &BTreeMap<PathBuf, ([u8; 32], u64)>,
    ) -> Result<()> {
        for suffix in ["-wal", "-shm"] {
            let path = source.with_file_name(format!(
                "{}{}",
                source.file_name().unwrap().to_string_lossy(),
                suffix
            ));
            if path.exists() != originals.contains_key(&path) {
                return Err(
                    format!("source SQLite sidecar set changed: {}", path.display()).into(),
                );
            }
        }
        for (path, expected) in originals {
            if fingerprint(path)? != *expected {
                return Err(format!("source changed: {}", path.display()).into());
            }
        }
        Ok(())
    }
    fn copy_external(path: &Path, output: &Path, max: u64) -> Result<PathBuf> {
        reject_link(path)?;
        if !path.is_absolute() || !path.is_file() {
            return Err(format!("missing local media: {}", path.display()).into());
        }
        let (digest, size) = fingerprint_bounded(path, max)?;
        let suffix = path.extension().and_then(|v| v.to_str()).unwrap_or("bin");
        let suffix = if suffix.len() <= 12 && suffix.bytes().all(|b| b.is_ascii_alphanumeric()) {
            suffix.to_ascii_lowercase()
        } else {
            "bin".into()
        };
        let directory = output.join("legacy-media");
        fs::create_dir_all(&directory)?;
        let target = directory.join(format!(
            "{}.{}",
            digest
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>(),
            suffix
        ));
        if !target.exists() {
            fs::copy(path, &target)?;
        }
        if fingerprint(path)? != (digest, size) || fingerprint(&target)? != (digest, size) {
            return Err(format!("local media changed during copy: {}", path.display()).into());
        }
        Ok(target)
    }
    fn rebase_source(
        source: &mut morrow_workbench_plugin::preferences::Source,
        output: &Path,
    ) -> Result<bool> {
        if !source.local {
            return Ok(false);
        }
        let old = url::Url::parse(&source.location)?
            .to_file_path()
            .map_err(|_| "invalid local media URI")?;
        let target = copy_external(&old, output, 1024 * 1024 * 1024)?;
        source.location = url::Url::from_file_path(target)
            .map_err(|_| "invalid copied media path")?
            .to_string();
        Ok(true)
    }
    fn copy_native_externals(host: &mut Workbench, source: &Path, output: &Path) -> Result<bool> {
        let (font, _) = host.read_ui_font()?;
        if !font.asset.is_empty() {
            let name = format!("{}.sfnt", font.asset);
            let parents = [
                source.parent().unwrap(),
                source
                    .parent()
                    .unwrap()
                    .parent()
                    .unwrap_or(source.parent().unwrap()),
            ];
            let candidate = parents
                .iter()
                .map(|p| p.join("fonts").join(&name))
                .find(|p| p.is_file())
                .ok_or("font asset referenced by library is missing")?;
            let (digest, size) = fingerprint(&candidate)?;
            if size < 12
                || size > 20 * 1024 * 1024
                || digest
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>()
                    != font.asset
            {
                return Err("font asset digest mismatch".into());
            }
            let target_dir = output.join("fonts");
            fs::create_dir_all(&target_dir)?;
            let target = target_dir.join(name);
            if !target.exists() {
                fs::copy(&candidate, &target)?;
            }
            if fingerprint(&candidate)? != (digest, size) || fingerprint(&target)? != (digest, size)
            {
                return Err("font copy changed".into());
            }
        }
        let Some(raw) = host.read_preferences()? else {
            return Ok(false);
        };
        let mut value = morrow_workbench_plugin::preferences::decode_wire(&raw)?;
        let mut changed = false;
        if let Some(source) = &mut value.texture {
            changed |= rebase_source(source, output)?;
        }
        for track in &mut value.tracks {
            if let Some(source) = &mut track.source {
                changed |= rebase_source(source, output)?;
            }
            if let Some(source) = &mut track.cover {
                changed |= rebase_source(source, output)?;
            }
        }
        if changed {
            if host.pending_preferences()?.is_some() {
                return Err(
                    "pending preferences proposal must be reconciled before media rebase".into(),
                );
            }
            let encoded = morrow_workbench_plugin::preferences::encode_wire(&value)?;
            let operation = format!(
                "library-migration-media-rebase-{:x}",
                Sha256::digest(&encoded)
            );
            host.save_preferences(&operation, encoded.clone())?;
            if host.read_preferences()?.as_deref() != Some(encoded.as_slice()) {
                return Err("copied media preferences did not read back".into());
            }
        }
        Ok(changed)
    }
    pub fn run() -> Result<()> {
        let args = arguments()?;
        let requested = args.source.as_ref().unwrap();
        let json = if requested.is_dir()
            && !requested.join("workbench.db").exists()
            && requested.join("shared_preferences.json").is_file()
        {
            Some(requested.join("shared_preferences.json"))
        } else if requested.is_file()
            && requested
                .extension()
                .and_then(|v| v.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
        {
            Some(requested.to_path_buf())
        } else {
            None
        };
        if let Some(json) = json {
            return legacy_json::run(&args, &json);
        }
        let source = database(requested)?;
        if source
            .parent()
            .ok_or("source directory missing")?
            .join("MIGRATION_INCOMPLETE.txt")
            .exists()
        {
            return Err("source is an incomplete migration target; keep it for inspection and use the original library".into());
        }
        let key = key_path(&source)?;
        reject_link(&key)?;
        let lease_path = source.with_file_name(format!(
            "{}.audit-lock",
            source.file_name().unwrap().to_string_lossy()
        ));
        reject_link(&lease_path)?;
        let lease = File::options().read(true).write(true).open(&lease_path)
            .map_err(|_| "source audit lock is missing or inaccessible; close the application and verify the library")?;
        lease
            .try_lock()
            .map_err(|_| "source library is active; close its workbench before migration")?;
        let trust = Key::load(&key)?.trust();
        let temporary = tempfile::tempdir()?;
        let (frozen, source_files) = frozen_source(&source, &key, temporary.path())?;
        let audited = Store::open_read_only_audited(&frozen, trust.clone())?;
        let db = open_read_only(&frozen)?;
        let before = inventory(&db)?;
        verify_source_files(&source, &source_files)?;
        println!(
            "源库: {}\nSQLite schema: {}\nV1 任务卡: {}；V2 任务卡: {}；其他记录: {}",
            source.display(),
            before.schema,
            before.v1,
            before.v2,
            before.other
        );
        if !before.pending.is_empty() {
            println!(
                "待决编辑或设置记录（阻止自动迁移）: {}",
                before.pending.join(", ")
            );
        }
        if !args.apply {
            println!("只读预检完成。提供 --apply 与 --output（或另选 --package） 才会创建新库。");
            return Ok(());
        }
        if !before.pending.is_empty() {
            return Err("pending editor/import/preferences evidence must be resolved before automatic card migration".into());
        }
        let package = catalog::read_file(args.package.as_ref().unwrap())?;
        if package.manifest().package_id != "org.morrow.workbench" {
            return Err("selected package is not org.morrow.workbench".into());
        }
        let output = args.output.as_ref().unwrap();
        if output.exists() {
            return Err("target directory already exists; choose a new path".into());
        }
        let parent = output
            .parent()
            .filter(|v| !v.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        if !parent.is_dir() {
            return Err("target parent directory does not exist".into());
        }
        fs::create_dir(output)?;
        let output = output.canonicalize()?;
        let marker = incomplete(&output)?;
        let target = output.join("workbench.db");
        let result = (|| -> Result<()> {
            audited.snapshot_to(&target, 8 * 1024 * 1024 * 1024)?;
            drop(audited);
            drop(Store::open_read_only_audited(&target, trust.clone())?);
            copy_key(&key, &key_path(&target)?)?;
            verify_source_files(&source, &source_files)?;
            let copied = inventory(&open_read_only(&target)?)?;
            if copied.records != before.records
                || copied.schema != before.schema
                || copied.operations != before.operations
            {
                return Err("snapshot differs from locked source inventory".into());
            }
            drop(db);
            let mut host = Workbench::open(&target, Some(package))?;
            let rebased_preferences = copy_native_externals(&mut host, &source, &output)?;
            let mut migrated = 0;
            for (id, revision) in &before.ids {
                let plan = host.plan_tasks_migration(id)?;
                if plan.source_revision != *revision {
                    return Err(format!("source revision changed in snapshot: {id}").into());
                }
                host.migrate_tasks(&plan.operation, id, *revision)?;
                migrated += 1;
                println!("已迁移 {migrated}/{}: {id}", before.v1);
            }
            host.finish()?;
            let after = open_read_only(&target)?;
            let observed = inventory(&after)?;
            if observed.v1 != 0
                || observed.v2 != before.v1 + before.v2
                || observed.other != before.other
                || observed.records.len() != before.records.len()
            {
                return Err("target inventory does not match source and migrated cards".into());
            }
            for (id, (digest, kind, format, revision, attachments)) in &before.records {
                let Some((
                    after_digest,
                    after_kind,
                    after_format,
                    after_revision,
                    after_attachments,
                )) = observed.records.get(id)
                else {
                    return Err(format!("target card missing: {id}").into());
                };
                let preference_rebased = rebased_preferences
                    && id == "morrow-studio-preferences"
                    && kind == "org.morrow.studio"
                    && *format == 1;
                if kind != after_kind
                    || attachments != after_attachments
                    || (preference_rebased
                        && (*after_format != 1 || *after_revision != revision + 1))
                    || (kind == "org.morrow.idea"
                        && *format == 1
                        && (*after_format != 2 || *after_revision != revision + 1))
                    || !preference_rebased
                        && (kind != "org.morrow.idea" || *format != 1)
                        && (digest != after_digest
                            || format != after_format
                            || revision != after_revision)
                {
                    return Err(format!("target card comparison failed: {id}").into());
                }
            }
            for (id, digest) in &before.operations {
                if observed.operations.get(id) != Some(digest) {
                    return Err(format!("historical operation changed: {id}").into());
                }
            }
            drop(after);
            let verified = Store::open_read_only_audited(&target, trust)?;
            for (id, (title, body)) in &before.v1_sources {
                let card = verified.card(id)?.ok_or("migrated card missing")?;
                let result = morrow_workbench_plugin::tasks_v2::decode(id, title, &card.body())?;
                let origin = result.origin.ok_or("migrated card has no source origin")?;
                if origin.original_properties != *body
                    || origin.original_title != *title
                    || origin.source_sha256 != Sha256::digest(body).as_slice()
                {
                    return Err(format!("migrated origin differs from source: {id}").into());
                }
            }
            verify_source_files(&source, &source_files)?;
            report(
                &output,
                &serde_json::json!({
                    "source_format": "protected-sqlite",
                    "source_schema": before.schema,
                    "source": source,
                    "v1_migrated": before.v1,
                    "v2_preserved": before.v2,
                    "other_types_preserved": before.other_types,
                    "historical_operations_preserved": before.operations.len(),
                    "source_files_unchanged": true,
                    "target_audit_verified": true
                }),
            )?;
            fs::remove_file(&marker)?;
            println!(
                "完成: {}；V2 任务卡: {}；其他记录保留: {}",
                output.display(),
                observed.v2,
                observed.other
            );
            Ok(())
        })();
        if result.is_err() {
            eprintln!(
                "目标副本保留在 {}。原库未被替换；请检查目标后选择全新目录重试。",
                output.display()
            );
        }
        result
    }
}
#[cfg(target_os = "windows")]
fn main() {
    if let Err(error) = windows::run() {
        eprintln!("迁移失败: {error}");
        std::process::exit(1);
    }
}
