//! HMOS trusted local development adapter. The production Workbench remains
//! gated until the Harmony HUKS/audit/lease backend is qualified.
use morrow_core::{
    content::CardRecord,
    dispatch::HostRuntime,
    lifecycle::GrantKind,
    store::{EventBudget, Store},
    versioned_content_change::VersionedContentChange,
};
use morrow_workbench_plugin::{cards_v2, query_v2, tasks_v2};
use prost::Message;
use serde::{Deserialize, Serialize};
use std::{
    ffi::{CStr, CString, c_char},
    path::Path,
    sync::Mutex,
};

// Byte-for-byte pure scheduler reference; provenance is pinned in
// ../query-plan-reference.json. It establishes query correspondence only,
// not a production plugin execution, permission, or durable capture.
mod draft_bridge;
mod attachment_bridge;
pub mod editor_draft;
pub mod editor_draft_staging;
pub mod file_stream;
pub mod clipboard;
pub mod editor_field;
pub mod editor_input;
mod create_todos;
mod editor_business;
mod editor_intent;
mod editor_handoff;
mod music_bridge;
pub mod markdown;
pub mod query_plan_v2;

const LIMIT: usize = 512 * 1024;
static SESSION: Mutex<Option<Engine>> = Mutex::new(None);
type Result<T> = std::result::Result<T, String>;
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Request {
    action: String,
    path: String,
    operation: String,
    id: String,
    source: String,
    title: String,
    description: String,
    hypothesis: String,
    conclusion: String,
    todos: String,
    category: String,
    stage: String,
    task_id: String,
    order: Vec<String>,
    text: String,
    section: String,
    filter: String,
    sort: String,
    flag: bool,
    now_ms: String,
    draft: Option<draft_bridge::Write>,
    fork: Option<draft_bridge::Fork>,
    fork_retirement: Option<draft_bridge::ForkRetirement>,
    draft_id: String,
    generation: String,
    draft_operation: String,
    attachment_id: String,
    import_request: Option<attachment_bridge::ImportWrite>,
    editor_save: Option<editor_business::SaveEnvelope>,
    editor_commit: Option<editor_business::InspectEnvelope>,
    editor_intent: Option<editor_intent::Prepare>,
    editor_intent_issue: Option<editor_intent::Issue>,
    editor_intent_ref: Option<editor_intent::Read>,
    editor_intent_query: Option<editor_intent::Query>,
    editor_intent_close: Option<editor_intent::Close>,
    business_handoff: Option<editor_handoff::Envelope>,
    business_retirement: Option<editor_handoff::Envelope>,
    music: Option<music_bridge::Command>,
    #[serde(skip)]
    transport_json: String,
}
#[derive(Debug, Serialize)]
pub struct TaskView {
    id: String,
    text: String,
    completion: i32,
}
#[derive(Debug, Serialize)]
pub struct CardView {
    id: String,
    revision: String,
    source: String,
    // Current-card classification is derived only after complete schema and
    // migration-origin validation. Historical receipts keep their exact DTO.
    #[serde(skip_serializing_if = "Option::is_none")]
    content_kind: Option<&'static str>,
    title: String,
    description: String,
    hypothesis: String,
    conclusion: String,
    category: String,
    stage: String,
    favorite: bool,
    deleted: bool,
    deleted_at: String,
    tasks: Vec<TaskView>,
    assets: Vec<attachment_bridge::AssetView>,
}
#[derive(Debug, Serialize)]
pub struct Reply {
    ok: bool,
    error: String,
    cards: Vec<CardView>,
    ids: Vec<String>,
    drafts: Vec<draft_bridge::View>,
    markdown: markdown::MarkdownDoc,
    paste_text: String,
    receipt_revision: String,
    profile: &'static str,
    effect: &'static str,
    imports: Vec<attachment_bridge::ImportView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    editor_commit: Option<editor_business::CommitView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    editor_intents: Option<Vec<editor_intent::View>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    intent_next_after: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    music: Option<music_bridge::View>,
}
impl Reply {
    fn failure(message: String) -> Self {
        Self {
            ok: false,
            error: message,
            cards: vec![],
            ids: vec![],
            drafts: vec![],
            markdown: markdown::MarkdownDoc::default(),
            paste_text: String::new(),
            receipt_revision: String::new(),
            profile: "development-unsealed",
            effect: "unknown",
            imports: vec![],
            editor_commit: None, editor_intents: None, intent_next_after: None, music: None,
        }
    }
}

pub struct Engine {
    host: HostRuntime,
    start: std::time::Instant,
    effect: &'static str,
}
impl Engine {
    fn published_assets(&mut self, request: &Request) -> Result<Option<editor_draft::PublishedAssets>> {
        if request.draft_id.is_empty() {
            if !request.generation.is_empty() || !request.draft_operation.is_empty() { return Err("IncompleteDraftPublication".into()); }
            return Ok(None);
        }
        let committed = matches!(self.host.store_local().lookup_for_card(&request.id, &request.operation).map_err(err)?, morrow_core::transaction::Lookup::Committed(_));
        let selected = if committed {
            self.effect = "committed";
            editor_draft::published_assets_history(&self.host, &request.id, &request.draft_id, draft_bridge::number(&request.generation)?, &request.draft_operation, &unhex(&request.source)?).map_err(err)?
        } else {
            editor_draft::publish_assets(&self.host, &request.id, &request.draft_id, draft_bridge::number(&request.generation)?, &request.draft_operation, &unhex(&request.source)?).map_err(err)?
        };
        let values = &selected.values;
        if values.title.as_ref().is_none_or(|v| v.text != request.title) || values.description.as_ref().is_none_or(|v| v.text != request.description) ||
            values.hypothesis.as_ref().is_none_or(|v| v.text != request.hypothesis) || values.conclusion.as_ref().is_none_or(|v| v.text != request.conclusion) ||
            request.action == "create" && (values.category != request.category || values.stage != request.stage ||
                values.todos.as_ref().is_none_or(|v| v.text != request.todos)) {
            return Err("DraftPublicationValuesMismatch".into());
        }
        Ok(Some(selected))
    }
    fn import_reply(&self, imports: Vec<editor_draft_staging::DraftImportRecord>) -> Reply {
        Reply { ok: true, error: String::new(), cards: vec![], ids: vec![], drafts: vec![], markdown: markdown::MarkdownDoc::default(), paste_text: String::new(), receipt_revision: String::new(), profile: "development-unsealed", effect: self.effect, imports: imports.into_iter().map(Into::into).collect(), editor_commit: None, editor_intents: None, intent_next_after: None, music: None }
    }
    pub fn import_from(&mut self, request: Request, reader: &mut impl std::io::Read) -> Result<Reply> {
        self.effect = "not_committed";
        music_bridge::reject_other_envelope(&request)?;
        editor_business::reject_other_envelope(&request)?;
        editor_intent::reject_other_envelope(&request)?;
        editor_handoff::reject_other_envelope(&request)?;
        if request.action == "music_import" {
            let music_bridge::Command::ImportFile { request } = music_bridge::take_command(request)? else { return Err("MusicImportCommand".into()); };
            return music_bridge::import_from(self, &request, reader);
        }
        if request.action != "import_file" { return Err("UnsupportedFileAction".into()); }
        let start = self.start;
        let clock = || u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX - 1).saturating_add(1);
        let record = editor_draft_staging::import_durable(&mut self.host, &request.import_request.ok_or("ImportRequestRequired")?.request()?, reader, clock, unix_millis()?, &mut self.effect).map_err(err)?;
        Ok(self.import_reply(vec![record]))
    }
    pub fn export_to(&mut self, request: Request, writer: &mut impl std::io::Write) -> Result<file_stream::FileMetadata> {
        self.effect = "not_committed";
        music_bridge::reject_other_envelope(&request)?;
        editor_business::reject_other_envelope(&request)?;
        editor_intent::reject_other_envelope(&request)?;
        editor_handoff::reject_other_envelope(&request)?;
        if request.action == "music_export" {
            let command = music_bridge::take_command(request)?;
            return music_bridge::export_to(self, &command, writer);
        }
        if request.id.starts_with("morrow-host-") { return Err("HostOwnedIdentity".into()); }
        let info = match request.action.as_str() {
            "import_export" => editor_draft_staging::export_verified(&self.host, &request.id, &request.draft_id, draft_bridge::number(&request.generation)?, &request.operation, writer).map_err(err)?,
            "draft_asset_export" => editor_draft::export_asset_verified(&self.host, &request.id, &request.draft_id, draft_bridge::number(&request.generation)?, &request.attachment_id, writer).map_err(err)?,
            "attachment_export" => return self.export_card_attachment(&request, writer),
            _ => return Err("UnsupportedFileAction".into()),
        };
        Ok(file_stream::FileMetadata::new(info.byte_length, &info.sha256))
    }
    fn export_card_attachment(&mut self, request: &Request, writer: &mut impl std::io::Write) -> Result<file_stream::FileMetadata> {
        use sha2::{Digest, Sha256};
        let card = self.host.store_local().card(&request.id).map_err(err)?.ok_or("NotFound")?;
        // Full source and revision remain bound for the entire serialized read.
        if card.encode() != unhex(&request.source)? || card.summary().revision != draft_bridge::number(&request.generation)? { return Err("RevisionConflict".into()); }
        let asset = card.attachments().into_iter().find(|a| a.id == request.attachment_id).ok_or("NotFound")?;
        let start = self.start;
        let clock = || u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX - 1).saturating_add(1);
        let mut connection = self.host.connect().map_err(err)?;
        let result = (|| -> Result<file_stream::FileMetadata> {
            let now = clock();
            self.host.grant_attachment(&mut connection, &request.id, &request.attachment_id, now.saturating_add(300_000), now).map_err(err)?;
            let mut offset = 0_u64;
            let mut hash = Sha256::new();
            loop {
                let command = morrow_core::runtime::Command::ReadAttachment(morrow_core::runtime::ReadAttachment { request_id: format!("hmos-attachment-read-{offset}"), card_id: request.id.clone(), attachment_id: request.attachment_id.clone(), expected_revision: card.summary().revision, offset, length: 32768 });
                let response = self.host.dispatch(&connection, &command.encode().map_err(err)?, clock).map_err(err)?;
                let chunk = match morrow_core::response::Response::decode(&response).map_err(err)?.outcome {
                    morrow_core::response::Outcome::AttachmentChunk(chunk) => chunk,
                    _ => return Err("AttachmentReadFailed".into()),
                };
                if chunk.offset != offset || chunk.total_length != asset.byte_length || chunk.content_sha256 != asset.sha256 { return Err("AttachmentIntegrity".into()); }
                writer.write_all(&chunk.bytes).map_err(err)?; hash.update(&chunk.bytes);
                offset = offset.checked_add(chunk.bytes.len() as u64).ok_or("AttachmentIntegrity")?;
                if offset == asset.byte_length { break; }
                if chunk.bytes.is_empty() || offset > asset.byte_length { return Err("AttachmentIntegrity".into()); }
            }
            writer.flush().map_err(err)?;
            if hash.finalize().as_slice() != asset.sha256 { return Err("AttachmentIntegrity".into()); }
            Ok(file_stream::FileMetadata::new(asset.byte_length, &asset.sha256))
        })();
        let disconnected = self.host.disconnect(&connection).map_err(err);
        result.and_then(|metadata| disconnected.map(|_| metadata))
    }
    pub fn open(path: &Path) -> Result<Self> {
        // A separate development database; never open or migrate the production
        // managed library. No fallback from Workbench::open is permitted.
        if path.file_name().and_then(|n| n.to_str()) != Some("hmos-development.sqlite") {
            return Err("DevelopmentDatabaseRequired".into());
        }
        let store = Store::open(path, EventBudget::default()).map_err(err)?;
        Ok(Self {
            host: HostRuntime::new(store).map_err(err)?,
            start: std::time::Instant::now(),
            effect: "not_committed",
        })
    }
    fn ids(&self) -> Result<Vec<String>> {
        let mut ids = Vec::new();
        let mut after = String::new();
        loop {
            let page = self
                .host
                .store_local()
                .card_ids_local(&after, 64)
                .map_err(err)?;
            if page.is_empty() {
                break;
            }
            after = page.last().unwrap().clone();
            for id in page {
                let card = self
                    .host
                    .store_local()
                    .card(&id)
                    .map_err(err)?
                    .ok_or("NotFound")?;
                if editor_draft::is_journal(&card) {
                    editor_draft::validate_journal(&card).map_err(err)?;
                } else if editor_draft_staging::is_journal(&card) {
                    editor_draft_staging::validate_journal(&card).map_err(err)?;
                } else if editor_intent::is_journal(&card) {
                    editor_intent::validate_journal(&card)?;
                } else if music_bridge::is_library(&card) {
                    music_bridge::validate_library(&card)?;
                } else {
                    ids.push(id);
                }
            }
            if ids.len() > 256 {
                return Err("DevelopmentCardLimit".into());
            }
        }
        Ok(ids)
    }
    fn cards(&self) -> Result<Vec<CardView>> {
        let ids = self.ids()?;
        ids.into_iter()
            .map(|id| {
                let card = self
                    .host
                    .store_local()
                    .card(&id)
                    .map_err(err)?
                    .ok_or("NotFound")?;
                let s = card.summary();
                if s.type_id != "idea" {
                    return Err("UnsupportedCardType".into());
                }
                if s.format_version != 2 {
                    return Err("UnsupportedVersion".into());
                }
                let p = tasks_v2::decode(&id, &s.title, &card.body()).map_err(err)?;
                Ok(CardView {
                    id,
                    revision: s.revision.to_string(),
                    source: hex(&card.encode()),
                    content_kind: Some(if p.origin.is_some() { "legacy" } else { "v2" }),
                    title: s.title,
                    description: p.description,
                    hypothesis: p.hypothesis,
                    conclusion: p.conclusion,
                    category: p.category,
                    stage: p.stage,
                    favorite: p.favorite,
                    deleted: p.deleted,
                    deleted_at: p.deleted_at.to_string(),
                    assets: p.assets.iter().map(|asset| {
                        let outer = card.attachments().into_iter().find(|a| a.id == asset.id).ok_or("AttachmentMetadataMismatch")?;
                        if outer.display_name != asset.name || outer.byte_length != asset.bytes { return Err("AttachmentMetadataMismatch".into()); }
                        Ok(attachment_bridge::AssetView { id: asset.id.clone(), name: asset.name.clone(), kind: asset.kind.clone(), byte_length: asset.bytes.to_string(), sha256: hex(&outer.sha256), media_type: outer.media_type })
                    }).collect::<Result<Vec<_>>>()?,
                    tasks: p
                        .tasks
                        .into_iter()
                        .map(|t| TaskView {
                            id: t.id,
                            text: t.text,
                            completion: t.completion,
                        })
                        .collect(),
                })
            })
            .collect()
    }
    fn query_candidates(&self) -> Result<Vec<query_v2::Candidate>> {
        let ids = self.ids()?;
        let mut snapshot = self.host.store_local().open_card_snapshot().map_err(err)?;
        let mut candidates = Vec::with_capacity(ids.len());
        loop {
            let page = snapshot
                .next_page(32, morrow_core::content::MAX_RECORD_BYTES)
                .map_err(err)?;
            for entry in page.entries {
                let card = entry.card();
                if editor_draft::is_journal(&card) {
                    editor_draft::validate_journal(&card).map_err(err)?;
                    continue;
                }
                if editor_draft_staging::is_journal(&card) {
                    editor_draft_staging::validate_journal(&card).map_err(err)?;
                    continue;
                }
                if editor_intent::is_journal(&card) { editor_intent::validate_journal(&card)?; continue; }
                if music_bridge::is_library(&card) { music_bridge::validate_library(&card)?; continue; }
                if candidates.len() >= 256 {
                    return Err("DevelopmentCardLimit".into());
                }
                let summary = card.summary();
                if summary.format_version != 2 {
                    return Err("UnsupportedVersion".into());
                }
                // Use the original complete properties, never the reduced UI
                // projection. Every candidate belongs to one pinned WAL view.
                let candidate = query_v2::Candidate {
                    id: summary.id,
                    title: summary.title,
                    format_version: summary.format_version,
                    properties: card.body(),
                };
                // Validate each original body before retaining the whole bounded
                // set: 256 valid candidates each have at most 64 KiB properties.
                query_v2::sort_key(&candidate).map_err(err)?;
                candidates.push(candidate);
            }
            if page.done {
                break;
            }
        }
        snapshot.finish().map_err(err)?;
        if !candidates.iter().map(|c| &c.id).eq(ids.iter()) {
            // Discovery and snapshot acquisition observed different membership;
            // do not silently query a second source or retry a failed read.
            return Err("RevisionConflict".into());
        }
        snapshot.close().map_err(err)?;
        Ok(candidates)
    }
    fn query(&self, r: &Request) -> Result<Vec<String>> {
        let conditions = query_v2::Conditions {
            section: r.section.clone(),
            filter: r.filter.clone(),
            text: r.text.clone(),
            sort: r.sort.clone(),
        };
        query_v2::validate_request(&query_v2::Request::Filter {
            conditions: conditions.clone(),
            candidates: vec![],
        })
        .map_err(err)?;
        struct Local {
            candidates: std::vec::IntoIter<query_v2::Candidate>,
        }
        impl query_plan_v2::Backend for Local {
            fn next_candidate(&mut self) -> Result<Option<query_v2::Candidate>> {
                Ok(self.candidates.next())
            }
            fn invoke(
                &mut self,
                _: query_plan_v2::Phase,
                request: query_v2::Request,
            ) -> Result<query_v2::Response> {
                query_v2::execute(request).map_err(err)
            }
        }
        let mut local = Local {
            candidates: self.query_candidates()?.into_iter(),
        };
        query_plan_v2::execute(&conditions, &mut local)
    }
    pub fn execute(&mut self, r: Request) -> Result<Reply> {
        self.effect = "not_committed";
        music_bridge::reject_other_envelope(&r)?;
        editor_business::reject_other_envelope(&r)?;
        editor_intent::reject_other_envelope(&r)?;
        editor_handoff::reject_other_envelope(&r)?;
        if r.action == "music" { return music_bridge::execute(self, music_bridge::take_command(r)?); }
        if matches!(r.action.as_str(), "music_import" | "music_export") { return Err("MusicFileDescriptorRequired".into()); }
        if matches!(r.action.as_str(), "draft_continue_business" | "draft_continue_business_retire") { return editor_handoff::execute(self, r); }
        if r.action.starts_with("editor_intent_") { return editor_intent::execute(self, r); }
        if r.action == "editor_save" || r.action == "editor_commit_inspect" { return editor_business::execute(self, r); }
        if r.id.starts_with("morrow-host-") { return Err("HostOwnedIdentity".into()); }
        editor_intent::reject_legacy_business(self, &r)?;
        if r.action.starts_with("import_") {
            let start = self.start;
            let clock = || u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX - 1).saturating_add(1);
            let imports = match r.action.as_str() {
                "import_list" => editor_draft_staging::list(&self.host, &r.id, &r.draft_id).map_err(err)?,
                "import_inspect" => editor_draft_staging::inspect(&self.host, &r.id, &r.draft_id, &r.operation).map_err(err)?.into_iter().collect(),
                "import_begin" => vec![editor_draft_staging::begin(&mut self.host, &r.import_request.ok_or("ImportRequestRequired")?.request()?, clock, unix_millis()?, &mut self.effect).map_err(err)?],
                "import_abandon" => vec![editor_draft_staging::abandon(&mut self.host, &r.id, &r.draft_id, draft_bridge::number(&r.generation)?, &r.import_request.ok_or("ImportRequestRequired")?.operation_id, &r.operation, clock, unix_millis()?, &mut self.effect).map_err(err)?],
                "import_reconcile" => {
                    editor_draft_staging::reconcile(&mut self.host, &r.id, &r.draft_id, clock, unix_millis()?, &mut self.effect).map_err(err)?;
                    editor_draft_staging::list(&self.host, &r.id, &r.draft_id).map_err(err)?
                }
                _ => return Err("FileDescriptorRequired".into()),
            };
            return Ok(self.import_reply(imports));
        }
        if r.action == "markdown" || r.action == "paste_plain" {
            // Inert, bounded projection only. No Store lookup, host grant,
            // transaction, URL launch or filesystem access is involved.
            let (markdown, paste_text) = if r.action == "markdown" {
                (markdown::project(&r.text)?, String::new())
            } else {
                (
                    markdown::MarkdownDoc::default(),
                    markdown::paste_plain(&r.text, &r.section)?,
                )
            };
            return Ok(Reply {
                ok: true,
                error: String::new(),
                cards: vec![],
                ids: vec![],
                drafts: vec![],
                markdown,
                paste_text,
                receipt_revision: String::new(),
                profile: "development-unsealed",
                effect: self.effect, imports: vec![], editor_commit: None, editor_intents: None, intent_next_after: None, music: None,
            });
        }
        if r.action == "query" {
            // Trusted-local development read only. This neither grants guest
            // access nor records a production query/task/audit observation.
            return Ok(Reply {
                ok: true,
                error: String::new(),
                cards: vec![],
                ids: self.query(&r)?,
                drafts: vec![],
                markdown: markdown::MarkdownDoc::default(),
                paste_text: String::new(),
                receipt_revision: String::new(),
                profile: "development-unsealed",
                effect: self.effect, imports: vec![], editor_commit: None, editor_intents: None, intent_next_after: None, music: None,
            });
        }
        if r.action.starts_with("draft_") {
            let history_read = r.action == "draft_read_history";
            let start = self.start;
            let clock = || {
                u64::try_from(start.elapsed().as_millis())
                    .unwrap_or(u64::MAX - 1)
                    .saturating_add(1)
            };
            let drafts = match r.action.as_str() {
                "draft_list" => editor_draft::list(&self.host).map_err(err)?,
                "draft_read" => editor_draft::read(&self.host, &r.id, &r.draft_id)
                    .map_err(err)?
                    .into_iter()
                    .collect(),
                "draft_read_history" => {
                    let mut request = r;
                    let card = std::mem::take(&mut request.id);
                    let draft = std::mem::take(&mut request.draft_id);
                    let operation = std::mem::take(&mut request.draft_operation);
                    let generation = draft_bridge::number(&std::mem::take(&mut request.generation))?;
                    if generation == 0 || !editor_business::route_is_empty(&request) {
                        return Err("DraftHistoryOuterFields".into());
                    }
                    editor_draft::identity(&card, &draft, &operation)?;
                    let record = editor_draft::read_history(&self.host, &card, &draft, &operation)?;
                    if record.slot.generation != generation {
                        return Err("DraftHistoryGenerationMismatch".into());
                    }
                    vec![record]
                }
                "draft_save" => {
                    let request = r.draft.ok_or("DraftRequestRequired")?.request()?;
                    vec![
                        editor_draft::save_with_effect_at(
                            &mut self.host,
                            &request,
                            clock,
                            unix_millis()?,
                            &mut self.effect,
                        )
                        .map_err(err)?,
                    ]
                }
                "draft_discard" => {
                    let generation = draft_bridge::number(&r.generation)?;
                    vec![
                        editor_draft::discard_with_effect_at(
                            &mut self.host,
                            &r.id,
                            &r.draft_id,
                            generation,
                            &r.operation,
                            clock,
                            unix_millis()?,
                            &mut self.effect,
                        )
                        .map_err(err)?,
                    ]
                }
                "draft_fork" => {
                    let (request, link) = r.fork.ok_or("DraftForkRequestRequired")?.request()?;
                    vec![editor_draft::fork_with_effect_at(&mut self.host, &request, &link, clock, unix_millis()?, &mut self.effect)?]
                }
                "draft_fork_retire" => {
                    let (card, marker) = r.fork_retirement.ok_or("DraftForkRetirementRequired")?.request()?;
                    vec![editor_draft::retire_fork_with_effect_at(&mut self.host, &card, &marker, clock, unix_millis()?, &mut self.effect)?]
                }
                _ => return Err("UnsupportedAction".into()),
            };
            let reply = Reply {
                ok: true,
                error: String::new(),
                cards: vec![],
                ids: vec![],
                drafts: drafts
                    .into_iter()
                    .map(draft_bridge::View::from_record)
                    .collect::<Result<Vec<_>>>()?,
                markdown: markdown::MarkdownDoc::default(),
                paste_text: String::new(),
                receipt_revision: String::new(),
                profile: "development-unsealed",
                effect: self.effect, imports: vec![], editor_commit: None, editor_intents: None, intent_next_after: None, music: None,
            };
            if history_read && serde_json::to_vec(&reply).map_err(err)?.len() > LIMIT {
                return Err("DraftHistoryReplyBytesLimit".into());
            }
            return Ok(reply);
        }
        let mut receipt = String::new();
        if r.action != "list" {
            // A retry may already have committed before its caller received a
            // reply. Establish that outcome before connection admission or
            // current-card discovery can fail. Only a successful authoritative
            // absence lookup permits a later pre-write rejection to be reported
            // as not_committed. Core still compares the complete command below.
            self.effect = "unknown";
            let historical = match self.host.store_local()
                .lookup_for_card(&r.id, &r.operation).map_err(err)? {
                morrow_core::transaction::Lookup::Committed(_) => {
                    self.effect = "committed";
                    true
                }
                morrow_core::transaction::Lookup::Absent => {
                    self.effect = "not_committed";
                    false
                }
            };
            let start = self.start;
            let clock = || {
                u64::try_from(start.elapsed().as_millis())
                    .unwrap_or(u64::MAX - 1)
                    .saturating_add(1)
            };
            let now = clock();
            let mut connection = self.host.connect().map_err(err)?;
            let result = (|| -> Result<String> {
                if r.action == "create" {
                    if r.id.starts_with("morrow-host-") {
                        return Err("ReservedCardIdentity".into());
                    }
                    if !historical && self.ids()?.len() >= 256
                        && self.host.store_local().card(&r.id).map_err(err)?.is_none()
                    {
                        return Err("DevelopmentCardLimit".into());
                    }
                    let todos = create_todos::prepare(&r.id, &r.operation, &r.todos)?;
                    let p = tasks_v2::Properties {
                        version: 2,
                        description: r.description.clone(),
                        category: r.category.clone(),
                        stage: r.stage.clone(),
                        hypothesis: r.hypothesis.clone(),
                        conclusion: r.conclusion.clone(),
                        tasks: todos.tasks.clone(),
                        ..Default::default()
                    };
                    let published = self.published_assets(&r)?;
                    let mut body = if let Some(ref selected) = published {
                        cards_v2::apply(&r.id, &r.title, &p.encode_to_vec(), &cards_v2::Command::Edit(cards_v2::Fields {
                            title: r.title.clone(), description: r.description.clone(), hypothesis: r.hypothesis.clone(), conclusion: r.conclusion.clone(), icon: p.icon as u16, color: p.color, assets: selected.assets.clone(),
                        })).map_err(err)?.properties
                    } else { p.encode_to_vec() };
                    todos.append_raw_identity(&mut body);
                    tasks_v2::decode(&r.id, &r.title, &body).map_err(err)?;
                    let card = CardRecord::new_with_attachments(&r.id, "idea", 2, &r.title, body, &published.map(|p| p.attachments).unwrap_or_default()).map_err(err)?;
                    self.host
                        .grant(
                            &mut connection,
                            GrantKind::CreateContent,
                            &r.id,
                            now.saturating_add(60_000),
                            now,
                        )
                        .map_err(err)?;
                    if self.effect != "committed" { self.effect = "unknown"; }
                    let committed =
                        self.host
                            .create_content(&connection, &r.operation, &card, clock);
                    return self.commit_result(committed);
                }
                // Format-2 tasks are edited by TaskId commands, never by a
                // legacy newline editor projection over existing properties.
                if r.action == "edit" && !r.todos.is_empty() {
                    return Err("V2EditorTodosUnsupported".into());
                }
                let source = unhex(&r.source)?;
                let card = CardRecord::decode(&source).map_err(err)?;
                let s = card.summary();
                if s.id != r.id || s.format_version != 2 {
                    return Err("SourceMismatch".into());
                }
                let p = tasks_v2::decode(&r.id, &s.title, &card.body()).map_err(err)?;
                let published = if r.action == "edit" { self.published_assets(&r)? } else { None };
                let mut title = s.title.clone();
                let body = match r.action.as_str() {
                    "task_add" | "task_toggle" | "task_remove" | "task_rename" | "task_reorder"
                    | "task_complete_all" | "stage" => {
                        if p.deleted {
                            return Err("DeletedCard".into());
                        }
                        let cmd = match r.action.as_str() {
                            "task_add" => tasks_v2::Command::Add {
                                id: r.task_id.clone(),
                                text: r.text.clone(),
                            },
                            "task_toggle" => tasks_v2::Command::SetCompletion {
                                id: r.task_id.clone(),
                                complete: r.flag,
                            },
                            "task_remove" => tasks_v2::Command::Remove(r.task_id.clone()),
                            "task_rename" => tasks_v2::Command::Rename {
                                id: r.task_id.clone(),
                                text: r.text.clone(),
                            },
                            "task_reorder" => tasks_v2::Command::Reorder(r.order.clone()),
                            "task_complete_all" => {
                                tasks_v2::Command::CompleteAllAndSetStage(r.stage.clone())
                            }
                            _ => tasks_v2::Command::SetStage(r.stage.clone()),
                        };
                        tasks_v2::apply(&r.id, &s.title, &card.body(), cmd).map_err(err)?
                    }
                    _ => {
                        let command = match r.action.as_str() {
                            "edit" => cards_v2::Command::Edit(cards_v2::Fields {
                                title: r.title.clone(),
                                description: r.description.clone(),
                                hypothesis: r.hypothesis.clone(),
                                conclusion: r.conclusion.clone(),
                                icon: p.icon as u16,
                                color: p.color,
                                assets: published.as_ref().map(|p| p.assets.clone()).unwrap_or_else(|| p
                                    .assets
                                    .iter()
                                    .map(|a| morrow_workbench_plugin::Asset {
                                        id: a.id.clone(),
                                        name: a.name.clone(),
                                        kind: a.kind.clone(),
                                        bytes: a.bytes,
                                    })
                                    .collect()),
                            }),
                            "favorite" => cards_v2::Command::SetFavorite(r.flag),
                            "category" => cards_v2::Command::SetCategory {
                                category: r.category.clone(),
                                stage: r.stage.clone(),
                            },
                            "delete" => cards_v2::Command::Delete {
                                now_ms: r.now_ms.parse().map_err(|_| "InvalidTimestamp")?,
                            },
                            "restore" => cards_v2::Command::Restore {
                                now_ms: r.now_ms.parse().map_err(|_| "InvalidTimestamp")?,
                            },
                            _ => return Err("UnsupportedAction".into()),
                        };
                        let output = cards_v2::apply(&r.id, &s.title, &card.body(), &command)
                            .map_err(err)?;
                        title = output.title;
                        output.properties
                    }
                };
                self.host
                    .grant(
                        &mut connection,
                        GrantKind::EditContent,
                        &r.id,
                        now.saturating_add(60_000),
                        now,
                    )
                    .map_err(err)?;
                let change = VersionedContentChange {
                    operation_id: r.operation.clone(),
                    source_card: source,
                    title,
                    body,
                    preview_text: String::new(),
                    attachments: published.map(|p| p.attachments),
                };
                if self.effect != "committed" { self.effect = "unknown"; }
                let committed = self
                    .host
                    .edit_versioned_content(&connection, &change, clock);
                self.commit_result(committed)
            })();
            self.host.disconnect(&connection).map_err(err)?;
            receipt = result?;
        }
        // A read failure after commit is intentionally still an error. The caller
        // retains the SAME serialized request; never invent a fresh operation.
        Ok(Reply {
            ok: true,
            error: String::new(),
            cards: self.cards()?,
            ids: vec![],
            drafts: vec![],
            markdown: markdown::MarkdownDoc::default(),
            paste_text: String::new(),
            receipt_revision: receipt,
            profile: "development-unsealed",
            effect: self.effect, imports: vec![], editor_commit: None, editor_intents: None, intent_next_after: None, music: None,
        })
    }
    fn commit_result(
        &mut self,
        result: morrow_core::Result<morrow_core::transaction::Receipt>,
    ) -> Result<String> {
        if self.effect != "committed" { self.effect = "unknown"; }
        match result {
            Ok(receipt) => {
                self.effect = "committed";
                Ok(receipt.revision.to_string())
            }
            Err(error) => {
                use morrow_core::Error;
                if self.effect != "committed" && matches!(
                    error,
                    Error::RevisionConflict
                        | Error::UnsupportedVersion
                        | Error::Invalid(_)
                        | Error::Limit
                ) {
                    self.effect = "not_committed";
                }
                Err(error.to_string())
            }
        }
    }
}

#[cfg(test)]
#[path = "historical_retry_tests.rs"]
mod historical_retry_tests;
#[cfg(test)]
mod card_source_tests;
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn unix_millis() -> Result<i64> {
    let elapsed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(err)?;
    i64::try_from(elapsed.as_millis()).map_err(err)
}
fn unhex(text: &str) -> Result<Vec<u8>> {
    if text.len() > LIMIT || text.len() % 2 != 0 {
        return Err("InvalidSource".into());
    }
    text.as_bytes()
        .chunks_exact(2)
        .map(|p| {
            let digit = |b: u8| -> Result<u8> {
                match b {
                    b'0'..=b'9' => Ok(b - b'0'),
                    b'a'..=b'f' => Ok(b - b'a' + 10),
                    _ => Err("InvalidSource".into()),
                }
            };
            Ok(digit(p[0])? * 16 + digit(p[1])?)
        })
        .collect()
}
pub fn dispatch(input: &str) -> String {
    let result = (|| -> Result<Reply> {
        if input.len() > LIMIT {
            return Err("RequestTooLarge".into());
        }
        let mut r: Request = serde_json::from_str(input).map_err(|_| "InvalidRequest")?;
        r.transport_json = input.to_owned();
        music_bridge::reject_other_envelope(&r)?;
        editor_business::reject_other_envelope(&r)?;
        editor_intent::reject_other_envelope(&r)?;
        editor_handoff::reject_other_envelope(&r)?;
        let mut slot = SESSION.lock().map_err(|_| "SessionUnavailable")?;
        if r.action == "open" {
            if slot.is_some() {
                return Err("AlreadyOpen".into());
            }
            *slot = Some(Engine::open(Path::new(&r.path))?);
            return slot.as_mut().unwrap().execute(Request {
                action: "list".into(),
                ..Default::default()
            });
        }
        if r.action == "close" {
            *slot = None;
            return Ok(Reply {
                ok: true,
                error: String::new(),
                cards: vec![],
                ids: vec![],
                drafts: vec![],
                markdown: markdown::MarkdownDoc::default(),
                paste_text: String::new(),
                receipt_revision: String::new(),
                profile: "development-unsealed",
                effect: "not_committed", imports: vec![], editor_commit: None, editor_intents: None, intent_next_after: None, music: None,
            });
        }
        let engine = slot.as_mut().ok_or("NotOpen")?;
        match engine.execute(r) {
            Ok(reply) => Ok(reply),
            Err(message) => {
                let mut reply = Reply::failure(message);
                reply.effect = engine.effect;
                Ok(reply)
            }
        }
    })();
    serde_json::to_string(&result.unwrap_or_else(Reply::failure)).expect("serializable reply")
}

#[cfg(test)]
mod query_tests;
#[cfg(test)]
mod attachment_integration_tests;
/// C++ owns the request until return; every returned pointer must be freed once.
/// Calls must be serialized by the native owner. Never pass arbitrary pointers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn morrow_hmos_request(input: *const c_char) -> *mut c_char {
    let reply = if input.is_null() {
        serde_json::to_string(&Reply::failure("NullRequest".into())).unwrap()
    } else {
        match unsafe { CStr::from_ptr(input) }.to_str() {
            Ok(s) => dispatch(s),
            Err(_) => serde_json::to_string(&Reply::failure("InvalidUtf8".into())).unwrap(),
        }
    };
    CString::new(reply).expect("JSON has no raw NUL").into_raw()
}
/// Stateless measurement; independent of the serialized Store owner. The
/// JSON string retains escaped UTF16 surrogates for explicit strict rejection.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn morrow_hmos_editor_field(input: *const c_char) -> *mut c_char {
    let reply = if input.is_null() {
        editor_field::Reply::failure("", "EditorFieldNullRequest")
    } else {
        match unsafe { CStr::from_ptr(input) }.to_str() {
            Ok(input) => editor_field::request(input),
            Err(_) => editor_field::Reply::failure("", "EditorFieldInvalidUtf8"),
        }
    };
    CString::new(serde_json::to_string(&reply).expect("bounded field reply"))
        .expect("JSON has no raw NUL").into_raw()
}
/// Read-only complete editing proposal. Caller must still own the exact raw
/// input/IME epoch before adopting it; this never grants business-save rights.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn morrow_hmos_editor_input(input: *const c_char) -> *mut c_char {
    let reply = if input.is_null() {
        editor_input::Reply::failure("", "", 0, "EditorInputNullRequest", "")
    } else {
        match unsafe { CStr::from_ptr(input) }.to_str() {
            Ok(input) => editor_input::request(input),
            Err(_) => editor_input::Reply::failure("", "", 0, "EditorInputInvalidUtf8", ""),
        }
    };
    CString::new(serde_json::to_string(&reply).expect("bounded input reply"))
        .expect("JSON has no raw NUL").into_raw()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn morrow_hmos_free(value: *mut c_char) {
    if !value.is_null() {
        drop(unsafe { CString::from_raw(value) });
    }
}

#[cfg(unix)]
fn c_reply(reply: impl Serialize) -> *mut c_char {
    CString::new(serde_json::to_string(&reply).expect("serializable reply")).expect("JSON has no raw NUL").into_raw()
}
#[cfg(unix)]
unsafe fn take_file(fd: i32) -> Result<std::fs::File> {
    use std::os::fd::FromRawFd;
    if fd < 0 { return Err("InvalidFileDescriptor".into()); }
    // NAPI duplicates synchronously and transfers exactly one owned descriptor.
    Ok(unsafe { std::fs::File::from_raw_fd(fd) })
}
/// All passed descriptors are consumed, including invalid request/error paths.
#[cfg(unix)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn morrow_hmos_prepare(source_fd: i32, destination_fd: i32, max_bytes: u64) -> *mut c_char {
    let source = unsafe { take_file(source_fd) };
    let destination = if destination_fd == source_fd { Err("DistinctFileDescriptorsRequired".into()) } else { unsafe { take_file(destination_fd) } };
    let result = (|| -> Result<file_stream::FileMetadata> {
        let mut source = source?; let mut destination = destination?;
        use std::os::unix::fs::MetadataExt;
        use std::io::Seek;
        let source_metadata = source.metadata().map_err(err)?;
        let destination_metadata = destination.metadata().map_err(err)?;
        if source_metadata.dev() == destination_metadata.dev() && source_metadata.ino() == destination_metadata.ino() { return Err("SameFileRejected".into()); }
        if !destination_metadata.is_file() { return Err("PrivateSpoolFileRequired".into()); }
        destination.set_len(0).map_err(err)?;
        destination.rewind().map_err(err)?;
        let metadata = file_stream::prepare(&mut source, &mut destination, max_bytes)?;
        destination.sync_all().map_err(err)?; Ok(metadata)
    })();
    c_reply(file_stream::FileReply::from_result(result))
}
/// Distinct music preparation; consumes both owned descriptors on every path.
/// No pathname, provider URI or audio bytes cross the JSON request channel.
#[cfg(unix)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn morrow_hmos_music_prepare(source_fd: i32, destination_fd: i32, max_bytes: u64) -> *mut c_char {
    let source = unsafe { take_file(source_fd) };
    let destination = if destination_fd == source_fd { Err("DistinctFileDescriptorsRequired".into()) } else { unsafe { take_file(destination_fd) } };
    let result = (|| -> Result<file_stream::FileMetadata> {
        let mut source = source?; let mut destination = destination?;
        use std::{io::Seek, os::unix::fs::MetadataExt};
        if max_bytes == 0 || max_bytes > file_stream::MAX_MUSIC_IMPORT_BYTES { return Err("ImportByteLimit".into()); }
        let a = source.metadata().map_err(err)?; let b = destination.metadata().map_err(err)?;
        if !a.is_file() || !b.is_file() || a.len() == 0 { return Err("MusicRegularFileRequired".into()); }
        if a.dev() == b.dev() && a.ino() == b.ino() { return Err("SameFileRejected".into()); }
        if a.len() > max_bytes { return Err("ImportByteLimit".into()); }
        source.rewind().map_err(err)?; destination.set_len(0).map_err(err)?; destination.rewind().map_err(err)?;
        let result = file_stream::prepare_music(&mut source, &mut destination, max_bytes)?;
        destination.sync_all().map_err(err)?; Ok(result)
    })();
    c_reply(file_stream::FileReply::from_result(result))
}
#[cfg(unix)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn morrow_hmos_import(input: *const c_char, owned_fd: i32) -> *mut c_char {
    let file = unsafe { take_file(owned_fd) };
    let result = (|| -> Result<Reply> {
        let mut file = file?;
        if input.is_null() { return Err("NullRequest".into()); }
        let input = unsafe { CStr::from_ptr(input) }.to_str().map_err(|_| "InvalidUtf8")?;
        if input.len() > LIMIT { return Err("RequestTooLarge".into()); }
        let request: Request = serde_json::from_str(input).map_err(|_| "InvalidRequest")?;
        if request.action == "music_import" {
            use std::io::Seek;
            if !file.metadata().map_err(err)?.is_file() { return Err("MusicRegularFileRequired".into()); }
            file.rewind().map_err(err)?;
        }
        let mut slot = SESSION.lock().map_err(|_| "SessionUnavailable")?;
        let engine = slot.as_mut().ok_or("NotOpen")?;
        Ok(match engine.import_from(request, &mut file) { Ok(reply) => reply, Err(error) => { let mut reply = Reply::failure(error); reply.effect = engine.effect; reply } })
    })();
    c_reply(result.unwrap_or_else(Reply::failure))
}
#[cfg(unix)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn morrow_hmos_export(input: *const c_char, owned_fd: i32) -> *mut c_char {
    let file = unsafe { take_file(owned_fd) };
    let result = (|| -> Result<file_stream::FileMetadata> {
        let mut file = file?;
        use std::io::Seek;
        if !file.metadata().map_err(err)?.is_file() { return Err("PrivateExportFileRequired".into()); }
        file.set_len(0).map_err(err)?; file.rewind().map_err(err)?;
        if input.is_null() { return Err("NullRequest".into()); }
        let input = unsafe { CStr::from_ptr(input) }.to_str().map_err(|_| "InvalidUtf8")?;
        if input.len() > LIMIT { return Err("RequestTooLarge".into()); }
        let request = serde_json::from_str(input).map_err(|_| "InvalidRequest")?;
        let mut slot = SESSION.lock().map_err(|_| "SessionUnavailable")?;
        let metadata = slot.as_mut().ok_or("NotOpen")?.export_to(request, &mut file)?;
        file.sync_all().map_err(err)?; Ok(metadata)
    })();
    c_reply(file_stream::FileReply::from_result(result))
}

/// Read-only conversion consumes a duplicate source FD, rewinds it, and binds
/// every result to the complete immutable spool SHA. It needs no open Store.
#[cfg(unix)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn morrow_hmos_clipboard_convert(input: *const c_char, source_fd: i32) -> *mut c_char {
    let source = unsafe { take_file(source_fd) };
    let result = (|| -> Result<clipboard::ConvertReply> {
        let mut source = source?;
        use std::io::Seek;
        if !source.metadata().map_err(err)?.is_file() { return Err("ClipboardSpoolFileRequired".into()); }
        if input.is_null() { return Err("NullRequest".into()); }
        let input = unsafe { CStr::from_ptr(input) }.to_str().map_err(|_| "InvalidUtf8")?;
        source.rewind().map_err(err)?;
        clipboard::convert(input, &mut source)
    })();
    c_reply(result.unwrap_or_else(clipboard::ConvertReply::failure))
}

#[cfg(unix)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn morrow_hmos_clipboard_image(input: *const c_char, source_fd: i32, destination_fd: i32, max_bytes: u64) -> *mut c_char {
    let source = unsafe { take_file(source_fd) };
    let destination = if destination_fd == source_fd { Err("DistinctFileDescriptorsRequired".into()) } else { unsafe { take_file(destination_fd) } };
    let result = (|| -> Result<file_stream::FileMetadata> {
        let mut source = source?; let mut destination = destination?;
        use std::{io::Seek, os::unix::fs::MetadataExt};
        let a = source.metadata().map_err(err)?; let b = destination.metadata().map_err(err)?;
        if !a.is_file() || !b.is_file() { return Err("ClipboardSpoolFileRequired".into()); }
        if a.dev() == b.dev() && a.ino() == b.ino() { return Err("SameFileRejected".into()); }
        if input.is_null() { return Err("NullRequest".into()); }
        let input = unsafe { CStr::from_ptr(input) }.to_str().map_err(|_| "InvalidUtf8")?;
        source.rewind().map_err(err)?;
        // Parse, recheck the whole source and identify the bounded image before
        // mutating the private destination, including its previous length.
        let image = clipboard::extract_bytes(input, &mut source, max_bytes)?;
        destination.set_len(0).map_err(err)?; destination.rewind().map_err(err)?;
        let metadata = file_stream::prepare(&mut image.as_slice(), &mut destination, max_bytes)?;
        destination.sync_all().map_err(err)?; Ok(metadata)
    })();
    c_reply(file_stream::FileReply::from_result(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn req(value: serde_json::Value) -> Request {
        serde_json::from_value(value).unwrap()
    }
    fn create(e: &mut Engine) -> Reply {
        e.execute(req(serde_json::json!({"action":"create","operation":"create-1","id":"c1","title":"卡片","category":"灵感","stage":"待整理"}))).unwrap()
    }
    #[test]
    fn persistence_cas_exact_retry_and_duplicate_task_names() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hmos-development.sqlite");
        let mut e = Engine::open(&path).unwrap();
        let initial = create(&mut e);
        let add = serde_json::json!({"action":"task_add","operation":"add-1","id":"c1","source":initial.cards[0].source,"task_id":"task-1","text":"同名"});
        let one = e.execute(req(add.clone())).unwrap();
        let two=e.execute(req(serde_json::json!({"action":"task_add","operation":"add-2","id":"c1","source":one.cards[0].source,"task_id":"task-2","text":"同名"}))).unwrap();
        let done=e.execute(req(serde_json::json!({"action":"task_toggle","operation":"toggle-1","id":"c1","source":two.cards[0].source,"task_id":"task-1","flag":true}))).unwrap();
        assert_eq!(done.cards[0].tasks[0].completion, 1);
        assert_eq!(done.cards[0].tasks[1].completion, 0);
        let retry = e.execute(req(add.clone())).unwrap();
        assert_eq!(retry.receipt_revision, "2");
        assert_eq!(retry.cards[0].revision, "4");
        let mut stale = add;
        stale["operation"] = serde_json::json!("stale");
        assert!(
            e.execute(req(stale))
                .unwrap_err()
                .contains("RevisionConflict")
        );
        drop(e);
        let mut e = Engine::open(&path).unwrap();
        assert_eq!(
            e.execute(req(serde_json::json!({"action":"list"})))
                .unwrap()
                .cards[0]
                .revision,
            "4"
        );
        e.host.store_local().integrity_check().unwrap();
    }
    #[test]
    fn task_edits_keep_identity_validate_order_and_replay_after_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hmos-development.sqlite");
        let mut e = Engine::open(&path).unwrap();
        let mut card = create(&mut e).cards.remove(0);
        for id in ["a", "b"] {
            card = e.execute(req(serde_json::json!({"action":"task_add","operation":format!("add-{id}"),"id":"c1","source":card.source,"task_id":id,"text":"同名"}))).unwrap().cards.remove(0);
        }
        card = e.execute(req(serde_json::json!({"action":"task_toggle","operation":"toggle-a","id":"c1","source":card.source,"task_id":"a","flag":true}))).unwrap().cards.remove(0);
        let rename = serde_json::json!({"action":"task_rename","operation":"rename-b","id":"c1","source":card.source,"task_id":"b","text":"修改后的步骤"});
        card = e.execute(req(rename.clone())).unwrap().cards.remove(0);
        assert_eq!(card.tasks[0].text, "同名");
        assert_eq!(card.tasks[1].id, "b");
        assert_eq!(card.tasks[1].completion, 0);
        let reorder = serde_json::json!({"action":"task_reorder","operation":"reorder","id":"c1","source":card.source,"order":["b","a"]});
        card = e.execute(req(reorder.clone())).unwrap().cards.remove(0);
        assert_eq!(card.tasks[0].text, "修改后的步骤");
        assert_eq!(card.tasks[0].completion, 0);
        assert_eq!(card.tasks[1].completion, 1);
        assert_eq!(card.stage, "待整理");
        for (index, invalid) in [
            serde_json::json!({"action":"task_reorder","order":["a","a"]}),
            serde_json::json!({"action":"task_reorder","order":["a"]}),
            serde_json::json!({"action":"task_reorder","order":["a","missing"]}),
            serde_json::json!({"action":"task_rename","task_id":"missing","text":"x"}),
            serde_json::json!({"action":"task_rename","task_id":"b","text":""}),
            serde_json::json!({"action":"task_complete_all","stage":"已完成"}),
        ]
        .into_iter()
        .enumerate()
        {
            let mut invalid = invalid;
            invalid["id"] = "c1".into();
            invalid["operation"] = format!("invalid-{index}").into();
            invalid["source"] = card.source.clone().into();
            assert!(e.execute(req(invalid)).is_err());
            assert_eq!(e.effect, "not_committed");
            assert_eq!(e.cards().unwrap()[0].source, card.source);
        }
        let complete = serde_json::json!({"action":"task_complete_all","operation":"complete","id":"c1","source":card.source,"stage":"已整理"});
        card = e.execute(req(complete.clone())).unwrap().cards.remove(0);
        assert!(card.tasks.iter().all(|t| t.completion == 1));
        assert_eq!(card.stage, "已整理");
        let mut stale = reorder.clone();
        stale["operation"] = "stale-reorder".into();
        assert!(
            e.execute(req(stale))
                .unwrap_err()
                .contains("RevisionConflict")
        );
        drop(e);
        let mut e = Engine::open(&path).unwrap();
        for (original, revision) in [(rename, "5"), (reorder, "6"), (complete, "7")] {
            let replay = e.execute(req(original)).unwrap();
            assert_eq!(replay.receipt_revision, revision);
            assert_eq!(replay.cards[0].source, card.source);
        }
        let deleted = e.execute(req(serde_json::json!({"action":"delete","operation":"delete","id":"c1","source":card.source,"now_ms":"1000"}))).unwrap().cards.remove(0);
        for action in ["task_rename", "task_reorder", "task_complete_all"] {
            assert_eq!(e.execute(req(serde_json::json!({"action":action,"operation":action,"id":"c1","source":deleted.source,"task_id":"a","text":"x","order":["a","b"],"stage":"已整理"}))).unwrap_err(), "DeletedCard");
            assert_eq!(e.effect, "not_committed");
        }
        e.host.store_local().integrity_check().unwrap();
    }
    #[test]
    fn production_path_and_malformed_input_are_rejected() {
        assert!(Engine::open(Path::new("production.sqlite")).is_err());
        assert!(dispatch("{bad").contains("InvalidRequest"));
        assert!(unhex("é").is_err());
        assert!(unhex("00f").is_err());
    }
    #[test]
    fn invalid_edit_does_not_write_and_unknown_is_never_cleared() {
        let dir = tempfile::tempdir().unwrap();
        let mut e = Engine::open(&dir.path().join("hmos-development.sqlite")).unwrap();
        let original = create(&mut e);
        let invalid = req(
            serde_json::json!({"action":"edit","operation":"invalid","id":"c1","source":original.cards[0].source,"title":""}),
        );
        assert!(e.execute(invalid).is_err());
        assert_eq!(e.effect, "not_committed");
        assert_eq!(e.cards().unwrap()[0].revision, "1");
        assert!(
            e.commit_result(Err(morrow_core::Error::CommitUnknown))
                .is_err()
        );
        assert_eq!(e.effect, "unknown");
        assert!(e.commit_result(Err(morrow_core::Error::Storage)).is_err());
        assert_eq!(e.effect, "unknown");
    }
    #[test]
    fn markdown_and_plain_paste_are_read_only_and_have_complete_json_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hmos-development.sqlite");
        let mut e = Engine::open(&path).unwrap();
        let original = create(&mut e).cards.remove(0);
        assert_eq!(e.effect, "committed");
        let projected = e.execute(req(serde_json::json!({"action":"markdown","text":"## Preview😀\n\n**bold** [safe](https://example.com)"}))).unwrap();
        assert_eq!(projected.effect, "not_committed");
        assert!(
            projected.cards.is_empty() && projected.ids.is_empty() && projected.drafts.is_empty()
        );
        assert!(projected.paste_text.is_empty() && projected.receipt_revision.is_empty());
        let wire = serde_json::to_value(projected).unwrap();
        let block = &wire["markdown"]["blocks"][0];
        for key in [
            "kind",
            "runs",
            "level",
            "indent",
            "quote",
            "marker",
            "language",
            "rows",
            "alignments",
        ] {
            assert!(block.get(key).is_some(), "{key}");
        }
        for key in ["text", "bold", "italic", "strike", "code", "href", "image"] {
            assert!(block["runs"][0].get(key).is_some(), "{key}");
        }
        assert_eq!(block["level"], 2);
        assert_eq!(block["runs"][0]["text"], "Preview😀");
        let pasted = e.execute(req(serde_json::json!({"action":"paste_plain","section":"description","text":"A\tB\nC\tD"}))).unwrap();
        assert_eq!(pasted.effect, "not_committed");
        assert!(pasted.markdown.blocks.is_empty());
        assert_eq!(pasted.paste_text, "| A | B |\n| --- | --- |\n| C | D |");
        for invalid in [
            serde_json::json!({"action":"markdown","text":"😀".repeat(20_001)}),
            serde_json::json!({"action":"paste_plain","section":"description","text":"A\tB\n".repeat(501)}),
            serde_json::json!({"action":"paste_plain","section":"unknown","text":"x"}),
        ] {
            assert!(e.execute(req(invalid)).is_err());
            assert_eq!(e.effect, "not_committed");
        }
        assert_eq!(e.cards().unwrap()[0].source, original.source);
        let business = serde_json::to_value(
            e.execute(req(serde_json::json!({"action":"list"})))
                .unwrap(),
        )
        .unwrap();
        assert_eq!(business["markdown"], serde_json::json!({"blocks":[]}));
        assert_eq!(business["paste_text"], "");
        assert_eq!(
            serde_json::to_value(Reply::failure("test".into())).unwrap()["markdown"],
            serde_json::json!({"blocks":[]})
        );
        e.host.store_local().integrity_check().unwrap();
        drop(e);
        let reopened = Engine::open(&path).unwrap();
        assert_eq!(reopened.cards().unwrap()[0].source, original.source);
    }
    #[test]
    fn delete_undo_window_and_failed_restore_preserve_the_original() {
        let dir = tempfile::tempdir().unwrap();
        let mut e = Engine::open(&dir.path().join("hmos-development.sqlite")).unwrap();
        let original = create(&mut e);
        let deleted = e.execute(req(serde_json::json!({"action":"delete","operation":"delete","id":"c1","source":original.cards[0].source,"now_ms":"1000"}))).unwrap();
        assert!(deleted.cards[0].deleted);
        assert!(e.execute(req(serde_json::json!({"action":"restore","operation":"expired","id":"c1","source":deleted.cards[0].source,"now_ms":"9000"}))).is_err());
        assert_eq!(e.effect, "not_committed");
        assert_eq!(e.cards().unwrap()[0].source, deleted.cards[0].source);
        let restored = e.execute(req(serde_json::json!({"action":"restore","operation":"undo","id":"c1","source":deleted.cards[0].source,"now_ms":"8999"}))).unwrap();
        assert!(!restored.cards[0].deleted);
        e.host.store_local().integrity_check().unwrap();
    }
}
