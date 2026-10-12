//! Private, opt-in Windows qualification bootstrap. No owner/storage is opened here.
use morrow_workbench_host::Result;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{fs::{File, OpenOptions}, io::Read, os::windows::fs::{MetadataExt, OpenOptionsExt}, path::{Path, PathBuf}};

pub const ENV: &str = "MORROW_C28_SESSION_QUALIFICATION";
pub const FLAG: &str = "--qualification-session-plan";
pub const FIXTURE_SHA: &str = "cca04ebb2e787f69e84ec7260aca3e93ec895ec17b68afbb660e3c6896ae2f2b";
pub fn sha(bytes: &[u8]) -> [u8; 32] { Sha256::digest(bytes).into() }
pub fn hex(bytes: &[u8]) -> String { bytes.iter().map(|b| format!("{b:02x}")).collect() }
pub fn unhex<const N: usize>(s: &str) -> Result<[u8; N]> {
    if s.len() != N * 2 || !s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
        return Err("qualification lowercase hex contract".into());
    }
    let mut out = [0; N];
    for (i, byte) in out.iter_mut().enumerate() { *byte = u8::from_str_radix(&s[i*2..i*2+2], 16)?; }
    if out.iter().all(|b| *b == 0) { return Err("qualification nonzero identity".into()); }
    Ok(out)
}
pub fn path(value: &str) -> Result<PathBuf> {
    let b = value.as_bytes();
    if b.len() < 4 || b.len() > 240 || !b.is_ascii() || !b[0].is_ascii_alphabetic()
        || &b[1..3] != b":/" || value.contains('\\') || value[2..].contains(':')
        || value[3..].split('/').any(|p| p.is_empty() || p == "." || p == ".." || reserved(p)
            || p.ends_with('.') || p.ends_with(' ') || p.bytes().any(|b| b < 32 || b"<>\"|?*".contains(&b))) {
        return Err("qualification absolute plain path contract".into());
    }
    Ok(PathBuf::from(value))
}
fn reserved(component: &str) -> bool {
    let stem=component.split('.').next().unwrap_or("").trim_end_matches(' ').to_ascii_uppercase();
    ["CON","PRN","AUX","NUL","CONIN$","CONOUT$"].contains(&stem.as_str())
        || stem.len()==4 && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && matches!(stem.as_bytes()[3], b'1'..=b'9')
}
/// Match the original supervisor profile's deliberate case/prefix normalization.
pub fn normalized(path: &Path) -> String {
    lexical(path).to_ascii_lowercase()
}
fn lexical(path: &Path)->String {
    let s=path.to_string_lossy().replace('\\',"/");
    s.strip_prefix("//?/").unwrap_or(&s).to_owned()
}
pub fn plain_ancestors(path: &Path) -> Result<()> {
    for part in path.ancestors() {
        let m = std::fs::symlink_metadata(part)?;
        if m.file_attributes() & 0x400 != 0 || m.file_type().is_symlink() {
            return Err("qualification reparse path".into());
        }
        if part != path && !m.is_dir() { return Err("qualification ancestor type".into()); }
    }
    Ok(())
}
fn hold_directories(path: &Path) -> Result<Vec<File>> {
    let mut parts:Vec<_>=path.ancestors().collect(); parts.reverse();
    let mut held=Vec::new();
    for part in parts {
        let file=OpenOptions::new().read(true).share_mode(1)
            .custom_flags(0x02000000|0x00200000).open(part)?;
        let m=file.metadata()?;
        if !m.is_dir() || m.file_attributes() & 0x400 != 0 {return Err("qualification held directory type/reparse".into());}
        held.push(file);
    }
    Ok(held)
}
/// Share-read only denies writes/deletes while this exact source is retained.
/// File flags open the leaf itself; ancestry/type and held-handle metadata are checked.
pub fn pinned(path: &Path, bytes: u64, digest: [u8; 32], cap: u64) -> Result<(File, Vec<u8>)> {
    if bytes == 0 || bytes > cap { return Err("qualification pinned input bound".into()); }
    plain_ancestors(path)?;
    let mut file = OpenOptions::new().read(true).share_mode(1).custom_flags(0x00200000).open(path)?;
    let before = file.metadata()?;
    if !before.is_file() || before.file_attributes() & 0x400 != 0 || before.len() != bytes {
        return Err("qualification pinned input type/size".into());
    }
    let mut raw = Vec::with_capacity(bytes as usize);
    (&mut file).take(bytes + 1).read_to_end(&mut raw)?;
    let after = file.metadata()?;
    if raw.len() as u64 != bytes || sha(&raw) != digest || before.len() != after.len()
        || before.last_write_time() != after.last_write_time() || before.creation_time() != after.creation_time() {
        return Err("qualification pinned input unstable/hash".into());
    }
    plain_ancestors(path)?;
    Ok((file, raw))
}
#[derive(Clone)]
pub struct Artifact { pub path: PathBuf, pub bytes: u64, pub sha: [u8; 32] }
pub struct Plan {
    pub root: PathBuf, pub run: String, pub nonce: [u8; 16], pub digest: [u8; 32],
    pub host: Artifact, pub supervisor: Artifact, pub package: Artifact, pub fixture: Artifact,
}
impl Plan {
    pub fn decode(raw: &[u8], digest: [u8; 32]) -> Result<Self> {
        if raw.is_empty() || raw.len() > 16384 || !raw.is_ascii() || sha(raw) != digest {
            return Err("qualification plan bound/hash".into());
        }
        let v: Value = serde_json::from_slice(raw)?;
        if serde_json::to_vec(&v)? != raw { return Err("qualification canonical plan".into()); }
        let a = v.as_array().ok_or("qualification plan array")?;
        if a.len() != 19 { return Err("qualification exact plan fields".into()); }
        let s = |i: usize| -> Result<&str> { a[i].as_str().ok_or_else(|| "qualification plan string".into()) };
        let n = |i: usize| -> Result<u64> { a[i].as_u64().ok_or_else(|| "qualification plan integer".into()) };
        if s(0)? != "c28-product-session-qualification-v1" || n(16)? != 40000
            || n(17)? != 20000 || n(18)? != 60000 { return Err("qualification fixed policy".into()); }
        let nonce = unhex::<16>(s(3)?)?;
        let root = path(s(1)?)?;
        if root.file_name().and_then(|x| x.to_str()) != Some(format!("c28-product-session-{}", hex(&nonce)).as_str()) {
            return Err("qualification synthetic profile leaf".into());
        }
        let run = s(2)?;
        if run.is_empty() || run.len() > 64 || !run.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
            return Err("qualification run ID".into());
        }
        let artifact = |i: usize, cap: u64| -> Result<Artifact> {
            let bytes = n(i + 1)?;
            if bytes == 0 || bytes > cap { return Err("qualification artifact bound".into()); }
            Ok(Artifact { path: path(s(i)?)?, bytes, sha: unhex(s(i + 2)?)? })
        };
        let fixture = artifact(13, 425912)?;
        if fixture.bytes != 425912 || hex(&fixture.sha) != FIXTURE_SHA { return Err("qualification immutable fixture".into()); }
        let host = artifact(4, 268435456)?;
        let supervisor = artifact(7, 268435456)?;
        let package = artifact(10, 33554432)?;
        let paths = [&host.path, &supervisor.path, &package.path, &fixture.path];
        let mut digests=std::collections::BTreeSet::new();
        for a in [&host,&supervisor,&package,&fixture] {
            if !digests.insert(a.sha) { return Err("qualification duplicate content identity".into()); }
        }
        for (i, p) in paths.iter().enumerate() {
            if p.starts_with(&root) || paths[..i].contains(p) { return Err("qualification overlapping paths".into()); }
        }
        Ok(Self { root, run: run.into(), nonce, digest, host, supervisor, package, fixture })
    }
    pub fn sid(&self) -> String { format!("c28-product-session-{}", hex(&self.nonce)) }
    pub fn package_id(&self) -> String { format!("c28.product.session.{}", hex(&self.nonce)) }
}
pub struct Admission {
    pub plan: Plan, pub fixture: Vec<u8>, pub trailer: Vec<String>,
    // Never release pinned inputs between validation and dispatch/shutdown.
    _pins: Vec<File>,
}
impl Admission {
    pub fn parse(args: &mut impl Iterator<Item=String>, database: &str, package: &Path,
        host: &Path, supervisor: Option<&Path>, managed: bool) -> Result<Option<Self>> {
        let Some(flag) = args.next() else {
            if std::env::var_os(ENV).is_some() { return Err("qualification env without trailer".into()); }
            return Ok(None);
        };
        if flag != FLAG || managed { return Err("qualification trailer/managed mode".into()); }
        // Validate caller-selected lexical paths before canonicalization/file opens.
        path(database)?;
        path(&lexical(package))?;
        path(&lexical(host))?;
        if let Some(actual)=supervisor {
            path(&lexical(actual))?;
        }
        let p = args.next().ok_or("qualification plan path")?;
        let length = args.next().ok_or("qualification plan bytes")?;
        let hash = args.next().ok_or("qualification plan hash")?;
        if args.next().is_some() || std::env::var(ENV).ok().as_deref() != Some(hash.as_str()) {
            return Err("qualification env/trailer mismatch".into());
        }
        let bytes: u64 = length.parse()?;
        if bytes.to_string() != length { return Err("qualification decimal bytes".into()); }
        let digest = unhex(&hash)?;
        let plan_path = path(&p)?;
        let mut directory_pins=hold_directories(plan_path.parent().ok_or("qualification plan parent")?)?;
        let (plan_pin, raw) = pinned(&plan_path, bytes, digest, 16384)?;
        let plan = Plan::decode(&raw, digest)?;
        let parent=plan.root.parent().ok_or("qualification profile parent")?;
        directory_pins.extend(hold_directories(parent)?);
        plain_ancestors(parent)?;
        let canonical_parent=parent.canonicalize()?;
        if !std::fs::symlink_metadata(parent)?.is_dir() || normalized(parent)!=normalized(&canonical_parent) {
            return Err("qualification canonical existing parent".into());
        }
        if plan_path.starts_with(&plan.root) || Path::new(database) != plan.root.join("workbench.db")
            || package.canonicalize()? != plan.package.path.canonicalize()?
            || host.canonicalize()? != plan.host.path.canonicalize()? {
            return Err("qualification actual path binding".into());
        }
        if let Some(actual)=supervisor {
            if actual.canonicalize()? != plan.supervisor.path.canonicalize()? {return Err("qualification actual supervisor path".into());}
        }
        let mut canonical_inputs=std::collections::BTreeSet::new();
        canonical_inputs.insert(normalized(&plan_path.canonicalize()?));
        let root_prefix=format!("{}/",normalized(&plan.root));
        let mut pins = directory_pins;
        pins.push(plan_pin);
        // Main must retain its already claimed profile path before ledger reads.
        if supervisor.is_none() {
            pins.extend(hold_directories(&plan.root)?);
            pins.extend(hold_directories(&plan.root.join("preview-cache"))?);
        }
        let mut fixture = Vec::new();
        for a in [&plan.host, &plan.supervisor, &plan.package, &plan.fixture] {
            pins.extend(hold_directories(a.path.parent().ok_or("qualification artifact parent")?)?);
            let canonical=normalized(&a.path.canonicalize()?);
            if canonical.starts_with(&root_prefix) || !canonical_inputs.insert(canonical) {
                return Err("qualification canonical input aliases/overlap".into());
            }
            let (pin, raw) = pinned(&a.path, a.bytes, a.sha, 268435456)?;
            if a.path == plan.fixture.path { fixture = raw; }
            pins.push(pin);
        }
        Ok(Some(Self { plan, fixture, trailer: vec![flag,p,length,hash], _pins: pins }))
    }
    pub fn create_profile_once(&mut self) -> Result<()> {
        let parent = self.plan.root.parent().ok_or("qualification profile parent")?;
        plain_ancestors(parent)?;
        if !std::fs::symlink_metadata(parent)?.is_dir() { return Err("qualification parent type".into()); }
        // Rust create_dir fails on AlreadyExists. No retry, adoption, remove or rollback.
        std::fs::create_dir(&self.plan.root)?;
        self._pins.extend(hold_directories(&self.plan.root)?);
        plain_ancestors(&self.plan.root)?;
        std::fs::create_dir(self.plan.root.join("preview-cache"))?;
        self._pins.extend(hold_directories(&self.plan.root.join("preview-cache"))?);
        self.profile(false)
    }
    pub fn pre_open(&self, binding: &Value) -> Result<()> {
        self.profile(true)?;
        if binding["phase"] != "Active" || binding["generation"] != 1
            || binding["host_sha256"] != hex(&self.plan.host.sha)
            || binding["supervisor_sha256"] != hex(&self.plan.supervisor.sha)
            || binding["package_sha256"] != hex(&self.plan.package.sha)
            || binding["profile"] != morrow_workbench_host::workbench_supervision::profile(&self.plan.root)
            || binding["child_pid"] != std::process::id() { return Err("qualification original active binding".into()); }
        unhex::<32>(binding["incarnation"].as_str().ok_or("qualification incarnation")?)?;
        Ok(())
    }
    pub fn pre_open_namespace(&self) -> Result<()> { self.profile(true) }
    fn profile(&self, claimed: bool) -> Result<()> {
        plain_ancestors(&self.plan.root)?;
        if !std::fs::symlink_metadata(&self.plan.root)?.is_dir() { return Err("qualification profile type".into()); }
        let mut seen = std::collections::BTreeSet::new();
        for item in std::fs::read_dir(&self.plan.root)? {
            let item = item?;
            let name = item.file_name().into_string().map_err(|_| "qualification profile filename")?;
            let allowed = name == "preview-cache" || claimed && [".morrow-supervision-v1", "workbench-supervisor.sqlite", "workbench-supervisor.sqlite-wal", "workbench-supervisor.sqlite-shm"].contains(&name.as_str());
            if !allowed || !seen.insert(name.clone()) { return Err("qualification unknown profile metadata".into()); }
            plain_ancestors(&item.path())?;
            let m = std::fs::symlink_metadata(item.path())?;
            if name == "preview-cache" {
                if !m.is_dir() || std::fs::read_dir(item.path())?.next().is_some() { return Err("qualification preview must be empty".into()); }
            } else if !m.is_file() || m.len() > 1048576 { return Err("qualification metadata bound/type".into()); }
        }
        if !seen.contains("preview-cache") || claimed && (!seen.contains(".morrow-supervision-v1") || !seen.contains("workbench-supervisor.sqlite")) {
            return Err("qualification required profile metadata".into());
        }
        Ok(())
    }
}
