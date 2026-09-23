//! HMOS trusted local development adapter. The production Workbench remains
//! gated until the Harmony HUKS/audit/lease backend is qualified.
use morrow_core::{
    content::CardRecord,
    dispatch::HostRuntime,
    lifecycle::GrantKind,
    store::{EventBudget, Store},
    versioned_content_change::VersionedContentChange,
};
use morrow_workbench_plugin::{cards_v2, tasks_v2};
use prost::Message;
use serde::{Deserialize, Serialize};
use std::{
    ffi::{CStr, CString, c_char},
    path::Path,
    sync::Mutex,
};

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
    category: String,
    stage: String,
    task_id: String,
    text: String,
    flag: bool,
    now_ms: String,
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
}
#[derive(Debug, Serialize)]
pub struct Reply {
    ok: bool,
    error: String,
    cards: Vec<CardView>,
    receipt_revision: String,
    profile: &'static str,
    effect: &'static str,
}
impl Reply {
    fn failure(message: String) -> Self {
        Self {
            ok: false,
            error: message,
            cards: vec![],
            receipt_revision: String::new(),
            profile: "development-unsealed",
            effect: "unknown",
        }
    }
}

pub struct Engine {
    host: HostRuntime,
    start: std::time::Instant,
    effect: &'static str,
}
impl Engine {
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
            ids.extend(page);
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
                if s.format_version != 2 {
                    return Err("UnsupportedVersion".into());
                }
                let p = tasks_v2::decode(&id, &s.title, &card.body()).map_err(err)?;
                Ok(CardView {
                    id,
                    revision: s.revision.to_string(),
                    source: hex(&card.encode()),
                    title: s.title,
                    description: p.description,
                    hypothesis: p.hypothesis,
                    conclusion: p.conclusion,
                    category: p.category,
                    stage: p.stage,
                    favorite: p.favorite,
                    deleted: p.deleted,
                    deleted_at: p.deleted_at.to_string(),
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
    pub fn execute(&mut self, r: Request) -> Result<Reply> {
        self.effect = "not_committed";
        let mut receipt = String::new();
        if r.action != "list" {
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
                    if self.ids()?.len() >= 256
                        && self.host.store_local().card(&r.id).map_err(err)?.is_none()
                    {
                        return Err("DevelopmentCardLimit".into());
                    }
                    let p = tasks_v2::Properties {
                        version: 2,
                        description: r.description.clone(),
                        category: r.category.clone(),
                        stage: r.stage.clone(),
                        hypothesis: r.hypothesis.clone(),
                        conclusion: r.conclusion.clone(),
                        ..Default::default()
                    };
                    let body = p.encode_to_vec();
                    tasks_v2::decode(&r.id, &r.title, &body).map_err(err)?;
                    let card = CardRecord::new(&r.id, "idea", 2, &r.title, body).map_err(err)?;
                    self.host
                        .grant(
                            &mut connection,
                            GrantKind::CreateContent,
                            &r.id,
                            now.saturating_add(60_000),
                            now,
                        )
                        .map_err(err)?;
                    self.effect = "unknown";
                    let committed =
                        self.host
                            .create_content(&connection, &r.operation, &card, clock);
                    return self.commit_result(committed);
                }
                let source = unhex(&r.source)?;
                let card = CardRecord::decode(&source).map_err(err)?;
                let s = card.summary();
                if s.id != r.id || s.format_version != 2 {
                    return Err("SourceMismatch".into());
                }
                let p = tasks_v2::decode(&r.id, &s.title, &card.body()).map_err(err)?;
                let mut title = s.title.clone();
                let body = match r.action.as_str() {
                    "task_add" | "task_toggle" | "task_remove" | "task_rename" | "stage" => {
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
                                assets: p
                                    .assets
                                    .iter()
                                    .map(|a| morrow_workbench_plugin::Asset {
                                        id: a.id.clone(),
                                        name: a.name.clone(),
                                        kind: a.kind.clone(),
                                        bytes: a.bytes,
                                    })
                                    .collect(),
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
                    attachments: None,
                };
                self.effect = "unknown";
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
            receipt_revision: receipt,
            profile: "development-unsealed",
            effect: self.effect,
        })
    }
    fn commit_result(
        &mut self,
        result: morrow_core::Result<morrow_core::transaction::Receipt>,
    ) -> Result<String> {
        self.effect = "unknown";
        match result {
            Ok(receipt) => {
                self.effect = "committed";
                Ok(receipt.revision.to_string())
            }
            Err(error) => {
                use morrow_core::Error;
                if matches!(
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
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
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
        let r: Request = serde_json::from_str(input).map_err(|_| "InvalidRequest")?;
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
                receipt_revision: String::new(),
                profile: "development-unsealed",
                effect: "not_committed",
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
#[unsafe(no_mangle)]
pub unsafe extern "C" fn morrow_hmos_free(value: *mut c_char) {
    if !value.is_null() {
        drop(unsafe { CString::from_raw(value) });
    }
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
