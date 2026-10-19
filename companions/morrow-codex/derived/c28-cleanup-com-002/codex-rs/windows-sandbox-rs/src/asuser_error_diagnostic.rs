//! Error-only, best-effort outer stderr diagnostics, not A1/v6/v7 wire.
//! Existing failure/catch sites only; no success, stdout, ACK or acceptance.
//! Pure codecs below never inspect error strings, handles, paths or payloads.
use crate::asuser_parent_diagnostic as legacy;
use crate::asuser_startup_observation::{
    FrontendStage, SdkStage, StartupEvent, StartupOutcome, StartupSeal,
    StartupSnapshot, StartupStage, StartupUnavailable,
};
use std::io;

pub const MAX_ERROR_RECORD_BYTES: usize = 537;
const SUMMARY_BYTES: usize = 128;
const PREFIX: &[u8; 8] = b"C28ERR1 ";
const SUMMARY_MAGIC: &[u8; 8] = b"C28ES001";
const HEX: &[u8; 16] = b"0123456789abcdef";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorDiagnosticRole { Parent, Import, Startup }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartupFailureTrigger { ReturnedError, CaughtUnwind }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorCodecError { Length, Prefix, Kind, Hex, Checksum, Reserved, Semantic }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ErrorDiagnosticRecord {
    pub kind: u8, pub payload: [u8; 256], pub payload_len: u16,
}
pub(super) struct Frame { bytes: [u8; MAX_ERROR_RECORD_BYTES], pub(super) len: usize }
impl Frame { pub(super) fn bytes(&self) -> &[u8] { &self.bytes[..self.len] } }
fn role_byte(role: ErrorDiagnosticRole) -> u8 {
    match role { ErrorDiagnosticRole::Parent => 1, ErrorDiagnosticRole::Import => 2,
        ErrorDiagnosticRole::Startup => 3 }
}
fn trigger_byte(trigger: StartupFailureTrigger) -> u8 {
    match trigger { StartupFailureTrigger::ReturnedError => 1, StartupFailureTrigger::CaughtUnwind => 2 }
}
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 { crc = (crc >> 1) ^ (0xedb88320u32 & 0u32.wrapping_sub(crc & 1)); }
    }
    !crc
}
fn digit(byte: u8) -> Result<u8, ErrorCodecError> {
    match byte { b'0'..=b'9' => Ok(byte - b'0'), b'a'..=b'f' => Ok(byte - b'a' + 10),
        _ => Err(ErrorCodecError::Hex) }
}
fn put32(out: &mut [u8], value: u32) { out.copy_from_slice(&value.to_le_bytes()); }
fn get32(bytes: &[u8]) -> u32 { u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) }
fn get64(bytes: &[u8]) -> u64 {
    u64::from_le_bytes([bytes[0],bytes[1],bytes[2],bytes[3],bytes[4],bytes[5],bytes[6],bytes[7]])
}
fn frame(kind: u8, payload: &[u8]) -> Frame {
    // All callers supply exactly the fixed 256 or 128 byte arrays below.
    let mut out = Frame { bytes: [0; MAX_ERROR_RECORD_BYTES], len: 25 + 2 * payload.len() };
    out.bytes[..8].copy_from_slice(PREFIX); out.bytes[8] = kind; out.bytes[9] = b' ';
    for n in 0..4 { out.bytes[10 + n] = HEX[(payload.len() >> (4 * (3 - n))) & 15]; }
    out.bytes[14] = b' ';
    for (n, value) in payload.iter().enumerate() {
        out.bytes[15 + 2 * n] = HEX[usize::from(value >> 4)];
        out.bytes[16 + 2 * n] = HEX[usize::from(value & 15)];
    }
    let end = 15 + 2 * payload.len();
    let checksum = crc32(&out.bytes[..end]); out.bytes[end] = b' ';
    for n in 0..8 { out.bytes[end + 1 + n] = HEX[((checksum >> (4 * (7 - n))) & 15) as usize]; }
    out.bytes[end + 9] = b'\n'; out
}
fn kind_code(kind: io::ErrorKind) -> u8 {
    match kind { io::ErrorKind::NotFound=>1, io::ErrorKind::PermissionDenied=>2,
        io::ErrorKind::AlreadyExists=>3, io::ErrorKind::InvalidInput=>4,
        io::ErrorKind::InvalidData=>5, io::ErrorKind::TimedOut=>6,
        io::ErrorKind::WouldBlock=>7, io::ErrorKind::Interrupted=>8,
        io::ErrorKind::UnexpectedEof=>9, io::ErrorKind::BrokenPipe=>10, _=>11 }
}
fn stage_code(stage: StartupStage) -> u8 {
    match stage {
        StartupStage::NotObserved=>0,
        StartupStage::Sdk(stage)=>match stage {
            SdkStage::ImportSuffix=>1,SdkStage::OriginLive=>2,SdkStage::EarlyInstall=>3,
            SdkStage::EarlyReserve=>4,SdkStage::EarlyObserve=>5,SdkStage::EarlyRequire=>6,
            SdkStage::EarlyAdmitted=>7,SdkStage::CallerArtifact=>8,SdkStage::RolePins=>9,
            SdkStage::GuardReturnedError=>10,
        },
        StartupStage::Frontend(stage)=>match stage {
            FrontendStage::Arguments=>32,FrontendStage::PhaseCheck=>33,FrontendStage::ConfigBuild=>34,
            FrontendStage::ValidateBeforeSetup=>35,FrontendStage::FirmwareUuidCheck=>36,
            FrontendStage::ConfigPreflight=>37,FrontendStage::ProvisioningConfigBuild=>38,
            FrontendStage::LifecyclePreflight=>39,
        },
        StartupStage::FirstPromptWrite=>64,StartupStage::FirstPromptFlush=>65,
    }
}
fn outcome_code(outcome: StartupOutcome) -> u8 {
    match outcome { StartupOutcome::NotObserved=>0,StartupOutcome::Entered=>1,
        StartupOutcome::Returned=>2,StartupOutcome::ReturnedVoid=>3,StartupOutcome::IoError=>4,
        StartupOutcome::OpaqueError=>5,StartupOutcome::Unwind=>6 }
}
fn event_bytes(event: StartupEvent) -> [u8; 16] {
    let mut out = [0; 16]; out[0]=stage_code(event.stage);out[1]=outcome_code(event.outcome);
    if let Some(kind)=event.io_kind {out[2]=kind_code(kind);}
    if let Some(raw)=event.raw_os_error {out[3]=1;out[8..12].copy_from_slice(&raw.to_le_bytes());}
    put32(&mut out[4..8],event.sequence);out
}
fn validate_event(bytes: &[u8], mode: u8, sequence_limit: u64) -> Result<(),ErrorCodecError> {
    if bytes.len()!=16 || bytes[12..].iter().any(|v|*v!=0) || bytes[3]>1 {
        return Err(ErrorCodecError::Reserved);
    }
    if bytes[0]==0 {
        return if bytes.iter().all(|v|*v==0) && mode!=3 {Ok(())} else {Err(ErrorCodecError::Semantic)};
    }
    let sdk=(1..=10).contains(&bytes[0]);
    let frontend=(32..=39).contains(&bytes[0]) || matches!(bytes[0],64|65);
    if (!sdk&&!frontend) || (mode==1&&!sdk) || (mode==2&&!frontend)
        || get32(&bytes[4..8])==0 || u64::from(get32(&bytes[4..8]))>sequence_limit {
        return Err(ErrorCodecError::Semantic);
    }
    if mode==3 {
        if !matches!(bytes[1],4..=6) {return Err(ErrorCodecError::Semantic);}
    } else if bytes[1]!=1 {return Err(ErrorCodecError::Semantic);}
    if bytes[1]==4 {
        if !(1..=11).contains(&bytes[2]) {return Err(ErrorCodecError::Semantic);}
    } else if bytes[2]!=0 || bytes[3]!=0 {return Err(ErrorCodecError::Semantic);}
    if bytes[3]==0 && bytes[8..12].iter().any(|v|*v!=0) {return Err(ErrorCodecError::Reserved);}
    Ok(())
}
fn unavailable(role:ErrorDiagnosticRole,trigger:StartupFailureTrigger,quality:u8)->[u8;SUMMARY_BYTES] {
    let mut out=[0;SUMMARY_BYTES];out[..8].copy_from_slice(SUMMARY_MAGIC);
    out[8]=1;out[9]=quality;out[10]=trigger_byte(trigger);out[12]=role_byte(role);out
}
fn unavailable_quality(value:StartupUnavailable)->u8 {
    match value {StartupUnavailable::Unsupported=>2,StartupUnavailable::Unbound=>3,
        StartupUnavailable::Busy=>4,StartupUnavailable::Poisoned=>5,
        StartupUnavailable::Incomplete=>6,StartupUnavailable::WrongGeneration=>7}
}
fn summary(snapshot:StartupSnapshot,trigger:StartupFailureTrigger)->Result<[u8;SUMMARY_BYTES],ErrorCodecError> {
    let mut out=unavailable(ErrorDiagnosticRole::Startup,trigger,1);
    out[11]=3;out[13]=1;out[14]=snapshot.sdk.retained;out[15]=snapshot.frontend.retained;
    out[16..24].copy_from_slice(&snapshot.generation.to_le_bytes());
    out[24..40].copy_from_slice(&event_bytes(snapshot.sdk.active));
    out[40..56].copy_from_slice(&event_bytes(snapshot.frontend.active));
    let first=snapshot.first_error.ok_or(ErrorCodecError::Semantic)?;
    if snapshot.seal!=StartupSeal::FirstError {return Err(ErrorCodecError::Semantic);}
    out[56..72].copy_from_slice(&event_bytes(first));
    put32(&mut out[72..76],snapshot.sdk.total_events);
    put32(&mut out[76..80],snapshot.frontend.total_events);
    validate_summary(&out)?;Ok(out)
}
fn validate_summary(bytes:&[u8])->Result<(),ErrorCodecError> {
    if bytes.len()!=SUMMARY_BYTES {return Err(ErrorCodecError::Length);}
    if &bytes[..8]!=SUMMARY_MAGIC || bytes[8]!=1 {return Err(ErrorCodecError::Prefix);}
    if !(1..=9).contains(&bytes[9]) || !(1..=2).contains(&bytes[10])
        || !(1..=3).contains(&bytes[12]) {return Err(ErrorCodecError::Kind);}
    if bytes[80..].iter().any(|v|*v!=0) {return Err(ErrorCodecError::Reserved);}
    if bytes[9]!=1 {
        if bytes[11]!=0 || bytes[13..80].iter().any(|v|*v!=0)
            || (bytes[9]==9 && bytes[12]==3) || (bytes[9]!=9 && bytes[12]!=3) {
            return Err(ErrorCodecError::Semantic);
        }
        return Ok(());
    }
    if bytes[12]!=3 || bytes[11]!=3 || bytes[13]!=1 || get64(&bytes[16..24])==0
        || bytes[14]>8 || bytes[15]>8 {return Err(ErrorCodecError::Semantic);}
    let sdk=get32(&bytes[72..76]);let frontend=get32(&bytes[76..80]);
    if u32::from(bytes[14])!=sdk.min(8) || u32::from(bytes[15])!=frontend.min(8) {
        return Err(ErrorCodecError::Semantic);
    }
    let limit=u64::from(sdk)+u64::from(frontend);
    validate_event(&bytes[24..40],1,limit)?;
    validate_event(&bytes[40..56],2,limit)?;
    validate_event(&bytes[56..72],3,limit)?;
    // FirstError retires the failing trace's active entry. Only the OTHER
    // trace may remain Entered, under a distinct global entry sequence.
    let first_is_sdk=(1..=10).contains(&bytes[56]);
    let own_active=if first_is_sdk{&bytes[24..40]}else{&bytes[40..56]};
    let other_active=if first_is_sdk{&bytes[40..56]}else{&bytes[24..40]};
    if own_active[0]!=0 || (other_active[0]!=0
        && get32(&other_active[4..8])==get32(&bytes[60..64])) {
        return Err(ErrorCodecError::Semantic);
    }
    if (bytes[24]!=0 && sdk==0) || (bytes[40]!=0 && frontend==0)
        || ((1..=10).contains(&bytes[56]) && sdk<2)
        || (!(1..=10).contains(&bytes[56]) && frontend<2) {
        return Err(ErrorCodecError::Semantic);
    }
    Ok(())
}
pub(super) fn startup_frame(snapshot:Result<StartupSnapshot,StartupUnavailable>,trigger:StartupFailureTrigger)->Option<Frame> {
    match snapshot {
        // A later failure is outside the sealed first-output scope. Do not
        // turn a historically normal flush into a startup-error record.
        Ok(value) if value.seal==StartupSeal::FirstPromptFlushReturned => None,
        Ok(value)=>Some(match summary(value,trigger) {
            Ok(payload)=>frame(b'S',&payload),
            Err(_)=>frame(b'U',&unavailable(ErrorDiagnosticRole::Startup,trigger,8)),
        }),
        Err(value)=>Some(frame(b'U',&unavailable(ErrorDiagnosticRole::Startup,trigger,unavailable_quality(value)))),
    }
}
pub(super) fn legacy_frame(role:ErrorDiagnosticRole,encoded:Result<[u8;256],legacy::CodecError>,
    trigger:StartupFailureTrigger)->Frame {
    let valid=match encoded {
        Ok(payload) if legacy::Snapshot::decode(&payload).is_ok()
            && payload[9]==role_byte(role)
            && payload[10]==match trigger {StartupFailureTrigger::ReturnedError=>2,StartupFailureTrigger::CaughtUnwind=>3}
            =>Some(payload),
        _=>None,
    };
    match valid {
        Some(payload)=>frame(if role==ErrorDiagnosticRole::Parent{b'P'}else{b'I'},&payload),
        None=>frame(b'U',&unavailable(role,trigger,9)),
    }
}
/// Strict finite decoder only; output is diagnostic DATA, never native proof.
/// A valid record does not establish delivery BOOL, EOF, process exit or close.
pub fn decode_error_only_record(bytes:&[u8])->Result<ErrorDiagnosticRecord,ErrorCodecError> {
    if bytes.len()<25 || bytes.len()>MAX_ERROR_RECORD_BYTES {return Err(ErrorCodecError::Length);}
    if &bytes[..8]!=PREFIX {return Err(ErrorCodecError::Prefix);}
    let len=match bytes[8] {b'P'|b'I'=>256,b'S'|b'U'=>128,_=>return Err(ErrorCodecError::Kind)};
    let end=15+2*len;
    if bytes.len()!=25+2*len {return Err(ErrorCodecError::Length);}
    if bytes[9]!=b' '||bytes[14]!=b' '||bytes[end]!=b' '||bytes[end+9]!=b'\n' {
        return Err(ErrorCodecError::Prefix);
    }
    let mut stated=0usize;for byte in &bytes[10..14]{stated=(stated<<4)|usize::from(digit(*byte)?);}
    if stated!=len {return Err(ErrorCodecError::Length);}
    let mut checksum=0u32;for byte in &bytes[end+1..end+9]{checksum=(checksum<<4)|u32::from(digit(*byte)?);}
    if checksum!=crc32(&bytes[..end]) {return Err(ErrorCodecError::Checksum);}
    let mut out=ErrorDiagnosticRecord{kind:bytes[8],payload:[0;256],payload_len:len as u16};
    for n in 0..len {out.payload[n]=(digit(bytes[15+2*n])?<<4)|digit(bytes[16+2*n])?;}
    if matches!(out.kind,b'P'|b'I') {
        legacy::Snapshot::decode(&out.payload).map_err(|_|ErrorCodecError::Semantic)?;
        if out.payload[9]!=if out.kind==b'P'{1}else{2}||!matches!(out.payload[10],2|3) {
            return Err(ErrorCodecError::Semantic);
        }
    } else {
        validate_summary(&out.payload[..SUMMARY_BYTES])?;
        if (out.kind==b'S')!=(out.payload[9]==1) {return Err(ErrorCodecError::Semantic);}
    }
    Ok(out)
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::asuser_startup_observation::{StartupCarrier,FrontendStage};
    fn failed()->StartupSnapshot {
        let carrier=StartupCarrier::new();carrier.bind(7);
        let observation=carrier.observation(7);
        observation.enter(FrontendStage::ConfigBuild);
        observation.frontend_opaque_error();observation.history().snapshot().unwrap()
    }
    fn legacy_error(scope:legacy::Scope,stage:legacy::Stage)->[u8;256] {
        let mut value=legacy::Capture::new(scope);value.enter(stage);
        value.latch_io(&io::Error::from_raw_os_error(123));value.snapshot().encode().unwrap()
    }
    #[test] fn fixed_frame_lengths_bound_parent_import_and_startup() {
        let p=legacy_frame(ErrorDiagnosticRole::Parent,Ok(legacy_error(legacy::Scope::Parent,legacy::Stage::ChildCreate)),
            StartupFailureTrigger::ReturnedError);
        let i=legacy_frame(ErrorDiagnosticRole::Import,Ok(legacy_error(legacy::Scope::Import,legacy::Stage::ImportRead)),
            StartupFailureTrigger::ReturnedError);
        let s=startup_frame(Ok(failed()),StartupFailureTrigger::ReturnedError).unwrap();
        assert_eq!((p.len,i.len,s.len),(537,537,281));assert!(MAX_ERROR_RECORD_BYTES<=1024);
        for value in [&p,&i,&s]{assert!(decode_error_only_record(value.bytes()).is_ok());}
    }
    #[test] fn legacy_payload_is_exact_original_codec_and_scope_is_strict() {
        let bytes=legacy_error(legacy::Scope::Parent,legacy::Stage::ChildCreate);
        let value=legacy_frame(ErrorDiagnosticRole::Parent,Ok(bytes),StartupFailureTrigger::ReturnedError);
        let decoded=decode_error_only_record(value.bytes()).unwrap();assert_eq!(decoded.payload,bytes);
        assert!(decode_error_only_record(frame(b'I',&bytes).bytes()).is_err());
    }
    #[test] fn old_returned_success_cannot_be_reported_as_error() {
        let mut value=legacy::Capture::new(legacy::Scope::Import);value.enter(legacy::Stage::ImportRead);
        value.completed(legacy::Stage::ImportRead);value.returned_ok();
        let encoded=value.snapshot().encode().unwrap();
        assert!(decode_error_only_record(frame(b'I',&encoded).bytes()).is_err());
        let fallback=legacy_frame(ErrorDiagnosticRole::Import,Ok(encoded),StartupFailureTrigger::ReturnedError);
        assert_eq!(decode_error_only_record(fallback.bytes()).unwrap().kind,b'U');
    }
    #[test] fn legacy_trigger_mismatch_and_bad_codec_are_unavailable_not_fake_old_data() {
        let raw=legacy_error(legacy::Scope::Parent,legacy::Stage::ChildCreate);
        let mismatch=legacy_frame(ErrorDiagnosticRole::Parent,Ok(raw),StartupFailureTrigger::CaughtUnwind);
        let invalid=legacy_frame(ErrorDiagnosticRole::Import,Err(legacy::CodecError::Inconsistent),
            StartupFailureTrigger::ReturnedError);
        for value in [mismatch,invalid]{let record=decode_error_only_record(value.bytes()).unwrap();
            assert_eq!(record.kind,b'U');assert_eq!(record.payload[9],9);}
    }
    #[test] fn every_unavailable_quality_has_absent_fields_not_clean_zero_claims() {
        for value in [StartupUnavailable::Unsupported,StartupUnavailable::Unbound,StartupUnavailable::Busy,
            StartupUnavailable::Poisoned,StartupUnavailable::Incomplete,StartupUnavailable::WrongGeneration] {
            let record=startup_frame(Err(value),StartupFailureTrigger::CaughtUnwind).unwrap();
            let decoded=decode_error_only_record(record.bytes()).unwrap();
            assert_eq!(decoded.kind,b'U');assert_eq!(decoded.payload[11],0);
            assert!(decoded.payload[13..80].iter().all(|v|*v==0));
        }
    }
    #[test] fn normal_flush_seal_is_not_reclassified_by_a_later_failure() {
        let carrier=StartupCarrier::new();carrier.bind(7);let observation=carrier.observation(7);
        let mut prompt=observation.first_prompt();prompt.write_entered().returned();
        let flush=prompt.flush_entered();prompt.flush_result(flush,&Ok(()));
        assert!(startup_frame(observation.history().snapshot(),StartupFailureTrigger::ReturnedError).is_none());
        assert!(startup_frame(observation.history().snapshot(),StartupFailureTrigger::CaughtUnwind).is_none());
    }
    #[test] fn typed_first_io_raw_and_active_frontend_remain_separate() {
        let carrier=StartupCarrier::new();carrier.bind(7);let observation=carrier.observation(7);
        observation.enter(FrontendStage::ValidateBeforeSetup);
        observation.sdk_enter(SdkStage::EarlyRequire).io_error(&io::Error::from_raw_os_error(123));
        let record=startup_frame(observation.history().snapshot(),StartupFailureTrigger::ReturnedError).unwrap();
        let decoded=decode_error_only_record(record.bytes()).unwrap();
        assert_eq!(decoded.payload[40],35);assert_eq!(decoded.payload[41],1);
        assert_eq!(decoded.payload[56],6);assert_eq!(decoded.payload[57],4);
        assert_eq!(decoded.payload[59],1);assert_eq!(get32(&decoded.payload[64..68]),123);
    }
    #[test] fn opaque_and_unwind_have_no_invented_io_kind_or_raw() {
        let opaque=summary(failed(),StartupFailureTrigger::ReturnedError).unwrap();
        assert_eq!(opaque[57],5);assert_eq!(opaque[58],0);assert_eq!(opaque[59],0);
        let carrier=StartupCarrier::new();carrier.bind(9);let observation=carrier.observation(9);
        observation.sdk_enter(SdkStage::EarlyObserve);carrier.sdk_unwind_current();
        let unwind=summary(observation.history().snapshot().unwrap(),StartupFailureTrigger::CaughtUnwind).unwrap();
        assert_eq!(unwind[57],6);assert_eq!(unwind[58],0);assert_eq!(unwind[59],0);
    }
    #[test] fn partial_oversize_extra_newline_and_concatenated_records_are_rejected() {
        let value=startup_frame(Ok(failed()),StartupFailureTrigger::ReturnedError).unwrap();
        for n in [0,8,24,100,280]{assert!(decode_error_only_record(&value.bytes()[..n]).is_err());}
        let mut joined=Vec::from(value.bytes());joined.extend_from_slice(value.bytes());
        assert!(decode_error_only_record(&joined).is_err());
        let mut extra=Vec::from(value.bytes());extra.push(b'\n');assert!(decode_error_only_record(&extra).is_err());
        assert!(decode_error_only_record(&[b'x';MAX_ERROR_RECORD_BYTES+1]).is_err());
    }
    #[test] fn corrupted_crc_hex_uppercase_header_and_length_are_rejected() {
        let value=startup_frame(Ok(failed()),StartupFailureTrigger::ReturnedError).unwrap();
        for (index,byte) in [(0,b'X'),(8,b'Z'),(9,b'\t'),(10,b'f'),(15,b'g'),(16,b'A'),(100,b'f'),(280,b'\r')] {
            let mut bytes=Vec::from(value.bytes());bytes[index]=byte;
            assert!(decode_error_only_record(&bytes).is_err(),"{index}");
        }
        let mut checksum=Vec::from(value.bytes());let last_digit=checksum.len()-2;
        checksum[last_digit]=if checksum[last_digit]==b'0'{b'1'}else{b'0'};
        assert_eq!(decode_error_only_record(&checksum),Err(ErrorCodecError::Checksum));
    }
    #[test] fn reserved_unknown_enums_and_false_first_error_are_rejected_even_with_valid_crc() {
        let raw=summary(failed(),StartupFailureTrigger::ReturnedError).unwrap();
        for (index,byte) in [(8,2),(9,0),(10,3),(11,7),(12,1),(13,0),(14,9),(56,99),(57,1),
            (58,12),(59,2),(68,1),(80,1),(127,1)] {
            let mut payload=raw;payload[index]=byte;
            assert!(decode_error_only_record(frame(b'S',&payload).bytes()).is_err(),"{index}");
        }
    }
    #[test] fn generation_presence_retained_count_and_sequence_must_be_consistent() {
        let raw=summary(failed(),StartupFailureTrigger::ReturnedError).unwrap();
        let mut no_generation=raw;no_generation[16..24].fill(0);
        let mut no_trace=raw;no_trace[15]=0;no_trace[76..80].fill(0);
        let mut no_sequence=raw;no_sequence[60..64].fill(0);
        for value in [no_generation,no_trace,no_sequence] {
            assert!(decode_error_only_record(frame(b'S',&value).bytes()).is_err());
        }
    }
    #[test] fn unavailable_cannot_smuggle_observed_data_or_wrong_role() {
        let raw=unavailable(ErrorDiagnosticRole::Startup,StartupFailureTrigger::ReturnedError,6);
        for (index,byte) in [(11,1),(12,1),(13,1),(16,7),(24,5),(56,6),(72,1)] {
            let mut payload=raw;payload[index]=byte;
            assert!(decode_error_only_record(frame(b'U',&payload).bytes()).is_err(),"{index}");
        }
        assert!(decode_error_only_record(frame(b'S',&raw).bytes()).is_err());
        assert!(decode_error_only_record(frame(b'U',&summary(failed(),StartupFailureTrigger::ReturnedError).unwrap()).bytes()).is_err());
    }
    #[test] fn busy_inflight_stage_is_unavailable_not_a_fabricated_return() {
        let carrier=StartupCarrier::new();carrier.bind(7);let observation=carrier.observation(7);
        observation.sdk_enter(SdkStage::EarlyObserve);carrier.unavailable();
        let value=startup_frame(observation.history().snapshot(),StartupFailureTrigger::ReturnedError).unwrap();
        let record=decode_error_only_record(value.bytes()).unwrap();
        assert_eq!(record.kind,b'U');assert_eq!(record.payload[9],6);
        assert!(record.payload[24..72].iter().all(|v|*v==0));
    }

    #[test] fn retained_ring_count_must_equal_total_capped_at_eight_with_valid_crc() {
        let mut small=summary(failed(),StartupFailureTrigger::ReturnedError).unwrap();
        assert_eq!(get32(&small[76..80]),2);assert_eq!(small[15],2);
        small[15]=1;
        assert_eq!(decode_error_only_record(frame(b'S',&small).bytes()),Err(ErrorCodecError::Semantic));

        let carrier=StartupCarrier::new();carrier.bind(7);let observation=carrier.observation(7);
        for _ in 0..5 {observation.enter(FrontendStage::Arguments).returned();}
        observation.enter(FrontendStage::ConfigBuild);observation.frontend_opaque_error();
        let mut wrapped=summary(observation.history().snapshot().unwrap(),StartupFailureTrigger::ReturnedError).unwrap();
        assert_eq!(get32(&wrapped[76..80]),12);assert_eq!(wrapped[15],8);
        assert!(decode_error_only_record(frame(b'S',&wrapped).bytes()).is_ok());
        wrapped[15]=7;
        assert_eq!(decode_error_only_record(frame(b'S',&wrapped).bytes()),Err(ErrorCodecError::Semantic));
    }

    #[test] fn sealed_first_error_cannot_coexist_with_same_trace_active_even_with_valid_crc() {
        let carrier=StartupCarrier::new();carrier.bind(7);let observation=carrier.observation(7);
        observation.enter(FrontendStage::ValidateBeforeSetup);
        observation.sdk_enter(SdkStage::EarlyRequire).io_error(&io::Error::from_raw_os_error(123));
        let sdk=summary(observation.history().snapshot().unwrap(),StartupFailureTrigger::ReturnedError).unwrap();
        assert!(decode_error_only_record(frame(b'S',&sdk).bytes()).is_ok());
        let frontend=summary(failed(),StartupFailureTrigger::ReturnedError).unwrap();
        for raw in [sdk,frontend] {
            let mut invalid=raw;let at=if (1..=10).contains(&raw[56]){24}else{40};
            invalid[at]=raw[56];invalid[at+1]=1;
            put32(&mut invalid[at+4..at+8],get32(&raw[60..64]));
            assert_eq!(decode_error_only_record(frame(b'S',&invalid).bytes()),Err(ErrorCodecError::Semantic));
        }
    }

    #[test] fn other_trace_active_cannot_reuse_first_error_global_sequence_with_valid_crc() {
        let carrier=StartupCarrier::new();carrier.bind(7);let observation=carrier.observation(7);
        observation.enter(FrontendStage::ValidateBeforeSetup);
        observation.sdk_enter(SdkStage::EarlyRequire).io_error(&io::Error::from_raw_os_error(123));
        let sdk=summary(observation.history().snapshot().unwrap(),StartupFailureTrigger::ReturnedError).unwrap();
        let carrier=StartupCarrier::new();carrier.bind(9);let observation=carrier.observation(9);
        observation.sdk_enter(SdkStage::EarlyObserve);
        observation.enter(FrontendStage::ConfigBuild);observation.frontend_opaque_error();
        let frontend=summary(observation.history().snapshot().unwrap(),StartupFailureTrigger::ReturnedError).unwrap();
        for raw in [sdk,frontend] {
            assert!(decode_error_only_record(frame(b'S',&raw).bytes()).is_ok());
            let mut invalid=raw;let other=if (1..=10).contains(&raw[56]){40}else{24};
            assert_eq!(invalid[other+1],1);
            put32(&mut invalid[other+4..other+8],get32(&raw[60..64]));
            assert_eq!(decode_error_only_record(frame(b'S',&invalid).bytes()),Err(ErrorCodecError::Semantic));
        }
    }
}
