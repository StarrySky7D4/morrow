//! Physical chunk sharing, never a new evidence identity or a runtime grant.
//! All writes are part of the caller's content/migration transaction.
use super::{boundary, sql};
use crate::{
    Error, Result, envelope,
    task_evidence::{self, Evidence},
};
use prost::{
    Message,
    encoding::{DecodeContext, WireType, decode_key, decode_varint, skip_field},
};
use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};
mod proto {
    include!(concat!(env!("OUT_DIR"), "/morrow.evidence_storage.v1.rs"));
}
pub(super) const CHUNK_BYTES: usize = 32 * 1024;
const MAX_CHUNKS: usize = task_evidence::MAX_CONTAINER_BYTES.div_ceil(CHUNK_BYTES);
const MAX_RECIPE_RAW: usize = 64 * 1024;
const MAX_RECIPE_CONTAINER: usize = MAX_RECIPE_RAW + MAX_RECIPE_RAW / 255 + 128;
const MAX_CHUNK_RAW: usize = CHUNK_BYTES + 256;
const MAX_CHUNK_CONTAINER: usize = MAX_CHUNK_RAW + MAX_CHUNK_RAW / 255 + 128;
const RECIPE_MAGIC: &[u8; 8] = b"MORROWQ1";
const CHUNK_MAGIC: &[u8; 8] = b"MORROWH1";
pub(super) const SCHEMA: &str = "
CREATE TABLE evidence_chunks (digest BLOB PRIMARY KEY CHECK(length(digest)=32), payload BLOB NOT NULL) STRICT;
CREATE TABLE task_evidence_chunks (evidence_digest BLOB NOT NULL REFERENCES task_evidence(digest), ordinal INTEGER NOT NULL CHECK(ordinal>=0), digest BLOB NOT NULL REFERENCES evidence_chunks(digest), PRIMARY KEY(evidence_digest,ordinal)) STRICT;
CREATE INDEX task_evidence_chunks_digest ON task_evidence_chunks(digest);";
fn invalid<T>(_: T) -> Error {
    Error::Invalid("evidence physical protobuf")
}
fn transaction(connection: &Connection) -> Result<()> {
    if connection.is_autocommit() {
        return Err(Error::Invalid("evidence chunks require transaction"));
    }
    Ok(())
}
fn digest(bytes: &[u8]) -> Result<[u8; 32]> {
    bytes.try_into().map_err(|_| Error::Integrity)
}
// No nested messages. Check spans/counts/wire types and duplicate known fields before prost allocates.
fn preflight(mut raw: &[u8], recipe: bool) -> Result<()> {
    let mut count = 0usize;
    let mut chunks = 0usize;
    let mut seen = 0u8;
    while !raw.is_empty() {
        count += 1;
        if count > MAX_CHUNKS + 32 {
            return Err(Error::Limit);
        }
        let (field, wire) = decode_key(&mut raw).map_err(invalid)?;
        let known = field <= if recipe { 5 } else { 3 };
        if known {
            if recipe && field == 5 {
                chunks += 1;
                if chunks > MAX_CHUNKS {
                    return Err(Error::Limit);
                }
            } else {
                let bit = 1u8 << field;
                if seen & bit != 0 {
                    return Err(Error::Invalid("duplicate physical field"));
                }
                seen |= bit;
            }
            let expected = if field == 1 || recipe && field == 4 {
                WireType::Varint
            } else {
                WireType::LengthDelimited
            };
            if wire != expected {
                return Err(Error::Invalid("physical wire type"));
            }
        }
        match wire {
            WireType::Varint => {
                let value = decode_varint(&mut raw).map_err(invalid)?;
                if known && field == 1 && value != 1 {
                    return Err(Error::UnsupportedVersion);
                }
                if recipe && field == 4 && value > task_evidence::MAX_CONTAINER_BYTES as u64 {
                    return Err(Error::Limit);
                }
            }
            WireType::LengthDelimited => {
                let length = usize::try_from(decode_varint(&mut raw).map_err(invalid)?)
                    .map_err(|_| Error::Limit)?;
                let limit = if known {
                    if !recipe && field == 3 {
                        CHUNK_BYTES
                    } else {
                        32
                    }
                } else {
                    MAX_RECIPE_RAW.max(MAX_CHUNK_RAW)
                };
                if length > limit {
                    return Err(Error::Limit);
                }
                if length > raw.len() {
                    return Err(Error::Invalid("physical field length"));
                }
                raw = &raw[length..];
            }
            WireType::StartGroup | WireType::EndGroup => {
                return Err(Error::Invalid("physical protobuf group"));
            }
            _ => skip_field(wire, field, &mut raw, DecodeContext::default()).map_err(invalid)?,
        }
    }
    Ok(())
}
fn bounded_payload(
    connection: &Connection,
    query: &str,
    id: [u8; 32],
    limit: usize,
) -> Result<Option<Vec<u8>>> {
    let value = sql(connection
        .query_row(query, params![id.as_slice(), limit as i64], |r| {
            let value = r.get_ref(0)?;
            if matches!(value, rusqlite::types::ValueRef::Null) {
                return Ok(None);
            }
            let raw = value.as_blob()?;
            Ok(if raw.len() > limit {
                None
            } else {
                Some(raw.to_vec())
            })
        })
        .optional())?;
    match value {
        None => Ok(None),
        Some(None) => Err(Error::Limit),
        Some(Some(raw)) => Ok(Some(raw)),
    }
}
fn chunk(connection: &Connection, id: [u8; 32]) -> Result<Option<Vec<u8>>> {
    let Some(bytes) = bounded_payload(
        connection,
        "SELECT CASE WHEN length(payload)<=?2 THEN payload ELSE NULL END FROM evidence_chunks WHERE digest=?1",
        id,
        MAX_CHUNK_CONTAINER,
    )?
    else {
        return Ok(None);
    };
    let raw = envelope::unpack(CHUNK_MAGIC, &bytes, MAX_CHUNK_RAW)?;
    preflight(&raw, false)?;
    let value = proto::Chunk::decode(raw.as_slice()).map_err(invalid)?;
    if value.schema_version != 1 {
        return Err(Error::UnsupportedVersion);
    }
    if value.payload.is_empty() || value.payload.len() > CHUNK_BYTES {
        return Err(Error::Limit);
    }
    if digest(&value.sha256)? != id || Sha256::digest(&value.payload).as_slice() != id {
        return Err(Error::Integrity);
    }
    Ok(Some(value.payload))
}
fn recipe(bytes: &[u8], id: [u8; 32]) -> Result<proto::Recipe> {
    let raw = envelope::unpack(RECIPE_MAGIC, bytes, MAX_RECIPE_RAW)?;
    preflight(&raw, true)?;
    let value = proto::Recipe::decode(raw.as_slice()).map_err(invalid)?;
    if value.schema_version != 1 {
        return Err(Error::UnsupportedVersion);
    }
    if digest(&value.evidence_sha256)? != id {
        return Err(Error::Integrity);
    }
    digest(&value.container_sha256)?;
    let length = usize::try_from(value.container_length).map_err(|_| Error::Limit)?;
    if length == 0
        || length > task_evidence::MAX_CONTAINER_BYTES
        || value.chunk_sha256.len() > MAX_CHUNKS
        || value.chunk_sha256.len() != length.div_ceil(CHUNK_BYTES)
    {
        return Err(Error::Limit);
    }
    for id in &value.chunk_sha256 {
        digest(id)?;
    }
    Ok(value)
}
pub(super) fn read(connection: &Connection, id: [u8; 32]) -> Result<Option<Evidence>> {
    let Some(raw) = bounded_payload(
        connection,
        "SELECT CASE WHEN length(payload)<=?2 THEN payload ELSE NULL END FROM task_evidence WHERE digest=?1",
        id,
        MAX_RECIPE_CONTAINER,
    )?
    else {
        return Ok(None);
    };
    let recipe = recipe(&raw, id)?;
    let mut statement = sql(connection.prepare(
        "SELECT ordinal,digest FROM task_evidence_chunks WHERE evidence_digest=?1 ORDER BY ordinal",
    ))?;
    let mut rows = sql(statement.query([id.as_slice()]))?;
    let mut count = 0usize;
    let mut container = Vec::with_capacity(recipe.container_length as usize);
    while let Some(row) = sql(rows.next())? {
        let ordinal: i64 = sql(row.get(0))?;
        let id = sql(row.get_ref(1))?
            .as_blob()
            .map_err(|_| Error::Integrity)?;
        if count >= recipe.chunk_sha256.len()
            || ordinal != count as i64
            || id != recipe.chunk_sha256[count]
        {
            return Err(Error::Integrity);
        }
        let bytes = chunk(connection, digest(id)?)?.ok_or(Error::Integrity)?;
        let expected = (recipe.container_length as usize - container.len()).min(CHUNK_BYTES);
        if bytes.len() != expected {
            return Err(Error::Integrity);
        }
        container.extend_from_slice(&bytes);
        count += 1;
    }
    if count != recipe.chunk_sha256.len()
        || container.len() != recipe.container_length as usize
        || Sha256::digest(&container).as_slice() != recipe.container_sha256
    {
        return Err(Error::Integrity);
    }
    let evidence = task_evidence::decode(&container, digest(&recipe.evidence_sha256)?)?;
    let version: i64 = sql(connection.query_row("PRAGMA user_version", [], |r| r.get(0)))?;
    if version < 9 && evidence.data().schema_version != task_evidence::VERSION {
        return Err(Error::UnsupportedVersion);
    }
    Ok(Some(evidence))
}
fn write(connection: &Connection, evidence: &Evidence, replace: bool) -> Result<()> {
    transaction(connection)?;
    let id = evidence.digest();
    let pieces = evidence.container().chunks(CHUNK_BYTES);
    let recipe = proto::Recipe {
        schema_version: 1,
        evidence_sha256: id.to_vec(),
        container_sha256: Sha256::digest(evidence.container()).to_vec(),
        container_length: evidence.container().len() as u64,
        chunk_sha256: pieces.clone().map(|p| Sha256::digest(p).to_vec()).collect(),
    };
    if recipe.encoded_len() > MAX_RECIPE_RAW || recipe.chunk_sha256.len() > MAX_CHUNKS {
        return Err(Error::Limit);
    }
    let raw = envelope::pack(RECIPE_MAGIC, &recipe.encode_to_vec(), MAX_RECIPE_RAW)?;
    if replace {
        if sql(connection.execute(
            "UPDATE task_evidence SET payload=?2 WHERE digest=?1",
            params![id.as_slice(), raw],
        ))? != 1
        {
            return Err(Error::Integrity);
        }
    } else {
        let exists: bool = sql(connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM task_evidence WHERE digest=?1)",
            [id.as_slice()],
            |r| r.get(0),
        ))?;
        if exists {
            return Err(Error::Integrity);
        }
        sql(connection.execute(
            "INSERT INTO task_evidence(digest,payload) VALUES(?1,?2)",
            params![id.as_slice(), raw],
        ))?;
    }
    for (ordinal, piece) in pieces.enumerate() {
        let chunk_id: [u8; 32] = Sha256::digest(piece).into();
        if let Some(previous) = chunk(connection, chunk_id)? {
            let referenced: bool = sql(connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM task_evidence_chunks WHERE digest=?1)",
                [chunk_id.as_slice()],
                |r| r.get(0),
            ))?;
            if !referenced || previous != piece {
                return Err(Error::Integrity);
            }
        } else {
            let chunk = proto::Chunk {
                schema_version: 1,
                sha256: chunk_id.to_vec(),
                payload: piece.to_vec(),
            };
            let raw = envelope::pack(CHUNK_MAGIC, &chunk.encode_to_vec(), MAX_CHUNK_RAW)?;
            sql(connection.execute(
                "INSERT INTO evidence_chunks(digest,payload) VALUES(?1,?2)",
                params![chunk_id.as_slice(), raw],
            ))?;
        }
        sql(connection.execute(
            "INSERT INTO task_evidence_chunks(evidence_digest,ordinal,digest) VALUES(?1,?2,?3)",
            params![id.as_slice(), ordinal as i64, chunk_id.as_slice()],
        ))?;
    }
    boundary("after-evidence-chunks");
    Ok(())
}
pub(super) fn insert(connection: &Connection, evidence: &Evidence) -> Result<()> {
    write(connection, evidence, false)
}
pub(super) fn verify_schema(connection: &Connection) -> Result<()> {
    let version: i64 = sql(connection.query_row("PRAGMA user_version", [], |r| r.get(0)))?;
    let count:i64=sql(connection.query_row("SELECT count(*) FROM sqlite_schema WHERE name IN ('evidence_chunks','task_evidence_chunks','task_evidence_chunks_digest')",[],|r|r.get(0)))?;
    if version < 8 && count != 0 || version >= 8 && count != 3 {
        return Err(Error::Integrity);
    }
    Ok(())
}
pub(super) fn verify(connection: &Connection) -> Result<()> {
    let invalid:i64=sql(connection.query_row("SELECT count(*) FROM task_evidence_chunks c LEFT JOIN task_evidence e ON c.evidence_digest=e.digest LEFT JOIN evidence_chunks p ON c.digest=p.digest WHERE e.digest IS NULL OR p.digest IS NULL",[],|r|r.get(0)))?;
    if invalid != 0 {
        return Err(Error::Integrity);
    }
    let orphan:i64=sql(connection.query_row("SELECT count(*) FROM evidence_chunks c WHERE NOT EXISTS(SELECT 1 FROM task_evidence_chunks r WHERE r.digest=c.digest)",[],|r|r.get(0)))?;
    if orphan != 0 {
        return Err(Error::Integrity);
    }
    let mut statement = sql(connection.prepare("SELECT digest FROM task_evidence"))?;
    let mut rows = sql(statement.query([]))?;
    while let Some(row) = sql(rows.next())? {
        let id = digest(
            sql(row.get_ref(0))?
                .as_blob()
                .map_err(|_| Error::Integrity)?,
        )?;
        read(connection, id)?.ok_or(Error::Integrity)?;
    }
    Ok(())
}
/// Caller verifies the complete legacy v7 database first, then invokes this in the same transaction.
/// This function neither commits nor changes user_version. Every original container is preserved.
pub(super) fn migrate(connection: &Connection) -> Result<()> {
    transaction(connection)?;
    let version: i64 = sql(connection.query_row("PRAGMA user_version", [], |r| r.get(0)))?;
    if version != 7 {
        return Err(Error::UnsupportedVersion);
    }
    verify_schema(connection)?;
    sql(connection.execute_batch(SCHEMA))?;
    let mut previous: Option<[u8; 32]> = None;
    loop {
        let next=sql(connection.query_row("SELECT digest,CASE WHEN length(payload)<=?2 THEN payload ELSE NULL END FROM task_evidence WHERE (?1 IS NULL OR digest>?1) ORDER BY digest LIMIT 1",params![previous.as_ref().map(|v|v.as_slice()),task_evidence::MAX_CONTAINER_BYTES as i64],|r|{
            let id=r.get_ref(0)?.as_blob()?;
            if id.len()!=32 {return Ok(None);}
            let value=r.get_ref(1)?;if matches!(value,rusqlite::types::ValueRef::Null) {return Ok(None);}
            let bytes=value.as_blob()?;
            Ok(Some((<[u8;32]>::try_from(id).expect("checked digest"),bytes.to_vec())))
        }).optional())?;
        let Some(next) = next else {
            break;
        };
        let (id, container) = next.ok_or(Error::Integrity)?;
        let evidence = task_evidence::decode(&container, id)?;
        write(connection, &evidence, true)?;
        let recovered = read(connection, id)?.ok_or(Error::Integrity)?;
        if recovered.container() != container || recovered.raw() != evidence.raw() {
            return Err(Error::Integrity);
        }
        previous = Some(id);
    }
    Ok(())
}
