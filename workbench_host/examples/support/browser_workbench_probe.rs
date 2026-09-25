//! Identical native/browser qualification over the real binary host protocol.
use capnp::{message::{Builder, ReaderOptions}, serialize};
use morrow_workbench_host::{host_capnp as wire, editor_draft_api_capnp as draft, protocol, Result};
use morrow_workbench_plugin::{Action, Idea, Request, codec};
use sha2::{Digest, Sha256};
const CARD: &str = "browser-parity-card";
const DRAFT: &str = "browser-parity-draft";
type Message = capnp::message::Reader<serialize::OwnedSegments>;
type Transport<'a> = dyn FnMut(&[u8]) -> Result<Vec<u8>> + 'a;
fn decode(bytes: &[u8]) -> Result<Message> {
    Ok(serialize::read_message(&mut &*bytes, ReaderOptions::new())?)
}
fn send(transport: &mut Transport<'_>, action: wire::Action, fill: impl FnOnce(wire::request::Builder<'_>)) -> Result<Message> {
    let mut message = Builder::new_default();
    let mut out = message.init_root::<wire::request::Builder>();
    out.set_version(1); out.set_digest(&protocol::digest()); out.set_action(action); fill(out);
    let reply = transport(&serialize::write_message_to_words(&message))?;
    let message = decode(&reply)?;
    let r = message.get_root::<wire::response::Reader>()?;
    if r.get_version() != 1 || r.get_digest()? != protocol::digest() { return Err("host response contract".into()); }
    Ok(message)
}
fn ok(message: &Message) -> Result<wire::response::Reader<'_>> {
    let r = message.get_root::<wire::response::Reader>()?;
    let error = r.get_error()?.to_str()?;
    if !error.is_empty() { return Err(error.to_owned().into()); }
    Ok(r)
}
fn check(value: bool, label: &str) -> Result<()> { if value {Ok(())} else {Err(label.to_owned().into())} }
fn mutate(transport: &mut Transport<'_>, action: Action, revision: u64, operation: &str, title: &str) -> Result<Message> {
    let payload = codec::encode_request(&Request {action,current:Idea::default(), proposed:Idea {
        id:CARD.into(),title:title.into(),description:"本地正文😀".into(),category:"进行中".into(),stage:"计划中".into(),todos:vec!["本地任务".into()],..Default::default()
    },text:String::new(),flag:false,now_ms:0,ideas:vec![],section:String::new(),filter:String::new(),sort:String::new()})?;
    send(transport,wire::Action::Mutate, |mut r| {r.set_id(CARD); r.set_revision(revision); r.set_operation(operation); r.set_payload(&payload);})
}
fn text(mut b: draft::text_value::Builder<'_>, value: &str) {
    b.set_text(value); let n=value.encode_utf16().count() as i32;
    b.set_selection_base(n);b.set_selection_extent(n);b.set_composing_start(-1);b.set_composing_end(-1);
}
fn draft_bytes(raw: &str) -> Vec<u8> {
    let mut message=Builder::new_default();let mut r=message.init_root::<draft::write_request::Builder>();
    r.set_version(1);r.set_digest(&Sha256::digest(include_str!("../../schemas/editor_draft_api.capnp").replace("\r\n","\n").as_bytes()));
    r.set_card_id(CARD);r.set_draft_id(DRAFT);r.set_operation("browser-draft-save");r.set_expected_generation(0);r.set_source_revision(2);r.set_source_kind(0);
    let mut values=r.init_values();text(values.reborrow().init_title(),"");text(values.reborrow().init_description(),raw);
    text(values.reborrow().init_hypothesis(),"");text(values.reborrow().init_conclusion(),"");text(values.reborrow().init_todos(),"");
    values.set_category("进行中");values.set_stage("计划中");serialize::write_message_to_words(&message)
}
fn download(transport:&mut Transport<'_>, first:&Message)->Result<Vec<u8>> {
    let r=ok(first)?;let token=r.get_transfer()?.to_str()?.to_owned();let total=r.get_total_length() as usize;
    check(total>0 && total<=4*1024*1024,"draft download budget")?;
    let sha=r.get_sha256()?.to_vec();let mut bytes=r.get_payload()?.to_vec();
    while bytes.len()<total {
        let offset=bytes.len();let m=send(transport,wire::Action::ReadEditorDraftPart,|mut r|{r.set_transfer(&token);r.set_offset(offset as u64);})?;
        let r=ok(&m)?;check(r.get_offset()==offset as u64 && r.get_total_length()==total as u64 && r.get_sha256()?==sha,"draft part binding")?;
        let payload=r.get_payload()?;check(!payload.is_empty() && bytes.len()+payload.len()<=total,"draft part progress")?;bytes.extend_from_slice(payload);
    }
    check(bytes.len()==total && Sha256::digest(&bytes).as_slice()==sha,"draft content hash")?;Ok(bytes)
}
pub fn run(transport: &mut Transport<'_>, restore: bool) -> Result<()> {
    let raw="未完成的本地草稿😀\n".repeat(13000);
    if !restore {
        check(ok(&mutate(transport,Action::Create,0,"browser-create","原始标题")?)?.get_revision()==1,"create revision")?;
        check(ok(&mutate(transport,Action::Edit,1,"browser-edit","编辑后标题")?)?.get_revision()==2,"edit revision")?;
        let stale=mutate(transport,Action::Edit,1,"browser-stale","不应覆盖")?;
        check(!stale.get_root::<wire::response::Reader>()?.get_error()?.is_empty(),"stale write was accepted")?;
        let bytes=draft_bytes(&raw);
        let m=send(transport,wire::Action::BeginEditorDraft,|mut r|{r.set_operation("browser-draft-save");r.set_total_length(bytes.len() as u64);r.set_sha256(&Sha256::digest(&bytes));})?;
        let token=ok(&m)?.get_transfer()?.to_str()?.to_owned();
        for (index,part) in bytes.chunks(32768).enumerate(){
            ok(&send(transport,wire::Action::AppendEditorDraft,|mut r|{r.set_transfer(&token);r.set_offset((index*32768) as u64);r.set_payload(part);})?)?;
        }
        let m=send(transport,wire::Action::FinishEditorDraft,|mut r|{r.set_transfer(&token);r.set_id(CARD);r.set_attachment(DRAFT);r.set_operation("browser-draft-save");})?;
        download(transport,&m)?;
        ok(&send(transport,wire::Action::SaveUiLocale,|mut r|{r.set_operation("browser-locale");r.set_payload(b"zh");})?)?;
    }
    let m=send(transport,wire::Action::Read,|mut r|r.set_id(CARD))?;let r=ok(&m)?;
    let card=codec::decode_response(r.get_payload()?)?.idea;
    check(r.get_revision()==2 && card.title=="编辑后标题" && card.description=="本地正文😀" && card.todos==["本地任务"],"card persistence/content conflict")?;
    let m=send(transport,wire::Action::ReadEditorDraft,|mut r|{r.set_id(CARD);r.set_attachment(DRAFT);})?;
    let bytes=download(transport,&m)?;let m=decode(&bytes)?;let envelope=m.get_root::<draft::envelope::Reader>()?;
    let record=envelope.get_record()?;let values=record.get_request()?.get_values()?;
    check(record.get_generation()==1 && record.get_current_generation()==1 && record.get_active(),"draft generation")?;
    check(values.get_description()?.get_text()?.to_str()?==raw && values.get_description()?.get_selection_extent()==raw.encode_utf16().count() as i32,"draft Unicode/content/selection")?;
    check(ok(&send(transport,wire::Action::ReadUiLocale, |_|{})?)?.get_payload()?==b"zh","local locale persistence")?;
    check(ok(&send(transport,wire::Action::PluginState, |_|{})?)?.get_plugin_enabled(),"bundled plugin approval persistence")?;
    Ok(())
}
