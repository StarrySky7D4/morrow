//! Isolated development music library. Snapshot staging and library publication
//! are separate durable transactions; neither proves AVPlayer or protected IO.
use crate::{Engine, Reply, Request, Result, err, hex, unhex};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use morrow_core::{content::{Attachment, CardRecord}, content_change::ContentChange,
    lifecycle::GrantKind, transaction::{self, Lookup}};
use morrow_workbench_plugin::{preferences, services};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, io::{Read, Write}};

const ID: &str = "morrow-host-music-library-v1";
const TYPE: &str = "hmos-music-library";
const TITLE: &str = "HMOS private music library";
pub const MAX_AUDIO_BYTES: u64 = 150 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 512 * 1024 * 1024;
const MAX_ENTRIES: usize = 512;
const MAX_BODY: usize = morrow_core::content::MAX_RECORD_BYTES;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ImportRequest {
    pub schema_version: u32, pub track_id: String, pub operation_id: String,
    pub expected_revision: String, pub name: String, pub byte_length: String, pub sha256: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MatchCandidate {
    pub title: String, pub artist: String, pub duration: f64, pub synced: bool, pub has_lyrics: bool,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Read { after: String, limit: u32 },
    TrackRead { library_revision: String, track_id: String },
    ImportBegin { request: ImportRequest },
    ImportInspect { request: ImportRequest },
    ImportFile { request: ImportRequest },
    Reconcile { request: ImportRequest },
    Export { library_revision: String, track_id: String, import_operation: String,
        byte_length: String, sha256: String },
    SetLyrics { library_revision: String, operation_id: String, track_id: String,
        lyrics: String, lyric_source: String },
    Reorder { library_revision: String, operation_id: String, order: Vec<String> },
    Remove { library_revision: String, operation_id: String, track_id: String },
    Select { library_revision: String, operation_id: String, track_id: String },
    ShowLyrics { library_revision: String, operation_id: String, flag: bool },
    LyricsRead { library_revision: String, track_id: String, position_ms: String },
    Policy { library_revision: String, index: String, playing: bool, blocked: bool,
        position_ms: String, duration_ms: String, music_action: String, value: String, flag: bool },
    Match { title: String, artist: String, duration: f64, candidates: Vec<MatchCandidate> },
    ImportPolicy { byte_length: String },
}
#[derive(Debug, Serialize)]
pub struct TrackView {
    pub track_id: String, pub library_revision: String, pub import_operation: String,
    pub name: String, pub byte_length: String, pub sha256: String, pub phase: String,
    pub source_uri: String, pub title: String, pub artist: String, pub duration_ms: String,
    pub lyric_source: String, pub lyrics_byte_length: String, pub bytes_retained: bool,
    pub request: ImportRequest, pub request_json: String, pub request_sha256: String,
}
#[derive(Debug, Serialize)]
pub struct LyricLine { pub time_ms: String, pub text: String }
#[derive(Debug, Serialize)]
pub struct LyricsView {
    pub track_id: String, pub text: String, pub source: String, pub lines: Vec<LyricLine>,
    pub active_index: i32, pub untimed: bool,
}
#[derive(Debug, Serialize)]
pub struct PlaybackView {
    pub track_id: String, pub index: String, pub playing: bool, pub blocked: bool,
    pub position_ms: String, pub duration_ms: String, pub transport_effect: String,
}
#[derive(Debug, Serialize)]
pub struct View {
    pub schema_version: u32, pub kind: String, pub library_revision: String,
    pub order: Vec<String>, pub selected_track_id: String, pub show_lyrics: bool,
    pub online_lyrics: bool, pub tracks: Vec<TrackView>, pub next_after: String,
    pub repeated: bool, pub operation_id: String, pub operation_revision: String,
    #[serde(skip_serializing_if = "Option::is_none")] pub lyrics: Option<LyricsView>,
    #[serde(skip_serializing_if = "Option::is_none")] pub playback: Option<PlaybackView>,
    #[serde(skip_serializing_if = "Option::is_none")] pub match_index: Option<i32>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Entry { request: ImportRequest, request_json: String, phase: String, blob_id: String }
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Library {
    schema_version: u32, generation: u64, preferences: String, entries: Vec<Entry>,
    last_operation: String, last_request_json: String,
}
fn number(value: &str) -> Result<u64> {
    if value.is_empty() || value.len() > 20 || !value.bytes().all(|b| b.is_ascii_digit()) ||
        value.len() > 1 && value.starts_with('0') { return Err("MusicNumber".into()); }
    value.parse().map_err(|_| "MusicNumber".into())
}
fn identity(value: &str, limit: usize) -> Result<()> {
    if value.is_empty() || value.len() > limit || value.starts_with("morrow-host-") ||
        !value.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.')) {
        return Err("MusicIdentity".into());
    }
    Ok(())
}
fn source(id: &str) -> String { format!("file:///morrow-music/{id}") }
fn id_for_source(uri: &str) -> Result<String> {
    let id = uri.strip_prefix("file:///morrow-music/").ok_or("MusicSourceIdentity")?;
    identity(id, 64)?; Ok(id.into())
}
fn hash(value: &str) -> Result<[u8; 32]> {
    if value.len() != 64 || !value.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
        return Err("MusicHash".into());
    }
    unhex(value)?.try_into().map_err(|_| "MusicHash".into())
}
fn validate_import(r: &ImportRequest) -> Result<()> {
    if r.schema_version != 1 { return Err("MusicVersion".into()); }
    identity(&r.track_id, 64)?; identity(&r.operation_id, 128)?;
    number(&r.expected_revision)?; hash(&r.sha256)?;
    let size = number(&r.byte_length)?;
    if size == 0 { return Err("MusicEmptyAudio".into()); }
    if size > MAX_AUDIO_BYTES { return Err("MusicAudioBudget".into()); }
    services::import_policy("audio", size, "", false).map_err(err)?;
    if r.name.is_empty() || r.name.len() > 512 || r.name.chars().any(|c| c.is_control() || matches!(c, '/' | '\\')) {
        return Err("MusicName".into());
    }
    let ext = r.name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    if !matches!(ext.as_str(), "mp3"|"wav"|"flac"|"m4a"|"aac"|"ogg"|"opus"|"wma"|"ape"|
        "aif"|"aiff"|"alac"|"wv"|"dsf"|"dff"|"mp2"|"ac3"|"dts"|"au") || !r.name.contains('.') {
        return Err("MusicStandardAudioRequired".into());
    }
    Ok(())
}
fn import_literal(request: &ImportRequest) -> Result<String> {
    #[derive(Serialize)] struct Transport<'a> { action: &'static str, music: CommandRef<'a> }
    #[derive(Serialize)] struct CommandRef<'a> { action: &'static str, request: &'a ImportRequest }
    serde_json::to_string(&Transport { action: "music_import", music: CommandRef { action: "import_file", request } }).map_err(err)
}
fn internal(domain: &str, r: &ImportRequest) -> String {
    let mut h = Sha256::new(); h.update(b"morrow.hmos.music.v1\0"); h.update(domain); h.update([0]);
    h.update(r.track_id.as_bytes()); h.update([0]); h.update(r.operation_id.as_bytes());
    format!("morrow-host-music-{domain}-{}", hex(&h.finalize()))
}
fn defaults() -> Result<Library> {
    let p = preferences::Preferences { version: 1,
        appearance: Some(preferences::proto::Appearance { theme: "white".into(), glass: "frosted".into(),
            background: "ambient".into(), opacity: 0.76, corner_radius: 20., window_radius: 20.,
            component_blur: 22., component_opacity: 0.76, material_version: 1, ..Default::default() }),
        ..Default::default() };
    Ok(Library { schema_version: 1, generation: 0,
        preferences: STANDARD.encode(preferences::encode_persistent(&p, None).map_err(err)?),
        entries: vec![], last_operation: String::new(), last_request_json: String::new() })
}
fn prefs(l: &Library) -> Result<(Vec<u8>, preferences::Preferences)> {
    let raw = STANDARD.decode(&l.preferences).map_err(|_| "MusicPreferencesEncoding")?;
    if STANDARD.encode(&raw) != l.preferences { return Err("MusicPreferencesEncoding".into()); }
    let p = preferences::decode_persistent(&raw).map_err(err)?;
    Ok((raw, p))
}
fn set_prefs(l: &mut Library, p: &preferences::Preferences) -> Result<()> {
    let (old, _) = prefs(l)?;
    l.preferences = STANDARD.encode(preferences::encode_persistent(p, Some(&old)).map_err(err)?);
    Ok(())
}
fn parsed_lyrics(input: &str) -> Result<Vec<services::LyricLine>> {
    let parsed = services::parse_lyrics(input).map_err(err)?;
    // Repeated timestamp text can expand well beyond the input. Reserve room
    // for the complete library header/track proof and reject before persisting
    // an unreadable lyric record, without truncating either text or lines.
    let mut bytes = serde_json::to_vec(input).map_err(err)?.len();
    for line in &parsed {
        bytes = bytes.checked_add(serde_json::to_vec(&line.text).map_err(err)?.len() + 64).ok_or("MusicLyricReplyBudget")?;
        if bytes > crate::LIMIT - 128 * 1024 { return Err("MusicLyricReplyBudget".into()); }
    }
    Ok(parsed)
}
fn pins(l: &Library) -> Result<Vec<Attachment>> {
    let (_, p) = prefs(l)?;
    p.tracks.iter().map(|t| {
        let id = id_for_source(&t.source.as_ref().ok_or("MusicTrackSource")?.location)?;
        let e = l.entries.iter().find(|e| e.request.track_id == id && e.phase == "ready").ok_or("MusicTrackOrigin")?;
        Ok(Attachment { id, display_name: e.request.name.clone(), media_type: "audio/morrow-local".into(),
            byte_length: number(&e.request.byte_length)?, sha256: hash(&e.request.sha256)? })
    }).collect()
}
fn validate(l: &Library) -> Result<()> {
    if l.schema_version != 1 || l.entries.len() > MAX_ENTRIES { return Err("MusicLibrarySchema".into()); }
    let mut ids = BTreeSet::new(); let mut ops = BTreeSet::new(); let mut total = 0_u64;
    for e in &l.entries {
        validate_import(&e.request)?;
        if number(&e.request.expected_revision)? >= l.generation || !ids.insert(&e.request.track_id) || !ops.insert(&e.request.operation_id) ||
            e.request_json != import_literal(&e.request)? ||
            !matches!(e.phase.as_str(), "pending"|"ready"|"retired") ||
            (e.phase == "pending") != e.blob_id.is_empty() { return Err("MusicEntryShape".into()); }
        total = total.checked_add(number(&e.request.byte_length)?).ok_or("MusicTotalBudget")?;
    }
    if total > MAX_TOTAL_BYTES { return Err("MusicTotalBudget".into()); }
    let (_, p) = prefs(l)?;
    if p.online_lyrics { return Err("MusicOnlineUnsupported".into()); }
    let mut selected = BTreeSet::new();
    for t in &p.tracks {
        let s = t.source.as_ref().ok_or("MusicTrackSource")?;
        let id = id_for_source(&s.location)?;
        if !selected.insert(id.clone()) || s.kind != "audio" || !s.local || t.cover.is_some() ||
            t.duration > (1_u64 << 40) as f64 / 1000. { return Err("MusicTrackShape".into()); }
        let e = l.entries.iter().find(|e| e.request.track_id == id && e.phase == "ready").ok_or("MusicTrackOrigin")?;
        if s.name != e.request.name { return Err("MusicTrackName".into()); }
        parsed_lyrics(&t.lyrics)?;
    }
    if l.entries.iter().filter(|e| e.phase == "ready").count() != p.tracks.len() { return Err("MusicReadyOrder".into()); }
    if l.generation > 0 && (l.last_operation.is_empty() || l.last_request_json.is_empty()) { return Err("MusicMutationMissing".into()); }
    Ok(())
}
fn decode(body: &[u8]) -> Result<Library> {
    if body.len() > MAX_BODY { return Err("MusicLibraryBytes".into()); }
    let l: Library = serde_json::from_slice(body).map_err(|_| "MusicLibrarySchema")?;
    validate(&l)?; Ok(l)
}
fn validate_card(card: &CardRecord) -> Result<Library> {
    let s = card.summary(); let l = decode(&card.body())?;
    if s.id != ID || s.type_id != TYPE || s.format_version != 1 || s.title != TITLE ||
        s.revision != l.generation || card.attachments() != pins(&l)? { return Err("MusicLibraryIdentity".into()); }
    Ok(l)
}
pub(crate) fn is_library(card: &CardRecord) -> bool { card.summary().id == ID || card.summary().type_id == TYPE }
pub(crate) fn validate_library(card: &CardRecord) -> Result<()> { validate_card(card).map(|_| ()) }
fn current(e: &Engine) -> Result<Library> {
    e.host.store_local().card(ID).map_err(err)?.as_ref().map(validate_card).transpose()?.map(Ok).unwrap_or_else(defaults)
}
fn history(e: &Engine, operation: &str) -> Result<Option<(Library, u64)>> {
    let Some((commit, receipt)) = e.host.store_local().operation_commit(ID, operation).map_err(err)? else {
        return match e.host.store_local().lookup(operation).map_err(err)? {
            Lookup::Absent => Ok(None), _ => Err("MusicOperationOwner".into()),
        };
    };
    let cmd = transaction::decode_command(&commit.command).map_err(err)?;
    let l = match cmd.action {
        Some(transaction::proto::command::Action::CreateCard(raw)) => validate_card(&CardRecord::decode(&raw).map_err(err)?)?,
        Some(transaction::proto::command::Action::SetContent(c)) => {
            let l = decode(&c.body)?; let expected = pins(&l)?;
            let actual = c.attachments.ok_or("MusicHistoryPins")?;
            if c.card_id != ID || c.title != TITLE || !c.preview_text.is_empty() ||
                c.expected_revision.checked_add(1) != Some(receipt.revision) || actual.items.len() != expected.len() ||
                actual.items.iter().zip(expected).any(|(a,b)| a.id != b.id || a.display_name != b.display_name ||
                    a.media_type != b.media_type || a.byte_length != b.byte_length || a.sha256 != b.sha256) {
                return Err("MusicHistoryCommand".into());
            }
            l
        },
        _ => return Err("MusicHistoryCommand".into()),
    };
    if l.generation != receipt.revision || l.last_operation != operation { return Err("MusicHistoryIdentity".into()); }
    Ok(Some((l, receipt.revision)))
}
fn original(e: &Engine, request: &ImportRequest) -> Result<(Entry, u64)> {
    validate_import(request)?;
    let (l, revision) = history(e, &request.operation_id)?.ok_or("MusicImportMissing")?;
    let cmd = Command::ImportBegin { request: request.clone() };
    if l.last_request_json != serde_json::to_string(&cmd).map_err(err)? { return Err("MusicImportChanged".into()); }
    let entry = l.entries.into_iter().find(|x| x.request.track_id == request.track_id).ok_or("MusicImportMissing")?;
    if number(&request.expected_revision)?.checked_add(1) != Some(revision) || entry.request != *request || entry.phase != "pending" || !entry.blob_id.is_empty() { return Err("MusicImportChanged".into()); }
    Ok((entry, revision))
}
fn retained(e: &Engine, entry: &Entry) -> Result<Option<morrow_core::attachment::BlobInfo>> {
    let value = e.host.store_local().retained_blob_local(&internal("owner", &entry.request)).map_err(err)?;
    if let Some(v) = &value {
        if v.byte_length != number(&entry.request.byte_length)? || v.sha256 != hash(&entry.request.sha256)? ||
            !entry.blob_id.is_empty() && v.id != entry.blob_id { return Err("MusicRetainedSourceChanged".into()); }
    }
    if entry.phase != "pending" && value.is_none() { return Err("MusicRetainedSourceMissing".into()); }
    Ok(value)
}
fn entry_view(e: &Engine, l: &Library, entry: &Entry) -> Result<TrackView> {
    let (original, _) = original(e, &entry.request)?;
    if original.request_json != entry.request_json { return Err("MusicImportChanged".into()); }
    let bytes_retained = retained(e, entry)?.is_some();
    let (_, p) = prefs(l)?;
    let t = p.tracks.iter().find(|t| t.source.as_ref().is_some_and(|s| s.location == source(&entry.request.track_id)));
    let fallback = entry.request.name.rsplit_once('.').map_or(entry.request.name.as_str(), |(s,_)| s).to_string();
    Ok(TrackView { track_id: entry.request.track_id.clone(), library_revision: l.generation.to_string(),
        import_operation: entry.request.operation_id.clone(), name: entry.request.name.clone(),
        byte_length: entry.request.byte_length.clone(), sha256: entry.request.sha256.clone(), phase: entry.phase.clone(),
        source_uri: source(&entry.request.track_id), title: t.map_or(fallback, |t| t.title.clone()),
        artist: t.map_or(String::new(), |t| t.artist.clone()), duration_ms: t.map_or(0, |t| (t.duration * 1000.).round() as u64).to_string(),
        lyric_source: t.map_or(String::new(), |t| t.lyric_source.clone()), lyrics_byte_length: t.map_or(0, |t| t.lyrics.len()).to_string(),
        bytes_retained, request: entry.request.clone(), request_json: entry.request_json.clone(),
        request_sha256: hex(&Sha256::digest(entry.request_json.as_bytes())) })
}
fn summary(l: &Library, kind: &str) -> Result<View> {
    let (_, p) = prefs(l)?;
    let order = p.tracks.iter().map(|t| id_for_source(&t.source.as_ref().ok_or("MusicTrackSource")?.location)).collect::<Result<Vec<_>>>()?;
    let selected_track_id = order.get(p.index as usize).cloned().unwrap_or_default();
    Ok(View { schema_version: 1, kind: kind.into(), library_revision: l.generation.to_string(), order,
        selected_track_id, show_lyrics: p.show_lyrics, online_lyrics: false, tracks: vec![], next_after: String::new(),
        repeated: false, operation_id: String::new(), operation_revision: String::new(), lyrics: None, playback: None, match_index: None })
}
fn reply(e: &Engine, view: View) -> Result<Reply> {
    let mut r = Reply::failure(String::new()); r.ok = true; r.effect = e.effect;
    r.receipt_revision = if e.effect == "committed" { view.operation_revision.clone() } else { String::new() }; r.music = Some(view);
    if serde_json::to_vec(&r).map_err(err)?.len() > crate::LIMIT { return Err("MusicReplyBytesLimit".into()); }
    Ok(r)
}
fn write(e: &mut Engine, l: &Library, previous: u64) -> Result<u64> {
    validate(l)?;
    let body = serde_json::to_vec(l).map_err(err)?;
    if body.len() > MAX_BODY { return Err("MusicLibraryBytes".into()); }
    let references = pins(l)?; let start = e.start;
    let clock = || u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX - 1).saturating_add(1);
    let mut c = e.host.connect().map_err(err)?;
    let result = (|| {
        let now = clock();
        e.host.grant(&mut c, if previous == 0 { GrantKind::CreateContent } else { GrantKind::EditContent },
            ID, now.saturating_add(30000), now).map_err(err)?;
        e.effect = "unknown";
        let commit = if previous == 0 {
            let card = CardRecord::new_with_attachments(ID, TYPE, 1, TITLE, body, &references).map_err(err)?;
            e.host.create_content(&c, &l.last_operation, &card, clock)
        } else {
            e.host.edit_content(&c, &ContentChange { operation_id: l.last_operation.clone(), card_id: ID.into(),
                expected_revision: previous, title: TITLE.into(), body, preview_text: String::new(), attachments: Some(references) }, clock)
        };
        e.commit_result(commit)?;
        Ok(l.generation)
    })();
    let disconnected = e.host.disconnect(&c).map_err(err);
    result.and_then(|v| disconnected.map(|_| v))
}
fn observed_write(e: &mut Engine, op: &str, literal: &str) -> Result<Option<u64>> {
    e.effect = "unknown";
    if let Some((old, revision)) = history(e, op)? {
        if old.last_request_json != literal { e.effect = "not_committed"; return Err("MusicOperationChanged".into()); }
        e.effect = "committed"; Ok(Some(revision))
    } else { e.effect = "not_committed"; Ok(None) }
}
fn mutation_reply(e: &Engine, op: &str, revision: u64, repeated: bool, track: Option<&str>) -> Result<Reply> {
    let l = current(e)?; let mut v = summary(&l, if track.is_some() { "track" } else { "library" })?;
    if let Some(id) = track {
        let entry = l.entries.iter().find(|x| x.request.track_id == id).ok_or("MusicTrackMissing")?;
        v.tracks.push(entry_view(e, &l, entry)?);
    }
    v.operation_id = op.into(); v.operation_revision = revision.to_string(); v.repeated = repeated;
    reply(e, v)
}
fn begin(e: &mut Engine, request: &ImportRequest) -> Result<Reply> {
    validate_import(request)?; let cmd = Command::ImportBegin { request: request.clone() };
    let literal = serde_json::to_string(&cmd).map_err(err)?;
    if let Some(rev) = observed_write(e, &request.operation_id, &literal)? {
        original(e, request)?;
        return mutation_reply(e, &request.operation_id, rev, true, Some(&request.track_id));
    }
    let mut l = current(e)?;
    if l.generation != number(&request.expected_revision)? { return Err("MusicRevisionConflict".into()); }
    if l.entries.len() >= MAX_ENTRIES || l.entries.iter().any(|x| x.request.track_id == request.track_id || x.request.operation_id == request.operation_id) {
        return Err("MusicTrackIdentityUsed".into());
    }
    let previous = l.generation;
    l.entries.push(Entry { request: request.clone(), request_json: import_literal(request)?, phase: "pending".into(), blob_id: String::new() });
    l.generation = previous.checked_add(1).ok_or("MusicRevisionLimit")?;
    l.last_operation = request.operation_id.clone(); l.last_request_json = literal;
    let rev = write(e, &l, previous)?;
    mutation_reply(e, &request.operation_id, rev, false, Some(&request.track_id))
}
fn inspect(e: &Engine, request: &ImportRequest) -> Result<Reply> {
    let (_, rev) = original(e, request)?; let l = current(e)?;
    let entry = l.entries.iter().find(|x| x.request == *request).ok_or("MusicImportChanged")?;
    let mut v = summary(&l, "track")?; v.tracks.push(entry_view(e, &l, entry)?);
    // Read-only receipt context, not a newly committed mutation.
    v.operation_id = request.operation_id.clone(); v.operation_revision = rev.to_string(); v.repeated = true;
    reply(e, v)
}
fn ready(e: &mut Engine, request: &ImportRequest) -> Result<Reply> {
    original(e, request)?; let op = internal("ready", request); let literal = import_literal(request)?;
    if let Some(rev) = observed_write(e, &op, &literal)? {
        return mutation_reply(e, &op, rev, true, Some(&request.track_id));
    }
    let mut l = current(e)?;
    let entry = l.entries.iter_mut().find(|x| x.request == *request).ok_or("MusicImportChanged")?;
    if entry.phase != "pending" { return Err("MusicImportNotPending".into()); }
    let Some(blob) = retained(e, entry)? else { return inspect(e, request); };
    entry.phase = "ready".into(); entry.blob_id = blob.id;
    let (_, mut p) = prefs(&l)?;
    p.tracks.push(preferences::Track { source: Some(preferences::Source { location: source(&request.track_id),
        name: request.name.clone(), kind: "audio".into(), local: true }),
        title: request.name.rsplit_once('.').map_or(request.name.as_str(), |(v,_)| v).into(), ..Default::default() });
    set_prefs(&mut l, &p)?;
    let previous = l.generation; l.generation = previous.checked_add(1).ok_or("MusicRevisionLimit")?;
    l.last_operation = op.clone(); l.last_request_json = literal;
    let rev = write(e, &l, previous)?;
    mutation_reply(e, &op, rev, false, Some(&request.track_id))
}
pub(crate) fn import_from(e: &mut Engine, request: &ImportRequest, reader: &mut impl Read) -> Result<Reply> {
    original(e, request)?;
    let l = current(e)?; let entry = l.entries.iter().find(|x| x.request == *request).ok_or("MusicImportChanged")?;
    if entry.phase != "pending" { return ready(e, request); }
    // A retained owner is authoritative: explicit retry does not read replacement bytes.
    if retained(e, entry)?.is_none() {
        e.effect = "unknown";
        let staged = e.host.store_local_mut().stage_blob_retained(reader, number(&request.byte_length)?,
            hash(&request.sha256)?, &internal("owner", request), crate::unix_millis()?);
        match staged {
            Ok(_) => e.effect = "not_committed", // only Ready's operation is the outward mutation receipt
            Err(x) => {
                // A transaction rollback is known for these validation/reader failures.
                if matches!(x, morrow_core::Error::Invalid(_) | morrow_core::Error::Limit | morrow_core::Error::Integrity | morrow_core::Error::Io) { e.effect = "not_committed"; }
                return Err(x.to_string());
            }
        }
    }
    ready(e, request)
}
pub(crate) fn export_to(e: &Engine, command: &Command, writer: &mut impl Write) -> Result<crate::file_stream::FileMetadata> {
    let Command::Export { library_revision, track_id, import_operation, byte_length, sha256 } = command else { return Err("MusicExportCommand".into()); };
    let l = current(e)?;
    if l.generation != number(library_revision)? { return Err("MusicRevisionConflict".into()); }
    let entry = l.entries.iter().find(|x| x.request.track_id == *track_id).ok_or("MusicTrackMissing")?;
    original(e, &entry.request)?;
    if entry.phase != "ready" || entry.request.operation_id != *import_operation ||
        entry.request.byte_length != *byte_length || entry.request.sha256 != *sha256 { return Err("MusicExportIdentity".into()); }
    let blob = retained(e, entry)?.ok_or("MusicRetainedSourceMissing")?;
    let exported = e.host.store_local().export_blob_local(&blob.id, writer).map_err(err)?;
    writer.flush().map_err(err)?;
    if exported != blob { return Err("MusicExportIdentity".into()); }
    Ok(crate::file_stream::FileMetadata::new(blob.byte_length, &blob.sha256))
}

pub(crate) fn reject_other_envelope(r: &Request) -> Result<()> {
    if r.music.is_some() && !matches!(r.action.as_str(), "music"|"music_import"|"music_export") { return Err("MusicOuterAction".into()); }
    Ok(())
}
pub(crate) fn take_command(mut r: Request) -> Result<Command> {
    let command = r.music.take().ok_or("MusicCommandRequired")?;
    if !crate::editor_business::route_is_empty(&r) || r.editor_save.is_some() || r.editor_commit.is_some() ||
        r.editor_intent.is_some() || r.editor_intent_issue.is_some() || r.editor_intent_ref.is_some() ||
        r.editor_intent_query.is_some() || r.editor_intent_close.is_some() || r.business_handoff.is_some() || r.business_retirement.is_some() {
        return Err("MusicOuterFields".into());
    }
    Ok(command)
}

fn require_revision(l: &Library, revision: &str) -> Result<()> {
    if l.generation != number(revision)? { return Err("MusicRevisionConflict".into()); }
    Ok(())
}
fn order(l: &Library) -> Result<Vec<String>> { Ok(summary(l, "library")?.order) }
fn mutate(e: &mut Engine, command: &Command) -> Result<Reply> {
    let (revision, op, track) = match command {
        Command::SetLyrics { library_revision, operation_id, track_id, .. } |
        Command::Remove { library_revision, operation_id, track_id } |
        Command::Select { library_revision, operation_id, track_id } => (library_revision, operation_id, Some(track_id.as_str())),
        Command::Reorder { library_revision, operation_id, .. } |
        Command::ShowLyrics { library_revision, operation_id, .. } => (library_revision, operation_id, None),
        _ => return Err("MusicMutationCommand".into()),
    };
    identity(op, 128)?; number(revision)?;
    if let Some(id) = track { identity(id, 64)?; }
    let literal = serde_json::to_string(command).map_err(err)?;
    if let Some(old) = observed_write(e, op, &literal)? { return mutation_reply(e, op, old, true, track); }
    let mut l = current(e)?; require_revision(&l, revision)?;
    let (_, mut p) = prefs(&l)?; let prior_order = order(&l)?;
    let selected = prior_order.get(p.index as usize).cloned();
    let index = track.map(|id| prior_order.iter().position(|x| x == id).ok_or("MusicReadyTrackRequired")).transpose()?;
    match command {
        Command::SetLyrics { lyrics, lyric_source, .. } => {
            if lyrics.len() > 49152 || lyric_source.len() > 4096 { return Err("MusicLyricBudget".into()); }
            parsed_lyrics(lyrics)?;
            let t = &mut p.tracks[index.ok_or("MusicReadyTrackRequired")?];
            t.lyrics = lyrics.clone(); t.lyric_source = lyric_source.clone();
        },
        Command::Reorder { order, .. } => {
            if order.len() != prior_order.len() || order.iter().collect::<BTreeSet<_>>().len() != order.len() ||
                order.iter().collect::<BTreeSet<_>>() != prior_order.iter().collect::<BTreeSet<_>>() { return Err("MusicOrderChanged".into()); }
            p.tracks = order.iter().map(|id| p.tracks[prior_order.iter().position(|v| v == id).unwrap()].clone()).collect();
            p.index = selected.as_ref().and_then(|id| order.iter().position(|v| v == id)).unwrap_or(0) as i32;
        },
        Command::Remove { track_id, .. } => {
            let removed = index.ok_or("MusicReadyTrackRequired")?;
            p.tracks.remove(removed);
            let entry = l.entries.iter_mut().find(|x| x.request.track_id == *track_id).ok_or("MusicTrackMissing")?;
            entry.phase = "retired".into();
            p.index = if selected.as_deref() == Some(track_id.as_str()) {
                removed.min(p.tracks.len().saturating_sub(1)) as i32
            } else {
                p.tracks.iter().position(|t| t.source.as_ref().is_some_and(|s| selected.as_ref().is_some_and(|id| s.location == source(id)))).unwrap_or(0) as i32
            };
            // The Snapshot and historical event pins remain. Explicit future GC
            // needs its own durable release proof, never a UI disappearance.
        },
        Command::Select { .. } => p.index = index.ok_or("MusicReadyTrackRequired")? as i32,
        Command::ShowLyrics { flag, .. } => p.show_lyrics = *flag,
        _ => unreachable!(),
    }
    set_prefs(&mut l, &p)?;
    let previous = l.generation; l.generation = previous.checked_add(1).ok_or("MusicRevisionLimit")?;
    l.last_operation = op.clone(); l.last_request_json = literal;
    let written = write(e, &l, previous)?;
    mutation_reply(e, op, written, false, track)
}
pub(crate) fn execute(e: &mut Engine, command: Command) -> Result<Reply> {
    match &command {
        Command::ImportBegin { request } => return begin(e, request),
        Command::ImportInspect { request } => return inspect(e, request),
        Command::Reconcile { request } => return ready(e, request),
        Command::SetLyrics { .. } | Command::Reorder { .. } | Command::Remove { .. } |
        Command::Select { .. } | Command::ShowLyrics { .. } => return mutate(e, &command),
        Command::ImportFile { .. } | Command::Export { .. } => return Err("MusicFileDescriptorRequired".into()),
        _ => {},
    }
    let l = current(e)?;
    let mut v = summary(&l, "library")?;
    match command {
        Command::Read { after, limit } => {
            if !(1..=16).contains(&limit) { return Err("MusicPageLimit".into()); }
            if !after.is_empty() { identity(&after, 64)?; }
            let mut entries: Vec<_> = l.entries.iter().filter(|x| x.request.track_id > after).collect();
            entries.sort_by(|a,b| a.request.track_id.cmp(&b.request.track_id));
            let more = entries.len() > limit as usize;
            for entry in entries.into_iter().take(limit as usize) { v.tracks.push(entry_view(e, &l, entry)?); }
            if more { v.next_after = v.tracks.last().ok_or("MusicPageEmpty")?.track_id.clone(); }
        },
        Command::TrackRead { library_revision, track_id } => {
            require_revision(&l, &library_revision)?; identity(&track_id, 64)?; v.kind = "track".into();
            let entry = l.entries.iter().find(|x| x.request.track_id == track_id).ok_or("MusicTrackMissing")?;
            v.tracks.push(entry_view(e, &l, entry)?);
        },
        Command::LyricsRead { library_revision, track_id, position_ms } => {
            require_revision(&l, &library_revision)?; identity(&track_id, 64)?;
            let position = number(&position_ms)?;
            if position > 1 << 40 { return Err("MusicTimeBudget".into()); }
            let entry = l.entries.iter().find(|x| x.request.track_id == track_id && x.phase == "ready").ok_or("MusicReadyTrackRequired")?;
            v.tracks.push(entry_view(e, &l, entry)?);
            let (_, p) = prefs(&l)?;
            let t = p.tracks.iter().find(|t| t.source.as_ref().is_some_and(|s| s.location == source(&track_id))).ok_or("MusicReadyTrackRequired")?;
            let parsed = parsed_lyrics(&t.lyrics)?;
            let active = parsed.iter().rposition(|x| x.time_ms <= position).map_or(-1, |x| x as i32);
            let untimed = parsed.is_empty();
            v.kind = "lyrics".into();
            v.lyrics = Some(LyricsView { track_id, text: t.lyrics.clone(), source: t.lyric_source.clone(),
                lines: parsed.into_iter().map(|x| LyricLine { time_ms: x.time_ms.to_string(), text: x.text }).collect(), active_index: active, untimed });
        },
        Command::Policy { library_revision, index, playing, blocked, position_ms, duration_ms, music_action, value, flag } => {
            require_revision(&l, &library_revision)?;
            let duration = number(&duration_ms)?; let position = number(&position_ms)?; let index = number(&index)?;
            if duration > 1 << 40 || position > 1 << 40 || index > i32::MAX as u64 { return Err("MusicTimeBudget".into()); }
            let value: i64 = value.parse().map_err(|_| "MusicSignedNumber")?;
            let action = match music_action.as_str() {
                "restore" => services::MusicAction::Restore, "block" => services::MusicAction::Block,
                "toggle" => services::MusicAction::Toggle, "seek" => services::MusicAction::Seek,
                "select" => services::MusicAction::Select, "next" => services::MusicAction::Next,
                "previous" => services::MusicAction::Previous, "remove" => services::MusicAction::Remove,
                _ => return Err("MusicPlaybackAction".into()),
            };
            let (decision, effect) = services::Playback { ids: v.order.clone(), index: index as i32, playing, blocked,
                position_ms: position, duration_ms: duration }.apply(action, value, flag).map_err(err)?;
            v.kind = "playback".into();
            v.playback = Some(PlaybackView { track_id: decision.ids.get(decision.index as usize).cloned().unwrap_or_default(),
                index: decision.index.to_string(), playing: decision.playing, blocked: decision.blocked,
                position_ms: decision.position_ms.to_string(), duration_ms: decision.duration_ms.to_string(), transport_effect: effect.into() });
        },
        Command::Match { title, artist, duration, candidates } => {
            if title.len() > 16384 || artist.len() > 16384 || candidates.iter().any(|x| x.title.len() > 16384 || x.artist.len() > 16384) {
                return Err("MusicMatchBudget".into());
            }
            let list = candidates.into_iter().map(|x| services::Candidate { title:x.title, artist:x.artist, duration:x.duration,
                synced:x.synced, lyrics: if x.has_lyrics { "present".into() } else { String::new() } }).collect::<Vec<_>>();
            v.kind = "match".into(); v.match_index = Some(services::lyric_match(&list, &title, &artist, duration).map_err(err)?.map_or(-1, |x| x as i32));
        },
        Command::ImportPolicy { byte_length } => {
            let size = number(&byte_length)?;
            if size == 0 { return Err("MusicEmptyAudio".into()); }
            services::import_policy("audio", size, "", false).map_err(err)?; v.kind = "import_policy".into();
        },
        _ => unreachable!(),
    }
    reply(e, v)
}

#[cfg(test)]
#[path = "music_bridge_tests.rs"]
mod tests;
