//! Portable bounded ciphertext envelope. Neither decoding nor possession grants
//! service authority. The platform codec authenticates all metadata on opening.
use crate::{Error, Result, envelope};
use prost::Message;
pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/morrow.tls_identity.v1.rs"));
}
pub const WINDOWS_PROVIDER: &str = "windows-current-user-dpapi-tls-v1";
pub const MAX_PEM_BYTES: usize = 65_536;
pub const MAX_CIPHERTEXT_BYTES: usize = 144 * 1024;
pub const MAX_RAW_BYTES: usize = MAX_CIPHERTEXT_BYTES + 256;
pub const MAX_CONTAINER_BYTES: usize = MAX_RAW_BYTES + MAX_RAW_BYTES / 255 + 128;
const MAGIC: &[u8; 8] = b"MROWTLS1";

// No Debug: even protected secret material is not diagnostic output.
pub struct Record {
    value: proto::Record,
    container: Vec<u8>,
}
impl Record {
    pub fn encode(value: proto::Record) -> Result<Self> {
        validate(&value)?;
        let container = envelope::pack(MAGIC, &value.encode_to_vec(), MAX_RAW_BYTES)?;
        Ok(Self { value, container })
    }
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let raw = envelope::unpack(MAGIC, bytes, MAX_RAW_BYTES)?;
        preflight(&raw)?;
        let value = proto::Record::decode(raw.as_slice())
            .map_err(|_| Error::Invalid("TLS identity protobuf"))?;
        validate(&value)?;
        if value.encode_to_vec() != raw {
            return Err(Error::Invalid("noncanonical TLS identity"));
        }
        Ok(Self {
            value,
            container: bytes.to_vec(),
        })
    }
    pub fn value(&self) -> &proto::Record {
        &self.value
    }
    pub fn container(&self) -> &[u8] {
        &self.container
    }
    pub fn reference(&self) -> [u8; 32] {
        self.value
            .reference
            .as_slice()
            .try_into()
            .expect("validated reference")
    }
    pub fn store_id(&self) -> [u8; 32] {
        self.value
            .store_id
            .as_slice()
            .try_into()
            .expect("validated store identity")
    }
}
fn validate(value: &proto::Record) -> Result<()> {
    if value.schema_version != 1 {
        return Err(Error::UnsupportedVersion);
    }
    for id in [&value.store_id, &value.reference, &value.certificate_sha256] {
        if id.len() != 32 || id.as_slice() == [0; 32] {
            return Err(Error::Invalid("TLS identity digest"));
        }
    }
    if value.revision == 0 || value.revision > i64::MAX as u64 || value.provider != WINDOWS_PROVIDER
    {
        return Err(Error::Invalid("TLS identity metadata"));
    }
    if value.ciphertext.is_empty() || value.ciphertext.len() > MAX_CIPHERTEXT_BYTES {
        return Err(Error::Limit);
    }
    Ok(())
}
// Bound every allocation and field count before the generated decoder runs.
fn preflight(mut bytes: &[u8]) -> Result<()> {
    use prost::encoding::{WireType, decode_key, decode_varint};
    let mut previous = 0;
    while !bytes.is_empty() {
        let (tag, wire) = decode_key(&mut bytes).map_err(|_| Error::Integrity)?;
        if tag <= previous || tag > 8 {
            return Err(Error::Integrity);
        }
        previous = tag;
        if matches!(tag, 1 | 4 | 5) {
            if wire != WireType::Varint {
                return Err(Error::Integrity);
            }
            decode_varint(&mut bytes).map_err(|_| Error::Integrity)?;
        } else {
            if wire != WireType::LengthDelimited {
                return Err(Error::Integrity);
            }
            let length = decode_varint(&mut bytes).map_err(|_| Error::Integrity)?;
            let limit = match tag {
                2 | 3 | 6 => 32,
                7 => WINDOWS_PROVIDER.len(),
                8 => MAX_CIPHERTEXT_BYTES,
                _ => return Err(Error::Integrity),
            };
            if length == 0 || length > limit as u64 || length > bytes.len() as u64 {
                return Err(Error::Limit);
            }
            bytes = &bytes[length as usize..];
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn value() -> proto::Record {
        proto::Record {
            schema_version: 1,
            store_id: vec![1; 32],
            reference: vec![2; 32],
            revision: 1,
            disabled: false,
            certificate_sha256: vec![3; 32],
            provider: WINDOWS_PROVIDER.into(),
            ciphertext: vec![4; 128],
        }
    }
    #[test]
    fn portable_envelope_roundtrip_and_metadata_bounds() {
        for disabled in [false, true] {
            let mut input = value();
            input.disabled = disabled;
            let record = Record::encode(input.clone()).unwrap();
            let reopened = Record::decode(record.container()).unwrap();
            assert_eq!(reopened.value(), &input);
            assert_eq!(reopened.store_id(), [1; 32]);
            assert_eq!(reopened.reference(), [2; 32]);
        }
        for case in 0..9 {
            let mut input = value();
            match case {
                0 => input.schema_version = 2,
                1 => input.store_id = vec![0; 32],
                2 => input.reference.pop().map(|_| ()).unwrap(),
                3 => input.revision = 0,
                4 => input.revision = i64::MAX as u64 + 1,
                5 => input.provider = "another-provider".into(),
                6 => input.certificate_sha256.clear(),
                7 => input.ciphertext.clear(),
                8 => input.ciphertext = vec![1; MAX_CIPHERTEXT_BYTES + 1],
                _ => unreachable!(),
            }
            assert!(Record::encode(input).is_err());
        }
        let mut maximum = value();
        maximum.ciphertext = vec![1; MAX_CIPHERTEXT_BYTES];
        let record = Record::encode(maximum).unwrap();
        assert!(Record::decode(record.container()).is_ok());
    }
    #[test]
    fn damaged_and_noncanonical_wire_is_rejected_before_copying_fields() {
        let record = Record::encode(value()).unwrap();
        for index in [0, 8, 10, 14, 18, record.container().len() - 1] {
            let mut bytes = record.container().to_vec();
            bytes[index] ^= 1;
            assert!(Record::decode(&bytes).is_err());
        }
        for tail in [
            vec![8, 1],
            vec![72, 1],
            vec![66, 255, 255, 255, 255, 15],
            vec![0],
            vec![40, 0],
        ] {
            let mut raw = value().encode_to_vec();
            raw.extend_from_slice(&tail);
            assert!(Record::decode(&envelope::pack(MAGIC, &raw, MAX_RAW_BYTES).unwrap()).is_err());
        }
        let mut raw = value().encode_to_vec();
        raw.splice(1..2, [0x81, 0]); // same version, overlong varint
        assert!(Record::decode(&envelope::pack(MAGIC, &raw, MAX_RAW_BYTES).unwrap()).is_err());
        assert!(Record::decode(&vec![0; MAX_CONTAINER_BYTES + 1]).is_err());
    }
}
