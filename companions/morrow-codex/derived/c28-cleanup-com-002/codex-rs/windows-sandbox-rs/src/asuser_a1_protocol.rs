//! Source047 private C/S A1 frames. Does not decode A0 or runner v6/v7/v8.
//! Decoding is DATA validation only. HELLO's own nonce/MAC is never first-sender
//! authority: the native helper must authenticate its inherited original parent,
//! kernel peer, image and S-only compiled role table BEFORE authorizing a frame.
use std::io;
use sha2::{Digest, Sha256};

pub(crate) const FRAME_BYTES: usize = 512;
pub(crate) const MAGIC: [u8; 8] = *b"C28A1V1\0";
pub(crate) const HELLO: u8 = 1;
pub(crate) const OFFER: u8 = 2;
pub(crate) const ACCEPT: u8 = 3;
pub(crate) const CLOSED: u8 = 4;
pub const A1_HELPER_SWITCH: &str = "--c28-asuser-a1-helper-v1";
pub(crate) const TABLE_BYTES: usize = 80;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RoleTable { pub c: [u8; 32], pub r: [u8; 32] }
impl RoleTable {
    // Data read from the exact held S PE section, or S's compiled static only.
    // A JSON/file/HELLO copy of a table is NOT a role capability.
    pub(crate) fn decode(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() != TABLE_BYTES || bytes[..8] != *b"C28A1TAB"
            || bytes[8..12] != 1u32.to_le_bytes() || bytes[12..16] != 1u32.to_le_bytes() {
            return Err(bad());
        }
        let c = bytes[16..48].try_into().map_err(|_| bad())?;
        let r = bytes[48..80].try_into().map_err(|_| bad())?;
        if c == [0; 32] || r == [0; 32] || c == r { return Err(bad()); }
        Ok(Self { c, r })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Binding {
    pub generation: u64,
    pub c_pid: u32,
    pub c_time: u64,
    pub s_pid: u32,
    pub s_time: u64,
    pub deadline_ticks: u64,
    pub c_image: [u8; 32],
    pub s_image: [u8; 32],
    pub r_image: [u8; 32],
    pub nonce: [u8; 32],
    pub logon_sid: [u8; 68],
    pub sid_len: u8,
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct Frame {
    pub kind: u8,
    pub binding: Binding,
    pub station: u64,
    pub desktop: u64,
    pub station_sd: [u8; 32],
    pub desktop_sd: [u8; 32],
}
fn bad() -> io::Error { io::Error::new(io::ErrorKind::InvalidData, "A1 fixed protocol rejected") }
fn put(out: &mut [u8], at: usize, value: u64) { out[at..at + 8].copy_from_slice(&value.to_le_bytes()); }
fn get(bytes: &[u8], at: usize) -> u64 { u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap()) }
pub(crate) fn valid_sid(bytes: &[u8]) -> bool {
    bytes.len() >= 8 && bytes.len() <= 68 && bytes[0] == 1 && bytes[1] <= 15
        && bytes.len() == 8 + usize::from(bytes[1]) * 4
}
fn valid_binding(v: &Binding) -> bool {
    let len = usize::from(v.sid_len);
    v.generation != 0 && v.c_pid != 0 && v.s_pid != 0 && v.c_pid != v.s_pid
        && v.c_time != 0 && v.s_time != 0 && v.deadline_ticks != 0
        && v.c_image != [0; 32] && v.s_image != [0; 32] && v.r_image != [0; 32]
        && v.c_image != v.s_image && v.c_image != v.r_image && v.s_image != v.r_image
        && v.nonce != [0; 32] && len <= 68 && valid_sid(&v.logon_sid[..len])
        // Require the exact S-1-5-5-X-Y logon SID, not a stable account SID.
        && len == 20 && v.logon_sid[..12] == [1,3,0,0,0,0,0,5,5,0,0,0]
        && v.logon_sid[12..20].iter().any(|v|*v!=0)
        && v.logon_sid[len..].iter().all(|v| *v == 0)
}
fn mac(key: &[u8; 32], bytes: &[u8]) -> [u8; 32] {
    let mut inner = [0x36; 64]; let mut outer = [0x5c; 64];
    for i in 0..32 { inner[i] ^= key[i]; outer[i] ^= key[i]; }
    let mut h = Sha256::new(); h.update(inner); h.update(bytes);
    let first = h.finalize();
    let mut h = Sha256::new(); h.update(outer); h.update(first); h.finalize().into()
}
pub(crate) fn message(kind: u8, binding: Binding) -> Frame {
    Frame { kind, binding, station: 0, desktop: 0, station_sd: [0; 32], desktop_sd: [0; 32] }
}
pub(crate) fn encode(frame: Frame) -> io::Result<[u8; FRAME_BYTES]> {
    if !valid_binding(&frame.binding) || !matches!(frame.kind, HELLO | OFFER | ACCEPT | CLOSED) {
        return Err(bad());
    }
    if frame.kind == OFFER {
        if frame.station == 0 || frame.desktop == 0 || frame.station == frame.desktop
            || frame.station_sd == [0; 32] || frame.desktop_sd == [0; 32] { return Err(bad()); }
    } else if frame.station != 0 || frame.desktop != 0
        || frame.station_sd != [0; 32] || frame.desktop_sd != [0; 32] { return Err(bad()); }
    let b = frame.binding;
    let mut out = [0; FRAME_BYTES]; out[..8].copy_from_slice(&MAGIC); out[8] = frame.kind; out[9] = 1;
    put(&mut out, 16, b.generation); put(&mut out, 24, u64::from(b.c_pid)); put(&mut out, 32, b.c_time);
    put(&mut out, 40, u64::from(b.s_pid)); put(&mut out, 48, b.s_time); put(&mut out, 56, b.deadline_ticks);
    out[64..96].copy_from_slice(&b.c_image); out[96..128].copy_from_slice(&b.s_image);
    out[128..160].copy_from_slice(&b.r_image); out[160..192].copy_from_slice(&b.nonce);
    out[192..196].copy_from_slice(&u32::from(b.sid_len).to_le_bytes());
    out[196..264].copy_from_slice(&b.logon_sid);
    put(&mut out, 264, frame.station); put(&mut out, 272, frame.desktop);
    out[280..312].copy_from_slice(&frame.station_sd); out[312..344].copy_from_slice(&frame.desktop_sd);
    let tag = mac(&b.nonce, &out[..480]); out[480..].copy_from_slice(&tag); Ok(out)
}
pub(crate) fn decode(bytes: &[u8], expected_kind: u8, expected: Option<Binding>) -> io::Result<Frame> {
    if bytes.len() != FRAME_BYTES || bytes[..8] != MAGIC || bytes[8] != expected_kind || bytes[9] != 1
        || bytes[10..16].iter().chain(&bytes[344..480]).any(|v| *v != 0) { return Err(bad()); }
    let sid_len = u32::from_le_bytes(bytes[192..196].try_into().unwrap());
    let binding = Binding {
        generation: get(bytes, 16), c_pid: get(bytes, 24).try_into().map_err(|_| bad())?,
        c_time: get(bytes, 32), s_pid: get(bytes, 40).try_into().map_err(|_| bad())?,
        s_time: get(bytes, 48), deadline_ticks: get(bytes, 56),
        c_image: bytes[64..96].try_into().unwrap(), s_image: bytes[96..128].try_into().unwrap(),
        r_image: bytes[128..160].try_into().unwrap(), nonce: bytes[160..192].try_into().unwrap(),
        sid_len: sid_len.try_into().map_err(|_| bad())?, logon_sid: bytes[196..264].try_into().unwrap(),
    };
    if !valid_binding(&binding) || expected.is_some_and(|old| old != binding) { return Err(bad()); }
    let tag = mac(&binding.nonce, &bytes[..480]);
    if tag.iter().zip(&bytes[480..]).fold(0u8, |v, (a,b)| v | (a ^ b)) != 0 { return Err(bad()); }
    let frame = Frame { kind: expected_kind, binding, station: get(bytes, 264), desktop: get(bytes, 272),
        station_sd: bytes[280..312].try_into().unwrap(), desktop_sd: bytes[312..344].try_into().unwrap() };
    // Encoding checks frame-specific canonical payload, and creates no authority.
    if encode(frame)?.as_slice() != bytes { return Err(bad()); }
    Ok(frame)
}
pub(crate) fn endpoint(nonce: &[u8; 32]) -> String {
    let digest = Sha256::digest(nonce);
    let suffix = digest.iter().map(|b| format!("{b:02x}")).collect::<String>();
    format!(r"\\.\pipe\C28AsUserA1-{suffix}")
}
pub(crate) fn valid_endpoint(value: &str) -> bool {
    value.strip_prefix(r"\\.\pipe\C28AsUserA1-").is_some_and(|tail|
        tail.len() == 64 && tail.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn binding() -> Binding {
        let mut sid = [0; 68]; sid[..20].copy_from_slice(&[1,3,0,0,0,0,0,5,5,0,0,0,7,0,0,0,8,0,0,0]);
        Binding { generation: 1, c_pid: 4, c_time: 11, s_pid: 8, s_time: 12,
            deadline_ticks: 20, c_image: [1; 32], s_image: [2; 32], r_image: [3; 32],
            nonce: [4; 32], logon_sid: sid, sid_len: 20 }
    }
    #[test]
    fn a1_frames_reject_a0_cross_version_padding_and_noncanonical_sizes() {
        let frame = encode(message(HELLO, binding())).unwrap();
        assert_eq!(decode(&frame, HELLO, None).unwrap().binding, binding());
        for index in [0, 9, 10, 344, 479, 480] {
            let mut changed = frame; changed[index] ^= 1;
            assert!(decode(&changed, HELLO, None).is_err());
        }
        assert!(decode(&frame[..256], HELLO, None).is_err());
        let mut a0 = frame; a0[..8].copy_from_slice(b"C28A0V1\0");
        assert!(decode(&a0, HELLO, None).is_err());
        assert!(!valid_endpoint(r"\\.\pipe\C28StationBrokerA0-dead"));
    }
    #[test]
    fn a1_peer_table_and_same_attempt_fields_are_not_self_authenticating() {
        assert!(RoleTable::decode(&[0; 80]).is_err());
        let mut table = [0; 80]; table[..8].copy_from_slice(b"C28A1TAB");
        table[8..12].copy_from_slice(&1u32.to_le_bytes()); table[12..16].copy_from_slice(&1u32.to_le_bytes());
        table[16..48].fill(1); table[48..].fill(3);
        assert_eq!(RoleTable::decode(&table).unwrap().r, [3; 32]);
        let old = binding(); let frame = encode(message(HELLO, old)).unwrap();
        let mut foreign = old; foreign.generation += 1;
        assert!(decode(&frame, HELLO, Some(foreign)).is_err());
        foreign = old; foreign.c_time += 1;
        assert!(decode(&frame, HELLO, Some(foreign)).is_err());
        foreign = old; foreign.r_image = [7; 32];
        assert!(decode(&frame, HELLO, Some(foreign)).is_err());
        let mut bad_sid = old; bad_sid.sid_len = 68;
        assert!(encode(message(HELLO, bad_sid)).is_err());
        // Correct self-MAC still yields only Frame DATA, never native peer authority.
        assert_eq!(decode(&frame, HELLO, None).unwrap().binding.c_image, [1; 32]);
    }
}
