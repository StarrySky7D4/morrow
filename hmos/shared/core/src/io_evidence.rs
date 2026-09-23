//! Fixed protected IO request/response originals. These immutable bytes are
//! historical material only: never a live dispatch permit, a credential store,
//! or proof that a remote peer accepted or retained anything. Storage,
//! admission and capacity accounting live in the Store layer.
use crate::{Error, Result, envelope, identity, plugin_package::io};
use prost::Message;
use sha2::{Digest, Sha256};
pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/morrow.io_evidence.v1.rs"));
}
pub const MAGIC: &[u8; 8] = b"MROWIOE1";
pub const VERSION: u32 = 1;
pub const MAX_MATERIAL_BYTES: usize = io::MAX_JOB_BYTES as usize;
pub const MAX_RAW_BYTES: usize = MAX_MATERIAL_BYTES + 4096;
pub const MAX_CONTAINER_BYTES: usize = MAX_RAW_BYTES + MAX_RAW_BYTES / 255 + 128;
/// Which side of one protected exchange the stored material belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Request,
    Response,
}
impl Kind {
    pub fn number(self) -> u32 {
        match self {
            Kind::Request => proto::Kind::Request as u32,
            Kind::Response => proto::Kind::Response as u32,
        }
    }
    pub fn from_number(value: u32) -> Result<Self> {
        match proto::Kind::try_from(i32::try_from(value).map_err(|_| Error::UnsupportedVersion)?)
            .map_err(|_| Error::UnsupportedVersion)?
        {
            proto::Kind::Request => Ok(Kind::Request),
            proto::Kind::Response => Ok(Kind::Response),
            proto::Kind::InvalidKind => Err(Error::UnsupportedVersion),
        }
    }
}
/// Worst-case stored container size for one material byte count. The Store
/// reserves this before any original exists; it is a logical quota, never a
/// dispatch permit or physical disk-space guarantee.
pub fn max_container_bytes(material_bytes: u64) -> Result<u64> {
    if material_bytes > io::MAX_JOB_BYTES {
        return Err(Error::Limit);
    }
    let raw = material_bytes + 4096;
    Ok(raw + raw / 255 + 128)
}
/// One bounded protected original. The digest identifies the exact raw
/// protobuf; integrity is not authorship, freshness or remote success.
#[derive(Clone, Debug)]
pub struct Material {
    value: proto::Material,
    kind: Kind,
    request_sha256: [u8; 32],
    payload_sha256: [u8; 32],
    raw: Vec<u8>,
    container: Vec<u8>,
    digest: [u8; 32],
}
impl Material {
    pub fn encode(
        kind: Kind,
        operation_id: &str,
        subject: &str,
        request_sha256: [u8; 32],
        payload: &[u8],
    ) -> Result<Self> {
        identity(operation_id)?;
        identity(subject)?;
        if request_sha256 == [0; 32] {
            return Err(Error::Invalid("empty IO digest"));
        }
        if payload.len() > MAX_MATERIAL_BYTES {
            return Err(Error::Limit);
        }
        Self::from_value(proto::Material {
            schema_version: VERSION,
            operation_id: operation_id.to_string(),
            subject: subject.to_string(),
            kind: kind.number(),
            request_sha256: request_sha256.to_vec(),
            payload_sha256: Sha256::digest(payload).to_vec(),
            payload_bytes: payload.len() as u64,
            payload: payload.to_vec(),
        })
    }
    fn from_value(value: proto::Material) -> Result<Self> {
        let raw = value.encode_to_vec();
        let container = envelope::pack(MAGIC, &raw, MAX_RAW_BYTES)?;
        Self::from_parts(value, raw, container)
    }
    fn from_parts(value: proto::Material, raw: Vec<u8>, container: Vec<u8>) -> Result<Self> {
        let kind = validate(&value)?;
        let payload_sha256 = value
            .payload_sha256
            .as_slice()
            .try_into()
            .expect("validated payload digest");
        let digest = <[u8; 32]>::from(Sha256::digest(&raw));
        Ok(Self {
            request_sha256: fixed_digest(&value.request_sha256)?,
            payload_sha256,
            kind,
            value,
            raw,
            container,
            digest,
        })
    }
    pub fn decode(container: &[u8]) -> Result<Self> {
        let raw = envelope::unpack(MAGIC, container, MAX_RAW_BYTES)?;
        preflight(&raw)?;
        let value = proto::Material::decode(raw.as_slice())
            .map_err(|_| Error::Invalid("IO evidence protobuf"))?;
        if value.encode_to_vec() != raw {
            return Err(Error::Invalid("noncanonical IO evidence"));
        }
        Self::from_parts(value, raw, container.to_vec())
    }
    pub fn kind(&self) -> Kind {
        self.kind
    }
    pub fn operation_id(&self) -> &str {
        &self.value.operation_id
    }
    pub fn subject(&self) -> &str {
        &self.value.subject
    }
    pub fn request_sha256(&self) -> [u8; 32] {
        self.request_sha256
    }
    pub fn payload_sha256(&self) -> [u8; 32] {
        self.payload_sha256
    }
    pub fn payload(&self) -> &[u8] {
        &self.value.payload
    }
    pub fn digest(&self) -> [u8; 32] {
        self.digest
    }
    pub fn raw(&self) -> &[u8] {
        &self.raw
    }
    pub fn container(&self) -> &[u8] {
        &self.container
    }
}
fn fixed_digest(value: &[u8]) -> Result<[u8; 32]> {
    let value: [u8; 32] = value
        .try_into()
        .map_err(|_| Error::Invalid("IO digest length"))?;
    if value == [0; 32] {
        return Err(Error::Invalid("empty IO digest"));
    }
    Ok(value)
}
fn validate(value: &proto::Material) -> Result<Kind> {
    if value.schema_version != VERSION {
        return Err(Error::UnsupportedVersion);
    }
    let kind = Kind::from_number(value.kind)?;
    identity(&value.operation_id)?;
    identity(&value.subject)?;
    fixed_digest(&value.request_sha256)?;
    if value.payload.len() > MAX_MATERIAL_BYTES {
        return Err(Error::Limit);
    }
    if value.payload_bytes != value.payload.len() as u64 {
        return Err(Error::Invalid("IO evidence payload length"));
    }
    let payload_sha256: [u8; 32] = value
        .payload_sha256
        .as_slice()
        .try_into()
        .map_err(|_| Error::Invalid("IO digest length"))?;
    if payload_sha256 == [0; 32] || Sha256::digest(&value.payload).as_slice() != payload_sha256 {
        return Err(Error::Invalid("IO evidence payload digest"));
    }
    Ok(kind)
}
// Strict extension: reject unknown/duplicate fields, wrong wire types and
// oversize owned values BEFORE prost allocates. New material semantics require
// an explicit new version.
fn preflight(mut raw: &[u8]) -> Result<()> {
    use prost::encoding::{WireType, decode_key, decode_varint};
    let mut seen = [false; 9];
    while !raw.is_empty() {
        let (number, wire) =
            decode_key(&mut raw).map_err(|_| Error::Invalid("IO evidence field"))?;
        if !(1..=8).contains(&number) {
            return Err(Error::UnsupportedVersion);
        }
        if std::mem::replace(&mut seen[number as usize], true) {
            return Err(Error::Invalid("duplicate IO evidence field"));
        }
        let scalar = matches!(number, 1 | 4 | 7);
        if wire
            != if scalar {
                WireType::Varint
            } else {
                WireType::LengthDelimited
            }
        {
            return Err(Error::Invalid("IO evidence wire type"));
        }
        let value = decode_varint(&mut raw).map_err(|_| Error::Invalid("IO evidence value"))?;
        if scalar {
            continue;
        }
        let limit = match number {
            2 | 3 => 256,
            5 | 6 => 32,
            _ => MAX_MATERIAL_BYTES,
        };
        if value > limit as u64 {
            return Err(Error::Limit);
        }
        if value > raw.len() as u64 {
            return Err(Error::Invalid("IO evidence span"));
        }
        raw = &raw[value as usize..];
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn sample() -> Material {
        Material::encode(
            Kind::Request,
            "io-operation-1",
            "plugin.io-test",
            [2; 32],
            b"material",
        )
        .unwrap()
    }
    fn pack(raw: &[u8]) -> Vec<u8> {
        envelope::pack(MAGIC, raw, MAX_RAW_BYTES).expect("bounded raw")
    }
    #[test]
    fn roundtrip_preserves_kind_bindings_and_exact_bytes() {
        for kind in [Kind::Request, Kind::Response] {
            let material = Material::encode(
                kind,
                "io-operation-1",
                "plugin.io-test",
                [2; 32],
                b"material",
            )
            .unwrap();
            assert_eq!(material.kind(), kind);
            assert_eq!(material.operation_id(), "io-operation-1");
            assert_eq!(material.subject(), "plugin.io-test");
            assert_eq!(material.request_sha256(), [2; 32]);
            assert_eq!(
                material.payload_sha256(),
                <[u8; 32]>::from(Sha256::digest(b"material"))
            );
            assert_eq!(material.payload(), b"material");
            let restored = Material::decode(material.container()).unwrap();
            assert_eq!(restored.kind(), material.kind());
            assert_eq!(restored.digest(), material.digest());
            assert_eq!(restored.container(), material.container());
            assert_eq!(restored.raw(), material.raw());
            assert_eq!(restored.payload(), material.payload());
            assert_eq!(restored.payload_sha256(), material.payload_sha256());
        }
    }
    #[test]
    fn tampering_truncation_and_trailing_bytes_never_decode() {
        let original = sample().container().to_vec();
        for position in [0, 8, 18, 40, original.len() - 1] {
            let mut bytes = original.clone();
            bytes[position] ^= 0x40;
            assert!(
                Material::decode(&bytes).is_err(),
                "tampered byte {position}"
            );
        }
        for length in 0..original.len() {
            assert!(
                Material::decode(&original[..length]).is_err(),
                "truncated to {length}"
            );
        }
        let mut bytes = original;
        bytes.push(0);
        assert!(Material::decode(&bytes).is_err());
    }
    #[test]
    fn oversize_materials_and_fields_reject_with_limit() {
        let oversized = vec![7u8; MAX_MATERIAL_BYTES + 1];
        assert!(matches!(
            Material::encode(
                Kind::Request,
                "io-operation-1",
                "plugin.io-test",
                [2; 32],
                &oversized
            ),
            Err(Error::Limit)
        ));
        assert!(matches!(
            max_container_bytes(MAX_MATERIAL_BYTES as u64 + 1),
            Err(Error::Limit)
        ));
        let mut long_request = vec![0x2a, 33];
        long_request.extend([1u8; 33]);
        assert!(matches!(
            Material::decode(&pack(&long_request)),
            Err(Error::Limit)
        ));
        // A payload field longer than the material bound exceeds the raw limit.
        let mut long_payload = vec![0x42];
        prost::encoding::encode_varint((MAX_MATERIAL_BYTES + 1) as u64, &mut long_payload);
        assert!(matches!(
            Material::decode(&pack(&long_payload)),
            Err(Error::Limit)
        ));
    }
    #[test]
    fn noncanonical_unknown_duplicate_and_wrong_wire_fields_are_rejected() {
        let raw = sample().raw().to_vec();
        // Canonical schema_version=1 rewritten as an overlong varint.
        let mut tampered = raw.clone();
        assert_eq!(&tampered[..2], &[8, 1]);
        tampered.splice(1..2, [0x81, 0]);
        assert!(Material::decode(&pack(&tampered)).is_err());
        // Unknown field numbers require an explicit new schema version.
        assert!(matches!(
            Material::decode(&pack(&[0x48, 1])),
            Err(Error::UnsupportedVersion)
        ));
        assert!(matches!(
            Material::decode(&pack(&[0x50, 1])),
            Err(Error::UnsupportedVersion)
        ));
        // Duplicate known fields are rejected before allocation.
        let mut duplicate = raw.clone();
        duplicate.extend_from_slice(&[0x12, 1, b'x']);
        assert!(matches!(
            Material::decode(&pack(&duplicate)),
            Err(Error::Invalid("duplicate IO evidence field"))
        ));
        // Wrong wire type for the kind scalar.
        assert!(matches!(
            Material::decode(&pack(&[0x22, 1, 1])),
            Err(Error::Invalid("IO evidence wire type"))
        ));
        // A length-delimited span beyond the remaining bytes is rejected.
        assert!(matches!(
            Material::decode(&pack(&[0x12, 40])),
            Err(Error::Invalid("IO evidence span"))
        ));
    }
    #[test]
    fn identities_digests_lengths_and_containers_are_validated() {
        assert!(Material::encode(Kind::Request, "", "plugin.io-test", [2; 32], b"x").is_err());
        assert!(Material::encode(Kind::Request, "io-operation-1", "", [2; 32], b"x").is_err());
        assert!(
            Material::encode(
                Kind::Request,
                "io-operation-1",
                "plugin.io-test",
                [0; 32],
                b"x"
            )
            .is_err()
        );
        let long_identity = "x".repeat(257);
        assert!(
            Material::encode(
                Kind::Request,
                &long_identity,
                "plugin.io-test",
                [2; 32],
                b"x"
            )
            .is_err()
        );
        assert_eq!(max_container_bytes(0).unwrap(), 4096 + 4096 / 255 + 128);
        assert_eq!(
            max_container_bytes(MAX_MATERIAL_BYTES as u64).unwrap(),
            MAX_CONTAINER_BYTES as u64
        );
        // Hand-built containers must still carry a truthful payload digest.
        let mut wrong_digest = proto::Material {
            schema_version: VERSION,
            operation_id: "io-operation-1".into(),
            subject: "plugin.io-test".into(),
            kind: Kind::Request.number(),
            request_sha256: vec![2; 32],
            payload: b"material".to_vec(),
            payload_bytes: 8,
            payload_sha256: vec![9; 32],
        };
        assert!(Material::decode(&pack(&wrong_digest.encode_to_vec())).is_err());
        wrong_digest.payload_sha256 = Sha256::digest(b"material").to_vec();
        wrong_digest.payload_bytes = 9;
        assert!(Material::decode(&pack(&wrong_digest.encode_to_vec())).is_err());
        wrong_digest.payload_bytes = 8;
        let good = Material::decode(&pack(&wrong_digest.encode_to_vec())).unwrap();
        assert_eq!(good.digest(), sample().digest());
    }
}
