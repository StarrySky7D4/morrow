//! Frozen TaskId migration projection, separate from legacy content projections.
//! Derivation proves correspondence, not execution, source authority or permission.
//! Migrator 1, its request encoding, tasks.capnp digest and output rules are a frozen
//! historical contract. The golden regression test pins their bytes; future behavior
//! needs a new migrator version and must retain this path for old evidence.
use crate::Result;
use capnp::{
    message::{Builder, ReaderOptions},
    serialize,
};
use morrow_core::{
    content::{CardRecord, MAX_RECORD_BYTES},
    content_migration::ContentMigration,
    dispatch::{Connection, HostRuntime},
    plugin_package::Package,
    task::{Invocation, Transform},
    task_evidence::{
        self, Evidence,
        proto::{Batch, Observation, TaskEvidence},
    },
    transaction::{self, Receipt},
};
use morrow_workbench_plugin::{
    tasks_capnp as wire,
    tasks_v2::{self, Baseline},
    tasks_v2_codec,
};
use prost::{
    Message,
    encoding::{WireType, decode_key, decode_varint},
};
use sha2::{Digest, Sha256};

pub mod proto {
    include!(concat!(
        env!("OUT_DIR"),
        "/morrow.workbench.tasks_migration.v1.rs"
    ));
}
pub const INTENT_TYPE: &str = "morrow.workbench.tasks-migration.v1";
const FUEL: u64 = 20_000_000;
const MEMORY: u64 = 16 * 1024 * 1024;

/// Host-selected exact baseline and package. This is a plan, never a grant.
pub struct Plan {
    intent: Vec<u8>,
    invocation: Invocation,
}
impl Plan {
    pub fn prepare(source: &CardRecord, package: &Package) -> Result<Self> {
        let summary = source.summary();
        let base = Baseline::capture(&summary.id, summary.revision, &source.body())?;
        let facts = proto::Facts {
            schema_version: 1,
            operation_id: base.operation_id(),
            source_card: source.encode(),
            package_sha256: package.digest().to_vec(),
            migrator_version: 1,
            target_format_version: 2,
        };
        let (invocation, _, _) = expected(&facts)?;
        let intent = facts.encode_to_vec();
        if intent.len() > task_evidence::MAX_INTENT_BYTES {
            return Err("migration intent budget".into());
        }
        Ok(Self { intent, invocation })
    }
    pub fn invocation(&self) -> &Invocation {
        &self.invocation
    }
    pub fn operation_id(&self) -> &str {
        self.invocation.task_id()
    }
    /// Wrap a real pool capture without changing any of its execution observations.
    pub fn capture(&self, capture: &Evidence) -> Result<ProjectedMigration> {
        let actual = capture.data();
        if actual.schema_version != task_evidence::VERSION
            || actual.batch.is_some()
            || actual.invocation != self.invocation.bytes()
        {
            return Err("migration capture differs from plan".into());
        }
        let evidence = task_evidence::encode(TaskEvidence {
            schema_version: task_evidence::BATCH_VERSION,
            package_archive: actual.package_archive.clone(),
            batch: Some(Batch {
                intent_type: INTENT_TYPE.into(),
                intent: self.intent.clone(),
                total_fuel: FUEL,
                observations: vec![Observation {
                    invocation: actual.invocation.clone(),
                    budget: actual.budget,
                    backend: actual.backend.clone(),
                    completion: actual.completion.clone(),
                    fault: actual.fault,
                    exit_code: actual.exit_code,
                    observed_host_calls: actual.observed_host_calls,
                    fuel_remaining: actual.fuel_remaining,
                }],
            }),
            ..Default::default()
        })?;
        derive(&evidence)
    }
}

pub struct ProjectedMigration {
    change: ContentMigration,
    command: Vec<u8>,
    card: CardRecord,
    evidence: Evidence,
    package_digest: [u8; 32],
}
impl ProjectedMigration {
    pub fn source_card(&self) -> &[u8] {
        &self.change.source_card
    }
    pub fn card(&self) -> &CardRecord {
        &self.card
    }
    pub fn command(&self) -> &[u8] {
        &self.command
    }
    pub fn evidence(&self) -> &Evidence {
        &self.evidence
    }
    /// No grants are minted here. Package identity and the live connection are required,
    /// including for historical receipt retries after newer content has been committed.
    pub fn commit(
        &self,
        host: &mut HostRuntime,
        connection: &Connection,
        clock: impl FnMut() -> u64,
    ) -> Result<Receipt> {
        if connection.package_digest() != Some(self.package_digest) {
            return Err("migration connection package mismatch".into());
        }
        Ok(host.migrate_content_guarded_with_evidence(
            connection,
            &self.change,
            std::slice::from_ref(&self.evidence),
            clock,
            |_| Ok(()),
        )?)
    }
}

// Six singular fields; reject duplicates/unknown semantics before owned allocation.
fn decode_facts(mut raw: &[u8]) -> Result<proto::Facts> {
    if raw.len() > task_evidence::MAX_INTENT_BYTES {
        return Err("migration facts budget".into());
    }
    let original = raw;
    let mut seen = 0u8;
    while !raw.is_empty() {
        let (tag, wire) = decode_key(&mut raw)?;
        if !(1..=6).contains(&tag) || seen & (1 << tag) != 0 {
            return Err("migration facts field".into());
        }
        seen |= 1 << tag;
        if matches!(tag, 2..=4) {
            if wire != WireType::LengthDelimited {
                return Err("migration facts wire".into());
            }
            let length = usize::try_from(decode_varint(&mut raw)?)?;
            let limit = match tag {
                2 => 256,
                3 => MAX_RECORD_BYTES,
                _ => 32,
            };
            if length > limit || length > raw.len() {
                return Err("migration facts length".into());
            }
            raw = &raw[length..];
        } else {
            if wire != WireType::Varint || decode_varint(&mut raw)? > u64::from(u32::MAX) {
                return Err("migration facts integer".into());
            }
        }
    }
    let value = proto::Facts::decode(original)?;
    if value.encode_to_vec() != original {
        return Err("noncanonical migration facts".into());
    }
    Ok(value)
}

fn expected(facts: &proto::Facts) -> Result<(Invocation, ContentMigration, Vec<u8>)> {
    if facts.schema_version != 1
        || facts.migrator_version != 1
        || facts.target_format_version != 2
        || facts.package_sha256.len() != 32
    {
        return Err("unsupported migration facts".into());
    }
    let source = CardRecord::decode(&facts.source_card)?;
    let summary = source.summary();
    if summary.type_id != "org.morrow.idea" || summary.format_version != 1 {
        return Err("migration requires a V1 idea".into());
    }
    let body = source.body();
    let base = Baseline::capture(&summary.id, summary.revision, &body)?;
    if facts.operation_id != base.operation_id() {
        return Err("migration operation baseline".into());
    }
    let mut message = Builder::new_default();
    {
        let mut r = message.init_root::<wire::request::Builder>();
        r.set_version(2);
        r.set_digest(&tasks_v2_codec::digest());
        r.set_action(wire::Action::Migrate);
        r.set_card_id(summary.id.as_str());
        r.set_title(summary.title.as_str());
        r.set_revision(summary.revision);
        r.set_base_revision(base.revision);
        r.set_source_digest(&base.sha256);
        r.set_properties(&body);
    }
    let input = serialize::write_message_to_words(&message);
    // Migrator 1 is deterministic. Preflight also reserves the complete target Card budget;
    // the actual captured guest response must equal this frozen contract's result.
    let output = tasks_v2_codec::process(&input)?;
    let reader = serialize::read_message_from_flat_slice(
        &mut output.as_slice(),
        ReaderOptions {
            traversal_limit_in_words: Some(16384),
            nesting_limit: 16,
        },
    )?;
    let response = reader.get_root::<wire::response::Reader>()?;
    let migrated_body = response.get_properties()?.to_vec();
    let migrated = tasks_v2::decode(&summary.id, &summary.title, &migrated_body)?;
    let origin = migrated.origin.as_ref().ok_or("missing migration origin")?;
    if origin.card_id != summary.id
        || origin.source_revision != summary.revision
        || origin.source_sha256 != base.sha256
        || origin.original_properties != body
        || origin.original_title != summary.title
        || origin.migrator_version != facts.migrator_version
        || origin.target_version != facts.target_format_version
    {
        return Err("migration origin differs from frozen source".into());
    }
    let change = ContentMigration {
        operation_id: facts.operation_id.clone(),
        source_card: facts.source_card.clone(),
        target_format_version: 2,
        body: migrated_body,
        // A format conversion alone does not invent a new preview or alter its unknown fields.
        preview_text: summary.preview_text,
    };
    change.propose(&source)?;
    let invocation = Invocation::new_transform(
        &facts.operation_id,
        Transform {
            handler: "workbench.tasks.v2".into(),
            input_type: "morrow.workbench.tasks.request.v2".into(),
            output_type: "morrow.workbench.tasks.response.v2".into(),
            input,
        },
    )?;
    Ok((invocation, change, output))
}

/// Pure exact projection; does not execute the archived Wasm or consult current state.
pub fn derive(evidence: &Evidence) -> Result<ProjectedMigration> {
    let actual = evidence.data();
    let batch = actual.batch.as_ref().ok_or("missing migration batch")?;
    if actual.schema_version != task_evidence::BATCH_VERSION
        || batch.intent_type != INTENT_TYPE
        || batch.observations.len() != 1
        || batch.total_fuel != FUEL
    {
        return Err("unsupported migration batch".into());
    }
    let facts = decode_facts(&batch.intent)?;
    let package = Package::decode(&actual.package_archive)?;
    if facts.package_sha256 != package.digest() {
        return Err("migration package substitution".into());
    }
    let (invocation, change, expected_output) = expected(&facts)?;
    let observation = &batch.observations[0];
    let budget = observation
        .budget
        .as_ref()
        .ok_or("missing migration budget")?;
    if observation.invocation != invocation.bytes()
        || observation.fault != 0
        || observation.exit_code != Some(0)
        || observation.observed_host_calls != 0
        || observation.backend != task_evidence::BACKEND
        || budget.fuel > FUEL
        || budget.memory_bytes > MEMORY
        || budget.host_calls > 16
    {
        return Err("migration observation mismatch".into());
    }
    let output = invocation.verify_output(&observation.completion)?;
    if output.bytes != expected_output {
        return Err("migration output differs from frozen migrator".into());
    }
    let card = change.propose(&change.source()?)?;
    let command = transaction::migration_command(&change)?;
    Ok(ProjectedMigration {
        change,
        card,
        command,
        evidence: evidence.clone(),
        package_digest: package.digest(),
    })
}

pub fn verify_commit(
    commit: &transaction::proto::Commit,
    evidence: &Evidence,
) -> Result<ProjectedMigration> {
    let projection = derive(evidence)?;
    let summary = projection.card.summary();
    if commit.schema_version != 2
        || commit.task_evidence_sha256 != vec![evidence.digest().to_vec()]
        || commit.command != projection.command
        || commit.command_sha256 != Sha256::digest(&projection.command).as_slice()
        || commit.operation_id != projection.change.operation_id
        || commit.event_id != projection.change.operation_id
        || commit.card_id != summary.id
        || commit.revision != summary.revision
        || commit.content_sha256 != Sha256::digest(projection.card.encode()).as_slice()
        || commit.attachment_sha256
            != projection
                .card
                .attachments()
                .iter()
                .map(|a| a.sha256.to_vec())
                .collect::<Vec<_>>()
    {
        return Err("migration projection differs from commit".into());
    }
    Ok(projection)
}
