use super::{Args, Result};
use morrow_core::plugin_package::catalog;
use morrow_workbench_host::Workbench;
use morrow_workbench_plugin::Idea;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
};

const MAX_JSON: u64 = 16 * 1024 * 1024;
const MAX_MEDIA: u64 = 1024 * 1024 * 1024;

fn known_keys(value: &Value, allowed: &[&str], where_: &str) -> Result<()> {
    let map = value
        .as_object()
        .ok_or_else(|| format!("{where_} is not an object"))?;
    for key in map.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(format!("unmapped {where_} field: {key}").into());
        }
    }
    Ok(())
}
fn field<'a>(value: &'a Value, name: &str) -> Result<&'a Value> {
    value
        .get(name)
        .ok_or_else(|| format!("missing field {name}").into())
}
fn string(value: &Value, name: &str) -> Result<String> {
    Ok(field(value, name)?
        .as_str()
        .ok_or_else(|| format!("{name} is not a string"))?
        .into())
}
fn optional_string(value: &Value, name: &str, fallback: &str) -> Result<String> {
    match value.get(name) {
        Some(v) => Ok(v
            .as_str()
            .ok_or_else(|| format!("{name} is not a string"))?
            .into()),
        None => Ok(fallback.into()),
    }
}
fn boolean(value: &Value, name: &str) -> Result<bool> {
    field(value, name)?
        .as_bool()
        .ok_or_else(|| format!("{name} is not a bool").into())
}
fn strings(value: &Value, name: &str) -> Result<Vec<String>> {
    field(value, name)?
        .as_array()
        .ok_or_else(|| format!("{name} is not a list"))?
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("{name} contains a non-string").into())
        })
        .collect()
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn sha_file(path: &Path, max: u64) -> Result<([u8; 32], u64)> {
    if !path.is_absolute()
        || fs::symlink_metadata(path)?.file_type().is_symlink()
        || !path.is_file()
    {
        return Err(format!("unsafe or missing local media: {}", path.display()).into());
    }
    let mut file = File::open(path)?;
    let size = file.metadata()?.len();
    if size > max {
        return Err(format!("media exceeds limit: {}", path.display()).into());
    }
    let mut hash = Sha256::new();
    let mut read = 0u64;
    let mut buffer = [0u8; 65536];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        read = read.checked_add(n as u64).ok_or("media size overflow")?;
        if read > max {
            return Err("media grew past limit while reading".into());
        }
        hash.update(&buffer[..n]);
    }
    if read != size {
        return Err("media changed while reading".into());
    }
    Ok((hash.finalize().into(), size))
}
fn source_path(source: &Value) -> Result<Option<PathBuf>> {
    known_keys(
        source,
        &["location", "name", "kind", "local"],
        "media source",
    )?;
    let local = boolean(source, "local")?;
    let location = string(source, "location")?;
    let _name = string(source, "name")?;
    let kind = string(source, "kind")?;
    if !matches!(kind.as_str(), "image" | "gif" | "video" | "audio" | "file") {
        return Err(format!("unsupported media kind: {kind}").into());
    }
    if local {
        Ok(Some(PathBuf::from(location)))
    } else {
        Ok(None)
    }
}
fn collect_source(
    source: &Value,
    paths: &mut BTreeMap<PathBuf, ([u8; 32], u64)>,
    max: u64,
) -> Result<()> {
    if let Some(path) = source_path(source)? {
        let found = sha_file(&path, max)?;
        paths.insert(path, found);
    }
    Ok(())
}
fn asset_size(value: &Value) -> Result<u64> {
    let size = match value.get("exactSize") {
        Some(v) => {
            let raw = v.as_str().ok_or("invalid exactSize")?;
            if raw.is_empty() || raw.len() > 20 || !raw.bytes().all(|b| b.is_ascii_digit()) {
                return Err("invalid exactSize".into());
            }
            raw.parse::<u64>()?
        }
        None => value.get("size").and_then(Value::as_u64).unwrap_or(0),
    };
    if let Some(v) = value.get("size") {
        if let Some(legacy) = v.as_u64() {
            if legacy != size {
                return Err("conflicting attachment sizes".into());
            }
        } else {
            return Err("invalid attachment size".into());
        }
    }
    Ok(size)
}
struct Legacy {
    snapshot: Value,
    raw: Vec<u8>,
    ideas: Vec<Value>,
    paths: BTreeMap<PathBuf, ([u8; 32], u64)>,
    unknown: Vec<String>,
    font: Option<PathBuf>,
}
fn inspect(path: &Path) -> Result<Legacy> {
    if fs::symlink_metadata(path)?.file_type().is_symlink() || !path.is_file() {
        return Err("legacy source must be a regular shared_preferences.json file".into());
    }
    if fs::metadata(path)?.len() > MAX_JSON {
        return Err("legacy JSON exceeds limit".into());
    }
    let raw = fs::read(path)?;
    let outer: Value = serde_json::from_slice(&raw)?;
    let snapshot: Value = match outer
        .get("flutter.daemon.studio.v1")
        .or_else(|| outer.get("daemon.studio.v1"))
    {
        Some(Value::String(encoded)) => serde_json::from_str(encoded)?,
        Some(_) => return Err("legacy storage entry is not JSON text".into()),
        None if outer.get("version").is_some() && outer.get("ideas").is_some() => outer,
        None => return Err("unrecognized legacy JSON snapshot".into()),
    };
    if snapshot.get("version").and_then(Value::as_u64) != Some(1) {
        return Err("unsupported legacy JSON version".into());
    }
    let ideas = field(&snapshot, "ideas")?
        .as_array()
        .ok_or("ideas is not a list")?
        .clone();
    let mut paths = BTreeMap::new();
    let mut ids = BTreeSet::new();
    for idea in &ideas {
        known_keys(
            idea,
            &[
                "id",
                "title",
                "description",
                "category",
                "time",
                "icon",
                "color",
                "favorite",
                "todos",
                "completed",
                "stage",
                "hypothesis",
                "conclusion",
                "attachments",
            ],
            "idea",
        )?;
        let id = string(idea, "id")?;
        if !ids.insert(id) {
            return Err("duplicate legacy idea ID".into());
        }
        let _ = string(idea, "title")?;
        let _ = string(idea, "description")?;
        let _ = string(idea, "category")?;
        let _ = string(idea, "time")?;
        let _ = boolean(idea, "favorite")?;
        let _ = strings(idea, "todos")?;
        let _ = strings(idea, "completed")?;
        let attachments = field(idea, "attachments")?
            .as_array()
            .ok_or("attachments is not a list")?;
        for attachment in attachments {
            known_keys(
                attachment,
                &["source", "size", "exactSize", "pluginId"],
                "attachment",
            )?;
            let source = field(attachment, "source")?;
            let size = asset_size(attachment)?;
            if source_path(source)?.is_none() {
                return Err("remote legacy attachment has no local bytes; retain source and resolve before migration".into());
            }
            collect_source(source, &mut paths, 200 * 1024 * 1024)?;
            let path = PathBuf::from(string(source, "location")?);
            if paths.get(&path).unwrap().1 != size {
                return Err(
                    format!("attachment size differs from source: {}", path.display()).into(),
                );
            }
        }
    }
    if let Some(texture) = snapshot.get("texture").filter(|v| !v.is_null()) {
        collect_source(texture, &mut paths, MAX_MEDIA)?;
    }
    if let Some(music) = snapshot.get("music") {
        if let Some(tracks) = music.get("tracks").and_then(Value::as_array) {
            for track in tracks {
                collect_source(field(track, "source")?, &mut paths, MAX_MEDIA)?;
                if let Some(cover) = track.get("cover").filter(|v| !v.is_null()) {
                    collect_source(cover, &mut paths, MAX_MEDIA)?;
                }
            }
        }
    }
    let mut font_path = None;
    if let Some(asset) = snapshot
        .get("uiFont")
        .and_then(|v| v.get("asset"))
        .and_then(Value::as_str)
    {
        if !asset.is_empty() {
            if asset.len() != 64
                || !asset
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
            {
                return Err("invalid font asset digest".into());
            }
            let font = path
                .parent()
                .ok_or("missing preferences parent")?
                .join("fonts")
                .join(format!("{asset}.sfnt"));
            let (digest, size) = sha_file(&font, 20 * 1024 * 1024)?;
            if size < 12 || hex(&digest) != asset {
                return Err("font asset digest or size mismatch".into());
            }
            paths.insert(font.clone(), (digest, size));
            font_path = Some(font);
        }
    }
    let known: BTreeSet<&str> = [
        "version",
        "ideas",
        "completed",
        "music",
        "texture",
        "uiLocale",
        "uiFont",
        "theme",
        "glass",
        "background",
        "visualStyle",
        "solidTint",
        "frostedOpacity",
        "customColor",
        "themeColor",
        "mediaPlaying",
        "liquidCanvas",
        "componentMaterials",
        "themeLightness",
        "windowRadius",
        "cornerRadius",
        "grayscale",
        "rounded",
        "sidebarExpanded",
        "appearanceExpanded",
        "canvasBlur",
        "canvasOpacity",
        "canvasColor",
        "componentCustom",
        "componentBlur",
        "componentOpacity",
        "componentColor",
    ]
    .into_iter()
    .collect();
    let unknown = snapshot
        .as_object()
        .ok_or("legacy snapshot is not an object")?
        .keys()
        .filter(|key| !known.contains(key.as_str()))
        .cloned()
        .collect();
    Ok(Legacy {
        snapshot,
        raw,
        ideas,
        paths,
        unknown,
        font: font_path,
    })
}
fn copied_path(output: &Path, source: &Path, digest: &[u8; 32]) -> PathBuf {
    let suffix = source.extension().and_then(|v| v.to_str()).unwrap_or("bin");
    let safe_suffix = if suffix.len() <= 12 && suffix.bytes().all(|b| b.is_ascii_alphanumeric()) {
        suffix.to_ascii_lowercase()
    } else {
        "bin".into()
    };
    output
        .join("legacy-media")
        .join(format!("{}.{}", hex(digest), safe_suffix))
}
fn copy_media(legacy: &Legacy, output: &Path) -> Result<BTreeMap<PathBuf, PathBuf>> {
    fs::create_dir(output.join("legacy-media"))?;
    let mut mapped = BTreeMap::new();
    for (original, (digest, size)) in &legacy.paths {
        if legacy.font.as_ref() == Some(original) {
            let id = hex(digest);
            let fonts = output.join("fonts");
            fs::create_dir_all(&fonts)?;
            let target = fonts.join(format!("{id}.sfnt"));
            if !target.exists() {
                fs::copy(original, &target)?;
            }
            if sha_file(&target, 20 * 1024 * 1024)? != (*digest, *size) {
                return Err("copied font differs from source".into());
            }
            mapped.insert(original.clone(), target);
            continue;
        }
        let target = copied_path(output, original, digest);
        if !target.exists() {
            fs::copy(original, &target)?;
        }
        if sha_file(original, MAX_MEDIA)? != (*digest, *size)
            || sha_file(&target, MAX_MEDIA)? != (*digest, *size)
        {
            return Err(format!("copied media differs from source: {}", original.display()).into());
        }
        mapped.insert(original.clone(), target);
    }
    Ok(mapped)
}
fn replace_source(value: &mut Value, mapped: &BTreeMap<PathBuf, PathBuf>) -> Result<()> {
    if source_path(value)?.is_none() {
        return Ok(());
    }
    let old = PathBuf::from(string(value, "location")?);
    let target = mapped.get(&old).ok_or("copied media path missing")?;
    value["location"] = Value::String(target.to_string_lossy().into_owned());
    Ok(())
}
fn rewrite_media(snapshot: &mut Value, mapped: &BTreeMap<PathBuf, PathBuf>) -> Result<()> {
    if let Some(texture) = snapshot.get_mut("texture").filter(|v| !v.is_null()) {
        replace_source(texture, mapped)?;
    }
    if let Some(tracks) = snapshot
        .get_mut("music")
        .and_then(|v| v.get_mut("tracks"))
        .and_then(Value::as_array_mut)
    {
        for track in tracks {
            replace_source(&mut track["source"], mapped)?;
            if let Some(cover) = track.get_mut("cover").filter(|v| !v.is_null()) {
                replace_source(cover, mapped)?;
            }
        }
    }
    Ok(())
}
fn idea(value: &Value, host: &mut Workbench, mapped: &BTreeMap<PathBuf, PathBuf>) -> Result<Idea> {
    let id = string(value, "id")?;
    let category = string(value, "category")?;
    let stage = optional_string(
        value,
        "stage",
        match category.as_str() {
            "进行中" => "推进中",
            "实验" => "待验证",
            _ => "待整理",
        },
    )?;
    let icon = field(value, "icon")?
        .as_i64()
        .ok_or("invalid icon")?
        .clamp(0, 3) as u16;
    let color = field(value, "color")?
        .as_u64()
        .ok_or("invalid color")?
        .try_into()?;
    let mut draft = Idea {
        id: id.clone(),
        title: string(value, "title")?,
        description: string(value, "description")?,
        category,
        stage,
        hypothesis: optional_string(value, "hypothesis", "")?,
        conclusion: optional_string(value, "conclusion", "")?,
        favorite: boolean(value, "favorite")?,
        todos: strings(value, "todos")?,
        completed: strings(value, "completed")?,
        icon,
        color,
        ..Default::default()
    };
    for attachment in field(value, "attachments")?
        .as_array()
        .ok_or("invalid attachments")?
    {
        let source = field(attachment, "source")?;
        let old = PathBuf::from(string(source, "location")?);
        let target = mapped.get(&old).ok_or("attachment copy missing")?;
        let mut reader = File::open(target)?;
        let asset = host.import(
            &id,
            &string(source, "name")?,
            &string(source, "kind")?,
            &mut reader,
            asset_size(attachment)?,
        )?;
        draft.assets.push(asset);
    }
    draft.validate()?;
    Ok(draft)
}
fn write_raw(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = File::options().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}
pub(super) fn run(args: &Args, source: &Path) -> Result<()> {
    let legacy = inspect(source)?;
    println!(
        "旧 JSON: {}；灵感卡: {}；本地媒体/字体: {}；阻止迁移的未知设置键: {}",
        source.display(),
        legacy.ideas.len(),
        legacy.paths.len(),
        legacy.unknown.join(", ")
    );
    morrow_workbench_host::legacy_json_preferences::validate_preferences(&legacy.snapshot)?;
    if !args.apply {
        println!("只读预检完成。--apply 与 --output 将生成新内容库；原 JSON 不变。");
        return Ok(());
    }
    let package = catalog::read_file(args.package.as_ref().ok_or("--package required")?)?;
    if package.manifest().package_id != "org.morrow.workbench" {
        return Err("selected package is not org.morrow.workbench".into());
    }
    let output = args.output.as_ref().ok_or("--output required")?;
    if output.exists() {
        return Err("target directory already exists".into());
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
    let marker = output.join("MIGRATION_INCOMPLETE.txt");
    write_raw(&marker, b"Migration is incomplete. Do not open this target as a library. Keep it for inspection and retry into a new directory.\n")?;
    let result = (|| -> Result<()> {
        if fs::read(source)? != legacy.raw {
            return Err("legacy JSON changed after preflight".into());
        }
        write_raw(&output.join("legacy-shared_preferences.json"), &legacy.raw)?;
        let mapped = copy_media(&legacy, &output)?;
        let mut snapshot = legacy.snapshot.clone();
        rewrite_media(&mut snapshot, &mapped)?;
        let db = output.join("workbench.db");
        let mut host = Workbench::open(&db, Some(package))?;
        let mut created_bodies = BTreeMap::new();
        for (index, value) in legacy.ideas.iter().enumerate() {
            let draft = idea(value, &mut host, &mapped)?;
            let id = draft.id.clone();
            let created = host.create(&format!("legacy-json-create-{index}"), draft.clone())?;
            if created.idea != draft || created.revision != 1 {
                return Err(format!("legacy idea changed during native create: {id}").into());
            }
            let body = morrow_workbench_plugin::persistence::encode(&created.idea, None)?;
            created_bodies.insert(id.clone(), (created.idea.title.clone(), body));
            let plan = host.plan_tasks_migration(&id)?;
            host.migrate_tasks(&plan.operation, &id, plan.source_revision)?;
            println!("已导入并迁移 {}/{}: {id}", index + 1, legacy.ideas.len());
        }
        morrow_workbench_host::legacy_json_preferences::apply_preferences(&mut host, &snapshot)?;
        morrow_workbench_host::legacy_json_preferences::verify_preferences(&host, &snapshot)?;
        host.finish()?;
        let key = morrow_audit::keys::Key::load(&morrow_audit::session::key_path(&db)?)?;
        let verified = morrow_core::store::Store::open_read_only_audited(&db, key.trust())?;
        for value in &legacy.ideas {
            let id = string(value, "id")?;
            let card = verified.card(&id)?.ok_or("imported card is missing")?;
            if card.summary().type_id != "org.morrow.idea"
                || card.summary().format_version != 2
                || card.summary().revision != 2
                || card.attachments().len()
                    != field(value, "attachments")?
                        .as_array()
                        .ok_or("invalid attachments")?
                        .len()
            {
                return Err(format!("imported card differs from legacy idea: {id}").into());
            }
            morrow_workbench_host::versioned_record::decode(&card)?;
            let (title, body) = created_bodies
                .get(&id)
                .ok_or("created source card missing")?;
            let properties = morrow_workbench_plugin::tasks_v2::decode(&id, title, &card.body())?;
            let origin = properties
                .origin
                .ok_or("migrated JSON card has no V1 origin")?;
            if origin.original_title != *title
                || origin.original_properties != *body
                || origin.source_revision != 1
                || origin.source_sha256 != Sha256::digest(body).as_slice()
            {
                return Err(
                    format!("migrated JSON origin differs from created V1 card: {id}").into(),
                );
            }
        }
        drop(verified);
        for (path, expected) in &legacy.paths {
            if sha_file(path, MAX_MEDIA)? != *expected {
                return Err(
                    format!("legacy media changed during migration: {}", path.display()).into(),
                );
            }
        }
        let manifest = format!(
            "source_sha256={}\nideas={}\nlocal_media={}\nunknown_snapshot_keys={}\noriginal=legacy-shared_preferences.json\nlegacy_time=preserved_only_in_original_json\nlegacy_attachment_pluginId=remapped_to_new_asset_ids_original_preserved\n",
            hex(&Sha256::digest(&legacy.raw)),
            legacy.ideas.len(),
            legacy.paths.len(),
            legacy.unknown.join(",")
        );
        write_raw(&output.join("migration-report.txt"), manifest.as_bytes())?;
        write_raw(
            &output.join("migration-report.json"),
            &serde_json::to_vec_pretty(&serde_json::json!({
                "source_format": "legacy-json-v1",
                "source_sha256": hex(&Sha256::digest(&legacy.raw)),
                "ideas_migrated": legacy.ideas.len(),
                "local_media_copied": legacy.paths.len(),
                "original_snapshot": "legacy-shared_preferences.json",
                "unknown_snapshot_keys": legacy.unknown,
                "legacy_time": "preserved only in original JSON",
                "legacy_attachment_pluginId": "replaced with new asset IDs; original IDs retained in original JSON",
                "target_audit_verified": true
            }))?,
        )?;
        fs::remove_file(&marker)?;
        println!(
            "旧 JSON 导入完成: {}。原始快照保存在 legacy-shared_preferences.json；仅原样保留的字段见 migration-report.txt。",
            output.display()
        );
        Ok(())
    })();
    if result.is_err() {
        eprintln!(
            "目标副本保留在 {}；原 JSON 与媒体未改动。请选择全新目录重试。",
            output.display()
        );
    }
    result
}
