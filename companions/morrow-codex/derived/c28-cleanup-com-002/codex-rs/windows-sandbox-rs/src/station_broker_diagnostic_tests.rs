//! SOURCE/NOT_RUN pure local atomic/layout/debt models. No mapping, process,
//! WinAPI, real registry, sleep, thread, fake native HANDLE or native proof.
use super::*;
fn case()->Case {Case::parse("0123456789abcdef0123456789abcdef").unwrap()}
fn scope()->Scope {Scope {generation:7,parent_pid:101,parent_time:0x1234_0000_0001}}
fn record()->Record {Record {words:std::array::from_fn(|_|AtomicU64::new(0))}}
fn store(record:&Record,mut payload:[u64;WORDS],sequence:u64) {
    payload[0]=sequence;
    for (word,value) in record.words.iter().zip(payload){word.store(value,Ordering::SeqCst);}
}
fn snapshot()->StationBrokerSnapshot {
    let mut h=StationBrokerSnapshot::new(7);h.helper_pid=Some(102);
    h.helper_creation_filetime_100ns=Some(0x1234_0000_0002);h
}

#[test]
fn a0_diagnostic044_exact_layout_namespace_and_header_are_bounded() {
    assert_eq!(std::mem::size_of::<Region>(),4096);assert_eq!(std::mem::align_of::<Region>(),64);
    assert_eq!(std::mem::size_of::<Record>(),1024);
    let region=Region {header:std::array::from_fn(|_|AtomicU64::new(0)),parent:record(),helper:record(),
        reserved:std::array::from_fn(|_|AtomicU64::new(0))};
    let base=&region as *const _ as usize;
    assert_eq!(&region.parent as *const _ as usize-base,256);
    assert_eq!(&region.helper as *const _ as usize-base,1280);
    assert_eq!(header(case(),[5;32])[0],u64::from_le_bytes(*b"C28D044\0"));
    let mut changed=header(case(),[5;32]);changed[31]=1;assert_ne!(changed,header(case(),[5;32]));
    for bad in ["", "00000000000000000000000000000000", "0123456789ABCDEF0123456789ABCDEF",
        "../0123456789abcdef0123456789abc", "0123456789abcdef0123456789abcdef0"] {assert!(Case::parse(bad).is_none());}
    let name=String::from_utf16(&case().name()).unwrap();
    assert_eq!(name,"Local\\C28A0Diagnostic044-0123456789abcdef0123456789abcdef\0");
    assert!(name.len()<96);assert_eq!(case().text().len(),32);
}

#[test]
fn a0_diagnostic044_binding_needs_case_sha_generation_and_full_verified_identities() {
    let mut h=snapshot();let good=encode(scope(),case(),[5;32],Stage::HelloSend,true,&h);
    assert!(parent_bound(&good,scope(),case(),[5;32],(102,0x1234_0000_0002)));
    assert!(!parent_bound(&good,Scope {generation:8,..scope()},case(),[5;32],(102,0x1234_0000_0002)));
    assert!(!parent_bound(&good,Scope {parent_time:scope().parent_time+(1u64<<32),..scope()},case(),[5;32],(102,0x1234_0000_0002)));
    assert!(!parent_bound(&good,scope(),Case::parse("1123456789abcdef0123456789abcdef").unwrap(),[5;32],(102,0x1234_0000_0002)));
    assert!(!parent_bound(&good,scope(),case(),[6;32],(102,0x1234_0000_0002)));
    assert!(!parent_bound(&good,scope(),case(),[5;32],(102,0x1235_0000_0002)));
    h.helper_pid=None;h.helper_creation_filetime_100ns=None;
    h.reported_creation=Some(StationBrokerReportedCreation {api_returned_success:true,process_id:102,thread_id:103});
    let reported=encode(scope(),case(),[5;32],Stage::ProcessIdentity,true,&h);
    assert_eq!(reported[8]&3,0);assert_eq!(reported[8]&8,8);
    assert!(!parent_bound(&reported,scope(),case(),[5;32],(102,0x1234_0000_0002)));
    let mut lease=Lease::new(case(),[5;32],Scope {generation:7,parent_pid:0,parent_time:0},Writer::Parent);
    assert!(lease.set_scope(Scope {generation:8,..scope()}).is_err());
    lease.set_scope(scope()).unwrap();assert!(lease.set_scope(scope()).is_err());
    lease.set_verified_helper(102,0x1234_0000_0002).unwrap();
    assert!(lease.set_verified_helper(103,0x1234_0000_0003).is_err());
    assert_eq!(lease.scope.parent_time,scope().parent_time);assert_eq!(lease.helper_identity,(102,0x1234_0000_0002));
}

#[test]
fn a0_diagnostic044_notstarted_odd_torn_and_sequence_exhaustion_are_not_completion() {
    let r=record();assert_eq!(read_once(&r),Err(StationBrokerDiagnosticRead::NotStarted));
    r.words[0].store(1,Ordering::SeqCst);assert_eq!(read_once(&r),Err(StationBrokerDiagnosticRead::InFlight));
    store(&r,encode(scope(),case(),[5;32],Stage::OfferReceive,true,&snapshot()),2);
    let copied=read_once(&r).unwrap();assert_eq!(copied[1],1);assert_eq!(copied[2],Stage::OfferReceive as u64);
    assert_eq!(validate_copy(2,copied,4),Err(StationBrokerDiagnosticRead::Torn));
    let mut exhausted=copied;exhausted[0]=MAX_COMMITS*2;
    assert_eq!(validate_copy(exhausted[0],exhausted,exhausted[0]),Err(StationBrokerDiagnosticRead::Unavailable));
}

#[test]
fn a0_diagnostic044_first_failure_and_channel_pending_updates_do_not_become_clean() {
    let mut h=snapshot();let first=StationBrokerFailure::from_io(StationBrokerDebt::Control,&io::Error::from_raw_os_error(109));
    latch_delivery(&mut h,ReportStage::Aliases,StationBrokerOutput::Failed,first);
    latch_delivery(&mut h,ReportStage::Aliases,StationBrokerOutput::Unavailable(StationBrokerUnavailable::BudgetExpired),
        StationBrokerFailure::from_io(StationBrokerDebt::Deadline,&io::Error::from(io::ErrorKind::TimedOut)));
    let r=record();store(&r,encode(scope(),case(),[5;32],Stage::OfferReceive,false,&h),2);
    let writer=ChannelWriter {record:&r};writer.pending(true,false,StationBrokerHandle::Held,
        StationBrokerHandle::Held,StationBrokerHandle::Held);
    let copied=read_once(&r).unwrap();assert_eq!(copied[0],4);assert_eq!(copied[21],1);
    assert_eq!(copied[1],3);assert_eq!(copied[10]&1,1);assert_eq!(copied[12],debt_code(first.target));
    assert_eq!(copied[15],109);assert_eq!(copied[39],handle_code(StationBrokerHandle::Held));
    writer.pending(false,true,StationBrokerHandle::Held,StationBrokerHandle::Held,StationBrokerHandle::Held);
    assert_eq!(read_once(&r).unwrap()[0],6);assert_eq!(read_once(&r).unwrap()[21],2);
    let mut lease=Lease::new(case(),[5;32],scope(),Writer::Parent);
    lease.first_io(io::ErrorKind::BrokenPipe);lease.first_io(io::ErrorKind::TimedOut);
    assert_eq!(lease.first_io_kind,Some(9));assert_eq!(h.first_failure,Some(first));
}

#[test]
fn a0_diagnostic044_unmapped_view_does_not_erase_handle_debt_or_reauthorize_retirement() {
    let mut h=snapshot();let mut state=StationBrokerDiagnosticSnapshot::default();
    state.enabled=true;state.mapping=StationBrokerHandle::Held;state.view=StationBrokerHandle::Held;
    state.record=StationBrokerDiagnosticRead::Committed;
    assert!(retirement_ready(state,true,&h));assert!(!retirement_ready(state,false,&h));
    h.no_replay=true;assert!(!retirement_ready(state,true,&h));h.no_replay=false;
    state.view=StationBrokerHandle::ClosedVerified;state.mapping=StationBrokerHandle::CloseFailed{error:170};
    state.record=StationBrokerDiagnosticRead::Retired;
    assert!(!state.closed());assert!(!retirement_ready(state,true,&h));
    state.mapping=StationBrokerHandle::ClosedVerified;assert!(state.closed());
    assert!(!retirement_ready(state,true,&h)); // no second unmap/close from history
    // A real local atomic region models ONLY the uncertain-view write guard.
    // No MapViewOfFile/UnmapViewOfFile or numeric native handle is involved.
    let mut memory=Region {header:std::array::from_fn(|_|AtomicU64::new(0)),parent:record(),helper:record(),
        reserved:std::array::from_fn(|_|AtomicU64::new(0))};
    store(&memory.parent,encode(scope(),case(),[5;32],Stage::ViewUnmap,true,&h),2);
    let mut lease=Lease::new(case(),[5;32],scope(),Writer::Parent);
    lease.view=&mut memory;lease.claimed=true;lease.unmap_in_flight=true;
    assert!(lease.channel_writer().is_none());lease.publish(Stage::ViewUnmap,false,&h);
    assert_eq!(lease.snapshot().record,StationBrokerDiagnosticRead::Unavailable);
    assert_eq!(memory.parent.words[0].load(Ordering::SeqCst),2);
}

#[test]
fn a0_diagnostic044_normal_gate_requires_receipt_native_joins_and_actual_local_closes() {
    let mut h=snapshot();h.helper_raw_dword=Some(0);h.helper_objects_closed_receipt=true;
    h.original_eof=[true;2];h.original_reader_joined=[true;2];h.host_association_unchanged=true;
    assert!(super::super::native::helper_closed_facts(&h));
    h.diagnostic.enabled=true;h.diagnostic.helper_close=StationBrokerHelperDiagnosticClose::NotObserved;
    assert!(!super::super::native::helper_closed_facts(&h));
    h.diagnostic.helper_close=StationBrokerHelperDiagnosticClose::AuthenticatedReceipt;
    assert!(!super::super::native::helper_closed_facts(&h));
    h.diagnostic.helper_close=StationBrokerHelperDiagnosticClose::AuthenticatedReceiptAndOriginalNativeJoins;
    assert!(super::super::native::helper_closed_facts(&h));assert!(!h.diagnostic.closed());
    h.diagnostic.view=StationBrokerHandle::ClosedVerified;h.diagnostic.mapping=StationBrokerHandle::ClosedVerified;
    assert!(h.diagnostic.closed());h.original_reader_joined[1]=false;
    assert!(!super::super::native::helper_closed_facts(&h));
    assert_eq!(budget(Instant::now()-std::time::Duration::from_secs(1)).unwrap_err().kind(),io::ErrorKind::TimedOut);
    // No lease acquire/retire, native call or capability is executed in this model.
}
