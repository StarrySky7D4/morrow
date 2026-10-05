//! Opaque native owners. Caller memory validity is not a pointer sandbox.
#![allow(clippy::missing_safety_doc)]
use crate::{Action,ActionKind,ClientState,Error,Opened,Reply,Request,Response,Result,Status,VERSION};
use std::{mem::{align_of,size_of},ptr};
pub struct MdrRequest(Request);
pub struct MdrResponse(Response);
pub struct MdrClient(ClientState);
#[repr(C)]
#[derive(Clone,Copy)]
pub struct RequestView {pub abi_version:u32,pub struct_size:u32,pub action:u32,pub has_after_entry_id:u32,pub nomination_ref:[u8;32],pub nonce:[u8;32],pub selection_epoch:[u8;32],pub after_entry_id:[u8;32],pub page_sequence:u64,pub reserved:[u32;2]}
#[repr(C)]
#[derive(Clone,Copy)]
pub struct ReplyView {pub abi_version:u32,pub struct_size:u32,pub kind:u32,pub status:u32,pub selection_epoch:[u8;32],pub page_sequence:u64,pub metadata_bytes:u64,pub entries:u32,pub reserved:u32,pub page_wire:*const u8,pub page_wire_length:u32,pub reserved2:u32}
#[repr(C)]
#[derive(Clone,Copy)]
pub struct ResponseView {pub abi_version:u32,pub struct_size:u32,pub action:u32,pub reserved:u32,pub nomination_ref:[u8;32],pub nonce:[u8;32],pub request_digest:[u8;32],pub reply:ReplyView}
#[repr(C)]
#[derive(Clone,Copy)]
pub struct Snapshot {pub abi_version:u32,pub struct_size:u32,pub phase:u32,pub reserved:u32,pub calls:u64,pub request_wire_bytes:u64,pub response_wire_bytes:u64,pub next_sequence:u64,pub resident_nonces:u64}
pub type Transport=unsafe extern "C" fn(*mut std::ffi::c_void,*const u8,u32,*mut u8,u32)->i32;
fn status(result:Result<()>)->u32 {match result{Ok(())=>0,Err(e)=>e as u32}}
fn range(address:usize,length:usize)->Result<(usize,usize)> {Ok((address,address.checked_add(length).ok_or(Error::Buffer)?))}
fn overlap(a:(usize,usize),b:(usize,usize))->bool {a.0<b.1&&b.0<a.1}
fn aligned<T>(p:*const T)->Result<()> {if p.is_null()||!p.addr().is_multiple_of(align_of::<T>()){Err(Error::Buffer)}else{range(p.addr(),size_of::<T>())?;Ok(())}}
fn output<T>(p:*mut T,n:u32)->Result<()> {aligned(p)?;if (n as usize)<size_of::<T>(){Err(Error::Buffer)}else{Ok(())}}
fn disjoint(address:usize,length:usize,forbidden:&[(usize,usize)])->Result<()> {let area=range(address,length)?;for r in forbidden{if overlap(area,*r){return Err(Error::Buffer);}}Ok(())}
fn request_ranges(r:&MdrRequest)->Result<Vec<(usize,usize)>> {Ok(vec![range(ptr::from_ref(r).addr(),size_of::<MdrRequest>())?,range(r.0.wire().as_ptr().addr(),r.0.wire().len())?])}
fn response_ranges(r:&MdrResponse)->Result<Vec<(usize,usize)>> {let mut ranges=vec![range(ptr::from_ref(r).addr(),size_of::<MdrResponse>())?,range(r.0.wire().as_ptr().addr(),r.0.wire().len())?,range(r.0.request().wire().as_ptr().addr(),r.0.request().wire().len())?];if let Some(p)=r.0.page_wire(){ranges.push(range(p.as_ptr().addr(),p.len())?);}Ok(ranges)}
unsafe fn bytes<'a>(p:*const u8,n:u32,max:usize)->Result<&'a[u8]> {if n as usize>max{return Err(Error::Limit);}if n==0{return Ok(&[]);}if p.is_null(){return Err(Error::Buffer);}range(p.addr(),n as usize)?;
 // SAFETY: caller provides readable storage for the checked bounded byte length.
 Ok(unsafe{std::slice::from_raw_parts(p,n as usize)})}
fn view_action(v:RequestView)->Result<Action> {
 if v.abi_version!=u32::from(VERSION)||v.struct_size as usize!=size_of::<RequestView>()||v.reserved!=[0;2]||v.has_after_entry_id>1{return Err(Error::Contract);}
 let kind=ActionKind::try_from(u16::try_from(v.action).map_err(|_|Error::Invalid)?)?;
 let zero_epoch=v.selection_epoch==[0;32];let zero_after=v.after_entry_id==[0;32];
 if v.has_after_entry_id==0&&!zero_after{return Err(Error::Invalid);}
 let action=match kind {
  ActionKind::Open=>{if !zero_epoch||v.page_sequence!=0||v.has_after_entry_id!=0||!zero_after{return Err(Error::Invalid);}Action::Open},
  ActionKind::Next=>Action::Next{selection_epoch:v.selection_epoch,page_sequence:v.page_sequence,after_entry_id:if v.has_after_entry_id==1{Some(v.after_entry_id)}else{None}},
  ActionKind::Finish|ActionKind::Cancel=>{if v.page_sequence!=0||v.has_after_entry_id!=0||!zero_after{return Err(Error::Invalid);}if kind==ActionKind::Finish{Action::Finish{selection_epoch:v.selection_epoch}}else{Action::Cancel{selection_epoch:v.selection_epoch}}},
 };action.validate()?;Ok(action)
}
fn request_view(r:&Request)->RequestView {let mut v=RequestView{abi_version:1,struct_size:size_of::<RequestView>() as u32,action:r.action().kind() as u32,has_after_entry_id:0,nomination_ref:r.nomination_ref(),nonce:r.nonce(),selection_epoch:[0;32],after_entry_id:[0;32],page_sequence:0,reserved:[0;2]};match r.action(){Action::Open=>{},Action::Next{selection_epoch,page_sequence,after_entry_id}=>{v.selection_epoch=selection_epoch;v.page_sequence=page_sequence;if let Some(id)=after_entry_id{v.has_after_entry_id=1;v.after_entry_id=id;}},Action::Finish{selection_epoch}|Action::Cancel{selection_epoch}=>v.selection_epoch=selection_epoch}v}
fn reply_view(r:&Response)->ReplyView {let mut v=ReplyView{abi_version:1,struct_size:size_of::<ReplyView>() as u32,kind:0,status:0,selection_epoch:[0;32],page_sequence:0,metadata_bytes:0,entries:0,reserved:0,page_wire:ptr::null(),page_wire_length:0,reserved2:0};match r.reply(){Reply::Opened(o)=>{v.selection_epoch=o.selection_epoch;v.page_sequence=o.page_sequence;v.entries=o.entries;v.metadata_bytes=o.metadata_bytes;},Reply::Page(page)=>{v.kind=1;v.selection_epoch=page.selection_epoch;v.page_sequence=page.page_sequence;let raw=r.page_wire().expect("Page response owns page wire");v.page_wire=raw.as_ptr();v.page_wire_length=raw.len() as u32;},Reply::Finished=>v.kind=2,Reply::Cancelled=>v.kind=3,Reply::Error(s)=>{v.kind=4;v.status=*s as u32;}}v}
unsafe fn view_reply(v:ReplyView)->Result<Reply> {
 if v.abi_version!=1||v.struct_size as usize!=size_of::<ReplyView>()||v.reserved!=0||v.reserved2!=0{return Err(Error::Contract);}
 let empty=v.selection_epoch==[0;32]&&v.page_sequence==0&&v.metadata_bytes==0&&v.entries==0&&v.page_wire.is_null()&&v.page_wire_length==0;
 match v.kind {
  0=>{if v.status!=0||!v.page_wire.is_null()||v.page_wire_length!=0{return Err(Error::Invalid);}let o=Opened{selection_epoch:v.selection_epoch,page_sequence:v.page_sequence,after_entry_id:None,entries:v.entries,metadata_bytes:v.metadata_bytes};o.validate()?;Ok(Reply::Opened(o))},
  1=>{if v.status!=0||v.metadata_bytes!=0||v.entries!=0{return Err(Error::Invalid);}
   // SAFETY: validated input view supplies readable bounded page bytes.
   let raw=unsafe{bytes(v.page_wire,v.page_wire_length,crate::MAX_PAGE_BYTES)}?;let page=crate::FsDirectoryPage::decode(raw).map_err(crate::page_error)?;if page.selection_epoch!=v.selection_epoch||page.page_sequence!=v.page_sequence{return Err(Error::Correlation);}Ok(Reply::Page(page))},
  2|3=>{if !empty||v.status!=0{return Err(Error::Invalid);}if v.kind==2{Ok(Reply::Finished)}else{Ok(Reply::Cancelled)}},
  4=>{if !empty{return Err(Error::Invalid);}Ok(Reply::Error(Status::try_from(u16::try_from(v.status).map_err(|_|Error::Invalid)?)?))},
  _=>Err(Error::Invalid),
 }
}
unsafe fn copy_wire(raw:&[u8],out:*mut u8,capacity:u32,length:*mut u32,forbidden:&[(usize,usize)])->Result<()> {
 aligned(length)?;if out.is_null()||(capacity as usize)<raw.len(){return Err(Error::Buffer);}let area=range(out.addr(),capacity as usize)?;let len_area=range(length.addr(),size_of::<u32>())?;if overlap(area,len_area){return Err(Error::Buffer);}disjoint(out.addr(),capacity as usize,forbidden)?;disjoint(length.addr(),size_of::<u32>(),forbidden)?;
 // SAFETY: disjoint writable caller buffers and live bounded source checked above.
 unsafe{ptr::copy_nonoverlapping(raw.as_ptr(),out,raw.len());ptr::write(length,raw.len() as u32);}Ok(())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mdr_request_set(v:*const RequestView,n:u32,out:*mut *mut MdrRequest)->u32 {status((||{aligned(v)?;aligned(out)?;if n as usize!=size_of::<RequestView>(){return Err(Error::Contract);}
 // SAFETY: caller supplied a live aligned readable view; copy before slot write.
 let v=unsafe{ptr::read(v)};let r=Request::new(v.nomination_ref,v.nonce,view_action(v)?)?;
 // SAFETY: output slot is writable; no borrowed input remains.
 unsafe{ptr::write(out,Box::into_raw(Box::new(MdrRequest(r))));}Ok(())})())}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mdr_request_decode(p:*const u8,n:u32,out:*mut *mut MdrRequest)->u32 {status((||{aligned(out)?;
 // SAFETY: caller supplies readable input and writable output; decode owns it first.
 let r=Request::decode(unsafe{bytes(p,n,crate::MAX_REQUEST_BYTES)}?)?;unsafe{ptr::write(out,Box::into_raw(Box::new(MdrRequest(r))));}Ok(())})())}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mdr_request_free(r:*mut MdrRequest){if !r.is_null(){// SAFETY: caller transfers this one live Box handle exactly once.
 unsafe{drop(Box::from_raw(r));}}}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mdr_request_get_view(r:*const MdrRequest,out:*mut RequestView,n:u32)->u32 {status((||{aligned(r)?;output(out,n)?;
 // SAFETY: caller supplies a valid live request handle.
 let r=unsafe{&*r};disjoint(out.addr(),size_of::<RequestView>(),&request_ranges(r)?)?;let v=request_view(&r.0);
 // SAFETY: writable checked output is disjoint from owner.
 unsafe{ptr::write(out,v);}Ok(())})())}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mdr_request_encode(r:*const MdrRequest,out:*mut u8,capacity:u32,length:*mut u32)->u32 {status((||{aligned(r)?;
 // SAFETY: valid immutable handle and caller buffers as documented.
 let r=unsafe{&*r};unsafe{copy_wire(r.0.wire(),out,capacity,length,&request_ranges(r)?)} })())}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mdr_response_set(r:*const MdrRequest,v:*const ReplyView,n:u32,out:*mut *mut MdrResponse)->u32 {status((||{aligned(r)?;aligned(v)?;aligned(out)?;if n as usize!=size_of::<ReplyView>(){return Err(Error::Contract);}
 // SAFETY: live immutable request and readable view copied before output write.
 let r=unsafe{&*r};disjoint(out.addr(),size_of::<*mut MdrResponse>(),&request_ranges(r)?)?;let v=unsafe{ptr::read(v)};let reply=unsafe{view_reply(v)}?;let response=Response::new(&r.0,reply)?;unsafe{ptr::write(out,Box::into_raw(Box::new(MdrResponse(response))));}Ok(())})())}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mdr_response_decode_for(p:*const u8,n:u32,r:*const MdrRequest,out:*mut *mut MdrResponse)->u32 {status((||{aligned(r)?;aligned(out)?;
 // SAFETY: original request owner/input are live, slot writable and disjoint.
 let r=unsafe{&*r};disjoint(out.addr(),size_of::<*mut MdrResponse>(),&request_ranges(r)?)?;let response=Response::decode_for(unsafe{bytes(p,n,crate::MAX_RESPONSE_BYTES)}?,&r.0)?;unsafe{ptr::write(out,Box::into_raw(Box::new(MdrResponse(response))));}Ok(())})())}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mdr_response_free(r:*mut MdrResponse){if !r.is_null(){// SAFETY: one caller-owned live Box is transferred exactly once.
 unsafe{drop(Box::from_raw(r));}}}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mdr_response_get_view(r:*const MdrResponse,out:*mut ResponseView,n:u32)->u32 {status((||{aligned(r)?;output(out,n)?;
 // SAFETY: live response and disjoint writable output are caller obligations.
 let r=unsafe{&*r};disjoint(out.addr(),size_of::<ResponseView>(),&response_ranges(r)?)?;let q=r.0.request();let v=ResponseView{abi_version:1,struct_size:size_of::<ResponseView>() as u32,action:q.action().kind() as u32,reserved:0,nomination_ref:q.nomination_ref(),nonce:q.nonce(),request_digest:q.digest(),reply:reply_view(&r.0)};unsafe{ptr::write(out,v);}Ok(())})())}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mdr_response_encode(r:*const MdrResponse,out:*mut u8,capacity:u32,length:*mut u32)->u32 {status((||{aligned(r)?;
 // SAFETY: valid owner/caller buffers; checked known allocation disjointness.
 let r=unsafe{&*r};unsafe{copy_wire(r.0.wire(),out,capacity,length,&response_ranges(r)?)} })())}
unsafe fn digest(out:*mut u8,capacity:u32,d:[u8;32])->u32 {status((||{if out.is_null()||capacity<32{return Err(Error::Buffer);}range(out.addr(),32)?;
 // SAFETY: caller supplies writable32-byte prefix; local digest cannot alias it.
 unsafe{ptr::copy_nonoverlapping(d.as_ptr(),out,32);}Ok(())})())}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mdr_schema_digest(out:*mut u8,n:u32)->u32 {// SAFETY: same documented digest buffer contract.
 unsafe{digest(out,n,crate::schema_digest())}}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mdr_page_schema_digest(out:*mut u8,n:u32)->u32 {// SAFETY: same documented digest buffer contract.
 unsafe{digest(out,n,crate::page_schema_digest())}}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mdr_client_new(p:*const u8,n:u32,out:*mut *mut MdrClient)->u32 {status((||{aligned(out)?;if n!=32{return Err(Error::Invalid);}
 // SAFETY: caller supplies32 readable bytes; copied before output-slot write.
 let reference:[u8;32]=unsafe{bytes(p,n,32)}?.try_into().map_err(|_|Error::Invalid)?;let state=ClientState::new(reference)?;unsafe{ptr::write(out,Box::into_raw(Box::new(MdrClient(state))));}Ok(())})())}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mdr_client_free(c:*mut MdrClient){if !c.is_null(){// SAFETY: one live exclusive Box handle is released exactly once.
 unsafe{drop(Box::from_raw(c));}}}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mdr_client_snapshot(c:*const MdrClient,out:*mut Snapshot,n:u32)->u32 {status((||{aligned(c)?;output(out,n)?;disjoint(out.addr(),size_of::<Snapshot>(),&[range(c.addr(),size_of::<MdrClient>())?])?;
 // SAFETY: immutable live client and writable disjoint snapshot output.
 let s=unsafe{&*c}.0.snapshot();let v=Snapshot{abi_version:1,struct_size:size_of::<Snapshot>() as u32,phase:s.phase as u32,reserved:0,calls:s.calls,request_wire_bytes:s.request_wire_bytes,response_wire_bytes:s.response_wire_bytes,next_sequence:s.next_sequence,resident_nonces:s.resident_nonces as u64};unsafe{ptr::write(out,v);}Ok(())})())}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mdr_client_begin(c:*mut MdrClient,v:*const RequestView,n:u32,out:*mut *mut MdrRequest)->u32 {status((||{aligned(c)?;aligned(v)?;aligned(out)?;if n as usize!=size_of::<RequestView>(){return Err(Error::Contract);}disjoint(out.addr(),size_of::<*mut MdrRequest>(),&[range(c.addr(),size_of::<MdrClient>())?])?;
 // SAFETY: caller supplies exclusive client, copied view and disjoint output slot.
 let v=unsafe{ptr::read(v)};let action=view_action(v)?;let c=unsafe{&mut *c};if v.nomination_ref!=c.0.nomination_ref(){return Err(Error::Correlation);}let request=c.0.begin(v.nonce,action)?;unsafe{ptr::write(out,Box::into_raw(Box::new(MdrRequest(request))));}Ok(())})())}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mdr_client_accept(c:*mut MdrClient,p:*const u8,n:u32,out:*mut *mut MdrResponse)->u32 {status((||{aligned(c)?;aligned(out)?;let own=[range(c.addr(),size_of::<MdrClient>())?];disjoint(out.addr(),size_of::<*mut MdrResponse>(),&own)?;disjoint(p.addr(),n as usize,&own)?;
 // SAFETY: caller guarantees input/slot do not alias any client-owned allocation.
 let input=unsafe{bytes(p,n,crate::MAX_RESPONSE_BYTES)}?;let response=unsafe{&mut *c}.0.accept(input)?;unsafe{ptr::write(out,Box::into_raw(Box::new(MdrResponse(response))));}Ok(())})())}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mdr_client_transport_unknown(c:*mut MdrClient)->u32 {status((||{aligned(c)?;
 // SAFETY: exclusive live client handle.
 unsafe{&mut *c}.0.transport_unknown()})())}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mdr_client_call_once(c:*mut MdrClient,v:*const RequestView,n:u32,call:Option<Transport>,context:*mut std::ffi::c_void,out:*mut *mut MdrResponse)->u32 {status((||{aligned(c)?;aligned(v)?;aligned(out)?;if n as usize!=size_of::<RequestView>(){return Err(Error::Contract);}disjoint(out.addr(),size_of::<*mut MdrResponse>(),&[range(c.addr(),size_of::<MdrClient>())?])?;
 // SAFETY: view copied before one callback; exclusive non-reentrant client.
 let v=unsafe{ptr::read(v)};let action=view_action(v)?;let c=unsafe{&mut *c};if v.nomination_ref!=c.0.nomination_ref(){return Err(Error::Correlation);}
 let response=if let Some(call)=call{c.0.call_once_with(v.nonce,action,|input,output|{
 // SAFETY: callback is caller-supplied valid ABI and cannot retain/reenter buffers.
 unsafe{call(context,input.as_ptr(),input.len() as u32,output.as_mut_ptr(),output.len() as u32)}
 })?}else{c.0.call_once(v.nonce,action)?};unsafe{ptr::write(out,Box::into_raw(Box::new(MdrResponse(response))));}Ok(())})())}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mdr_client_begin_request(c:*mut MdrClient,q:*const MdrRequest,out:*mut *mut MdrRequest)->u32 {status((||{aligned(c)?;aligned(q)?;aligned(out)?;
 // SAFETY: independent live request/client owners and writable disjoint slot.
 let q=unsafe{&*q};disjoint(out.addr(),size_of::<*mut MdrRequest>(),&request_ranges(q)?)?;disjoint(out.addr(),size_of::<*mut MdrRequest>(),&[range(c.addr(),size_of::<MdrClient>())?])?;let request=q.0.clone();let request=unsafe{&mut *c}.0.begin_request(request)?;unsafe{ptr::write(out,Box::into_raw(Box::new(MdrRequest(request))));}Ok(())})())}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mdr_client_call_request_once(c:*mut MdrClient,q:*const MdrRequest,call:Option<Transport>,context:*mut std::ffi::c_void,out:*mut *mut MdrResponse)->u32 {status((||{aligned(c)?;aligned(q)?;aligned(out)?;
 // SAFETY: caller supplies independent owner handles and disjoint writable slot.
 let q=unsafe{&*q};disjoint(out.addr(),size_of::<*mut MdrResponse>(),&request_ranges(q)?)?;disjoint(out.addr(),size_of::<*mut MdrResponse>(),&[range(c.addr(),size_of::<MdrClient>())?])?;let request=q.0.clone();let c=unsafe{&mut *c};
 let response=if let Some(call)=call{c.0.call_request_once_with(request,|input,output|{
 // SAFETY: valid non-reentrant callback; bounded buffers cannot be retained.
 unsafe{call(context,input.as_ptr(),input.len() as u32,output.as_mut_ptr(),output.len() as u32)}
 })?}else{c.0.call_request_once(request)?};unsafe{ptr::write(out,Box::into_raw(Box::new(MdrResponse(response))));}Ok(())})())}
