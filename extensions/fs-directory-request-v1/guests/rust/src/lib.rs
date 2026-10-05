//! Raw host-bound Open input; no SDK task envelope, path, or native capture.
use morrow_fs_directory_request_v1::{Action,ClientState,Phase,Reply,Request};
#[link(wasm_import_module="morrow_task_v1")]
unsafe extern "C" {fn read_input(output:*mut u8,capacity:u32)->i32;fn complete(input:*const u8,length:u32)->i32;}
fn run(mode:u32)->Result<(),()> {
    let mut input=vec![0u8;131072];
    // SAFETY: live owned disjoint bounded buffer for this synchronous task import.
    let n=unsafe{read_input(input.as_mut_ptr(),input.len() as u32)};
    if n<=0||n>512{return Err(());}
    let open=Request::decode(&input[..n as usize]).map_err(|_|())?;
    if open.action()!=Action::Open{return Err(());}
    let initial_nonce=open.nonce();let mut state=ClientState::new(open.nomination_ref()).map_err(|_|())?;
    let response=state.call_request_once(open).map_err(|_|())?;
    if !matches!(response.reply(),Reply::Opened(_)){return Err(());}
    let mut serial=0u64;
    let final_response=loop {
        let nonce=loop {serial=serial.checked_add(1).ok_or(())?;if serial>1024{return Err(());}
            let mut n=[0xd1;32];n[..8].copy_from_slice(&serial.to_le_bytes());if n!=initial_nonce{break n;}};
        let action=match mode {1=>state.finish_action(),2=>state.cancel_action(),_=>state.next_action()}.map_err(|_|())?;
        let response=state.call_once(nonce,action).map_err(|_|())?;
        match response.reply() {
            Reply::Page(_) if mode==0=>{},Reply::Finished if mode==1=>{},Reply::Cancelled if mode==2=>{},_=>return Err(()),
        }
        if state.snapshot().phase==Phase::Terminal{break response;}
        if state.snapshot().phase!=Phase::Ready{return Err(());}
    };
    // SAFETY: exact checked final Response wire remains owned through completion.
    if unsafe{complete(final_response.wire().as_ptr(),final_response.wire().len() as u32)}!=0{return Err(());}
    Ok(())
}
#[unsafe(no_mangle)] pub extern "C" fn morrow_run()->i32 {if run(0).is_ok(){0}else{-1}}
#[unsafe(no_mangle)] pub extern "C" fn morrow_finish()->i32 {if run(1).is_ok(){0}else{-1}}
#[unsafe(no_mangle)] pub extern "C" fn morrow_cancel()->i32 {if run(2).is_ok(){0}else{-1}}
