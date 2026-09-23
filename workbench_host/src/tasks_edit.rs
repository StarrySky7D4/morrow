//! Frozen TaskId V2 edit projection. The pure guest output is bound to exact host facts.
//! Derivation proves correspondence, not execution, source authority or permission.
//! Edit contract 1 and its request encoding remain available for historical replay.
use crate::Result;
use capnp::{
    message::{Builder, ReaderOptions},
    serialize,
};
use morrow_core::{
    content::{CardRecord, MAX_RECORD_BYTES},
    dispatch::{Connection, HostRuntime},
    plugin_package::Package,
    task::{Invocation, Transform},
    task_evidence::{
        self, Evidence,
        proto::{Batch, Observation, TaskEvidence},
    },
    transaction::{self, Receipt},
    versioned_content_change::VersionedContentChange,
};
use morrow_workbench_plugin::{
    tasks_capnp as wire,
    tasks_v2::{self, Command},
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
        "/morrow.workbench.tasks_edit.v1.rs"
    ));
}
pub const INTENT_TYPE: &str = "morrow.workbench.tasks-edit.v1";
const FUEL: u64 = 20_000_000;
const MEMORY: u64 = 16 * 1024 * 1024;

/// Host-selected original Card, operation, package and one typed edit command.
pub struct Plan {
    intent: Vec<u8>,
    invocation: Invocation,
}
impl Plan {
    pub fn prepare(
        source: &CardRecord,
        package: &Package,
        operation: &str,
        command: &Command,
    ) -> Result<Self> {
        let facts = facts_for(source, package.digest(), operation, command)?;
        let intent = facts.encode_to_vec();
        if intent.len() > task_evidence::MAX_INTENT_BYTES {
            return Err("TaskId edit intent budget".into());
        }
        let facts = decode_facts(&intent)?;
        let (invocation, _, _) = expected(&facts)?;
        Ok(Self { intent, invocation })
    }
    pub fn invocation(&self) -> &Invocation {
        &self.invocation
    }
    pub fn operation_id(&self) -> &str {
        self.invocation.task_id()
    }
    /// Wrap the actual single-task capture without rewriting execution observations.
    pub fn capture(&self, capture: &Evidence) -> Result<ProjectedEdit> {
        let actual = capture.data();
        if actual.schema_version != task_evidence::VERSION
            || actual.batch.is_some()
            || actual.invocation != self.invocation.bytes()
        {
            return Err("TaskId edit capture differs from plan".into());
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

pub struct ProjectedEdit {
    facts: proto::Facts,
    change: VersionedContentChange,
    command: Vec<u8>,
    card: CardRecord,
    evidence: Evidence,
    package_digest: [u8; 32],
}
impl ProjectedEdit {
    pub fn card(&self) -> &CardRecord {
        &self.card
    }
    pub fn command(&self) -> &[u8] {
        &self.command
    }
    pub fn evidence(&self) -> &Evidence {
        &self.evidence
    }
    pub fn source_card(&self) -> &[u8] {
        &self.change.source_card
    }
    pub fn operation_id(&self) -> &str {
        &self.change.operation_id
    }
    /// Compare the original normalized intent, including the source revision.
    /// Historical receipt retries may use this without looking at the current Card.
    pub fn matches_intent(
        &self,
        operation: &str,
        expected_revision: u64,
        command: &Command,
    ) -> Result<bool> {
        let source = self.change.source()?;
        if source.summary().revision != expected_revision {
            return Ok(false);
        }
        let candidate = facts_for(&source, self.package_digest, operation, command)?;
        Ok(candidate.encode_to_vec() == self.facts.encode_to_vec())
    }
    /// The owner validates format-specific intent and chooses a live package-bound
    /// connection. Core then checks its EditContent grant inside the transaction.
    pub fn commit(
        &self,
        host: &mut HostRuntime,
        connection: &Connection,
        clock: impl FnMut() -> u64,
    ) -> Result<Receipt> {
        if connection.package_digest() != Some(self.package_digest) {
            return Err("TaskId edit connection package mismatch".into());
        }
        Ok(host.edit_versioned_content_guarded_with_evidence(
            connection,
            &self.change,
            std::slice::from_ref(&self.evidence),
            clock,
            |_| Ok(()),
        )?)
    }
}

fn facts_for(
    source: &CardRecord,
    package_digest: [u8; 32],
    operation: &str,
    command: &Command,
) -> Result<proto::Facts> {
    if operation.len() > 256 {
        return Err("TaskId edit operation budget".into());
    }
    let bounded = match command {
        Command::SetCompletion { id, .. } | Command::Remove(id) => id.len() <= 256,
        Command::Rename { id, text } | Command::Add { id, text } => {
            id.len() <= 256 && text.len() <= 2048
        }
        Command::Reorder(order) => order.len() <= 128 && order.iter().all(|id| id.len() <= 256),
        Command::SetStage(text) | Command::CompleteAllAndSetStage(text) => text.len() <= 2048,
    };
    if !bounded {
        return Err("TaskId edit command budget".into());
    }
    let mut facts = proto::Facts {
        schema_version: 1,
        operation_id: operation.into(),
        source_card: source.encode(),
        package_sha256: package_digest.to_vec(),
        ..Default::default()
    };
    match command {
        Command::SetCompletion { id, complete } => {
            facts.action = 1;
            facts.task_id = id.clone();
            facts.complete = *complete;
        }
        Command::Rename { id, text } => {
            facts.action = 2;
            facts.task_id = id.clone();
            facts.text = text.clone();
        }
        Command::Reorder(order) => {
            facts.action = 3;
            facts.order = order.clone();
        }
        Command::SetStage(stage) => {
            facts.action = 4;
            facts.text = stage.clone();
        }
        Command::CompleteAllAndSetStage(stage) => {
            facts.action = 5;
            facts.text = stage.clone();
        }
        Command::Add { id, text } => {
            facts.action = 6;
            facts.task_id = id.clone();
            facts.text = text.clone();
        }
        Command::Remove(id) => {
            facts.action = 7;
            facts.task_id = id.clone();
        }
    }
    Ok(facts)
}

// Exact facts wire: singular fields cannot repeat, unknown fields have no semantics,
// and a re-encode must reproduce all bytes. Bound lengths before owned decoding.
fn decode_facts(mut raw: &[u8]) -> Result<proto::Facts> {
    if raw.len() > task_evidence::MAX_INTENT_BYTES {
        return Err("TaskId edit facts budget".into());
    }
    let original = raw;
    let mut seen = 0u16;
    let mut order_count = 0usize;
    while !raw.is_empty() {
        let (tag, wire) = decode_key(&mut raw)?;
        if !(1..=9).contains(&tag) || (tag != 9 && seen & (1 << tag) != 0) {
            return Err("TaskId edit facts field".into());
        }
        seen |= 1 << tag;
        if matches!(tag, 1 | 5 | 8) {
            if wire != WireType::Varint {
                return Err("TaskId edit facts wire".into());
            }
            let n = decode_varint(&mut raw)?;
            if n > u64::from(u32::MAX) || (tag == 8 && n > 1) {
                return Err("TaskId edit facts integer".into());
            }
        } else {
            if wire != WireType::LengthDelimited {
                return Err("TaskId edit facts wire".into());
            }
            let length = usize::try_from(decode_varint(&mut raw)?)?;
            let limit = match tag {
                2 | 6 | 9 => 256,
                3 => MAX_RECORD_BYTES,
                4 => 32,
                7 => 2048,
                _ => unreachable!(),
            };
            if length > limit || length > raw.len() {
                return Err("TaskId edit facts length".into());
            }
            if tag == 9 {
                order_count += 1;
                if order_count > 128 {
                    return Err("TaskId edit order budget".into());
                }
            }
            raw = &raw[length..];
        }
    }
    let value = proto::Facts::decode(original)?;
    if value.encode_to_vec() != original {
        return Err("noncanonical TaskId edit facts".into());
    }
    Ok(value)
}
fn command_from_facts(facts: &proto::Facts) -> Result<Command> {
    let id = &facts.task_id;
    let text = &facts.text;
    let order = &facts.order;
    let command = match facts.action {
        1 if !id.is_empty() && text.is_empty() && order.is_empty() => Command::SetCompletion {
            id: id.clone(),
            complete: facts.complete,
        },
        2 if !id.is_empty() && !text.is_empty() && order.is_empty() && !facts.complete => {
            Command::Rename {
                id: id.clone(),
                text: text.clone(),
            }
        }
        3 if id.is_empty() && text.is_empty() && !facts.complete => Command::Reorder(order.clone()),
        4 if id.is_empty() && !text.is_empty() && order.is_empty() && !facts.complete => {
            Command::SetStage(text.clone())
        }
        5 if id.is_empty() && !text.is_empty() && order.is_empty() && !facts.complete => {
            Command::CompleteAllAndSetStage(text.clone())
        }
        6 if !id.is_empty() && !text.is_empty() && order.is_empty() && !facts.complete => {
            Command::Add {
                id: id.clone(),
                text: text.clone(),
            }
        }
        7 if !id.is_empty() && text.is_empty() && order.is_empty() && !facts.complete => {
            Command::Remove(id.clone())
        }
        _ => return Err("TaskId edit action or unrelated parameter".into()),
    };
    Ok(command)
}

fn request_for(source: &CardRecord, command: &Command) -> Vec<u8> {
    let summary = source.summary();
    let body = source.body();
    let mut message = Builder::new_default();
    {
        let mut r = message.init_root::<wire::request::Builder>();
        r.set_version(2);
        r.set_digest(&tasks_v2_codec::digest());
        r.set_card_id(summary.id.as_str());
        r.set_title(summary.title.as_str());
        r.set_revision(summary.revision);
        r.set_properties(&body);
        // Migration-only baseRevision/sourceDigest and unused edit arguments remain
        // at their canonical defaults, so no extra identity can enter the guest.
        match command {
            Command::SetCompletion { id, complete } => {
                r.set_action(wire::Action::SetCompletion);
                r.set_task_id(id.as_str());
                r.set_complete(*complete);
            }
            Command::Rename { id, text } => {
                r.set_action(wire::Action::Rename);
                r.set_task_id(id.as_str());
                r.set_text(text.as_str());
            }
            Command::Reorder(order) => {
                r.set_action(wire::Action::Reorder);
                let mut selected = r.init_order(order.len() as u32);
                for (index, id) in order.iter().enumerate() {
                    selected.set(index as u32, id.as_str());
                }
            }
            Command::SetStage(stage) => {
                r.set_action(wire::Action::SetStage);
                r.set_text(stage.as_str());
            }
            Command::CompleteAllAndSetStage(stage) => {
                r.set_action(wire::Action::CompleteAllAndSetStage);
                r.set_text(stage.as_str());
            }
            Command::Add { id, text } => {
                r.set_action(wire::Action::Add);
                r.set_task_id(id.as_str());
                r.set_text(text.as_str());
            }
            Command::Remove(id) => {
                r.set_action(wire::Action::Remove);
                r.set_task_id(id.as_str());
            }
        }
    }
    serialize::write_message_to_words(&message)
}

fn expected(facts: &proto::Facts) -> Result<(Invocation, VersionedContentChange, Vec<u8>)> {
    if facts.schema_version != 1 || facts.package_sha256.len() != 32 {
        return Err("unsupported TaskId edit facts".into());
    }
    let source = CardRecord::decode(&facts.source_card)?;
    let summary = source.summary();
    if summary.type_id != "org.morrow.idea"
        || summary.format_version != 2
        || source.encode() != facts.source_card
    {
        return Err("TaskId edit requires exact V2 idea".into());
    }
    let command = command_from_facts(facts)?;
    let input = request_for(&source, &command);
    if input.len() > 65536 {
        return Err("TaskId edit request budget".into());
    }
    // Edit contract 1 freezes the exact guest implementation. Reserve the whole
    // resulting Card and persistent command before any guest task is submitted.
    let output = tasks_v2_codec::process(&input)?;
    let reader = serialize::read_message_from_flat_slice(
        &mut output.as_slice(),
        ReaderOptions {
            traversal_limit_in_words: Some(16384),
            nesting_limit: 16,
        },
    )?;
    let response = reader.get_root::<wire::response::Reader>()?;
    if response.get_version() != 2 || response.get_digest()? != tasks_v2_codec::digest() {
        return Err("TaskId edit response contract".into());
    }
    let body = response.get_properties()?.to_vec();
    tasks_v2::decode(&summary.id, &summary.title, &body)?;
    let change = VersionedContentChange {
        operation_id: facts.operation_id.clone(),
        source_card: facts.source_card.clone(),
        title: summary.title.clone(),
        body,
        preview_text: summary.preview_text,
        attachments: None,
    };
    change.propose(&source)?;
    transaction::versioned_content_command(&change)?;
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

/// Pure projection from pinned facts and recorded bytes; it does not run Wasm.
pub fn derive(evidence: &Evidence) -> Result<ProjectedEdit> {
    let actual = evidence.data();
    let batch = actual.batch.as_ref().ok_or("missing TaskId edit batch")?;
    if actual.schema_version != task_evidence::BATCH_VERSION
        || batch.intent_type != INTENT_TYPE
        || batch.observations.len() != 1
        || batch.total_fuel != FUEL
    {
        return Err("unsupported TaskId edit batch".into());
    }
    let facts = decode_facts(&batch.intent)?;
    let package = Package::decode(&actual.package_archive)?;
    if facts.package_sha256 != package.digest() {
        return Err("TaskId edit package substitution".into());
    }
    let (invocation, change, expected_output) = expected(&facts)?;
    let observation = &batch.observations[0];
    let budget = observation
        .budget
        .as_ref()
        .ok_or("missing TaskId edit budget")?;
    if observation.invocation != invocation.bytes()
        || observation.fault != 0
        || observation.exit_code != Some(0)
        || observation.observed_host_calls != 0
        || observation.backend != task_evidence::BACKEND
        || budget.fuel > FUEL
        || budget.memory_bytes > MEMORY
        || budget.host_calls > 16
    {
        return Err("TaskId edit observation mismatch".into());
    }
    let output = invocation.verify_output(&observation.completion)?;
    if output.bytes != expected_output {
        return Err("TaskId edit output differs from frozen contract".into());
    }
    let card = change.propose(&change.source()?)?;
    let command = transaction::versioned_content_command(&change)?;
    Ok(ProjectedEdit {
        facts,
        change,
        command,
        card,
        evidence: evidence.clone(),
        package_digest: package.digest(),
    })
}

pub fn verify_commit(
    commit: &transaction::proto::Commit,
    evidence: &Evidence,
) -> Result<ProjectedEdit> {
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
                .map(|item| item.sha256.to_vec())
                .collect::<Vec<_>>()
    {
        return Err("TaskId edit projection differs from commit".into());
    }
    Ok(projection)
}

#[cfg(test)]
mod tests {
    use super::*;
    use morrow_core::{
        content_migration::ContentMigration, plugin_package::proto::TransformHandler,
        task_evidence::proto::ExecutionBudget,
    };
    use morrow_workbench_plugin::{Idea, persistence, tasks_v2::Baseline};

    fn fixture() -> (CardRecord, Package, Command) {
        let old = Idea {
            id: "card".into(),
            title: "Original".into(),
            category: "进行中".into(),
            stage: "计划中".into(),
            todos: vec!["one".into()],
            ..Default::default()
        };
        let old_body = persistence::encode(&old, None).unwrap();
        let base = Baseline::capture("card", 1, &old_body).unwrap();
        let body = tasks_v2::migrate(&base, "card", 1, "Original", &old_body).unwrap();
        let properties = tasks_v2::decode("card", "Original", &body).unwrap();
        let original = CardRecord::new("card", "org.morrow.idea", 1, "Original", old_body).unwrap();
        let source = ContentMigration {
            operation_id: "migration".into(),
            source_card: original.encode(),
            target_format_version: 2,
            body,
            preview_text: String::new(),
        }
        .propose(&original)
        .unwrap();
        let command = Command::SetCompletion {
            id: properties.tasks[0].id.clone(),
            complete: true,
        };
        let wasm = b"\0asm\x01\0\0\0";
        let package = Package::build(
            Package::manifest_for_transform(
                "test.task-edit",
                "1.0.0",
                wasm,
                vec![TransformHandler {
                    handler: "workbench.tasks.v2".into(),
                    input_type: "morrow.workbench.tasks.request.v2".into(),
                    output_type: "morrow.workbench.tasks.response.v2".into(),
                    max_input_bytes: 65536,
                    max_output_bytes: 65536,
                }],
            ),
            wasm,
        )
        .unwrap();
        (source, package, command)
    }

    fn synthetic_single(package: &Package, invocation: &Invocation, output: &[u8]) -> Evidence {
        task_evidence::encode(TaskEvidence {
            schema_version: task_evidence::VERSION,
            package_archive: package.archive().to_vec(),
            invocation: invocation.bytes().to_vec(),
            budget: Some(ExecutionBudget {
                fuel: FUEL,
                memory_bytes: MEMORY,
                host_calls: 16,
            }),
            backend: task_evidence::BACKEND.into(),
            completion: invocation.output_completion(output).unwrap(),
            fault: 0,
            exit_code: Some(0),
            observed_host_calls: 0,
            fuel_remaining: FUEL - 1,
            batch: None,
        })
        .unwrap()
    }

    #[test]
    fn canonical_facts_reject_duplicate_unknown_noncanonical_and_wrong_wire() {
        let (source, package, command) = fixture();
        let facts = facts_for(&source, package.digest(), "edit", &command).unwrap();
        let raw = facts.encode_to_vec();
        assert_eq!(decode_facts(&raw).unwrap(), facts);
        let mut duplicate = raw.clone();
        duplicate.extend_from_slice(&[0x12, 1, b'x']);
        assert!(decode_facts(&duplicate).is_err());
        let mut unknown = raw.clone();
        unknown.extend_from_slice(&[0x50, 1]);
        assert!(decode_facts(&unknown).is_err());
        let mut noncanonical = vec![0x08, 0x81, 0x00];
        noncanonical.extend_from_slice(&raw[2..]);
        assert!(decode_facts(&noncanonical).is_err());
        let mut wire = raw;
        wire.extend_from_slice(&[0x48, 1]);
        assert!(decode_facts(&wire).is_err()); // Wrong wire type for repeated order.
    }

    #[test]
    fn unrelated_action_fields_wrong_source_and_operation_identity_are_rejected() {
        let (source, package, command) = fixture();
        let mut facts = facts_for(&source, package.digest(), "edit", &command).unwrap();
        facts.text = "unexpected".into();
        assert!(expected(&facts).is_err());
        facts.text.clear();
        facts.order.push("unexpected".into());
        assert!(expected(&facts).is_err());
        facts.order.clear();
        facts.action = 8; // Project is not a persisted edit.
        assert!(expected(&facts).is_err());
        facts.action = 1;
        facts.source_card = CardRecord::new("card", "org.morrow.idea", 1, "old", vec![])
            .unwrap()
            .encode();
        assert!(expected(&facts).is_err());
        facts.source_card = CardRecord::new("card", "other", 2, "old", vec![])
            .unwrap()
            .encode();
        assert!(expected(&facts).is_err());
        assert!(Plan::prepare(&source, &package, "bad/name", &command).is_err());
        assert!(Plan::prepare(&source, &package, "", &command).is_err());
        assert!(
            Plan::prepare(
                &source,
                &package,
                "edit",
                &Command::Reorder(vec!["x".into(); 129])
            )
            .is_err()
        );
    }

    #[test]
    fn pure_capture_receipt_and_intent_bind_exact_frozen_contract() {
        let (source, package, command) = fixture();
        let plan = Plan::prepare(&source, &package, "edit", &command).unwrap();
        let facts = decode_facts(&plan.intent).unwrap();
        let (_, _, output) = expected(&facts).unwrap();
        let actual = synthetic_single(&package, plan.invocation(), &output);
        let projection = plan.capture(&actual).unwrap();
        assert_eq!(projection.source_card(), source.encode());
        assert_eq!(projection.operation_id(), "edit");
        assert!(projection.matches_intent("edit", 2, &command).unwrap());
        assert!(!projection.matches_intent("other", 2, &command).unwrap());
        assert!(!projection.matches_intent("edit", 3, &command).unwrap());
        assert!(
            !projection
                .matches_intent("edit", 2, &Command::SetStage("进行中".into()))
                .unwrap()
        );
        let event = transaction::encode_commit_with_evidence(
            projection.command().to_vec(),
            projection.card(),
            &[projection.evidence().digest()],
        )
        .unwrap();
        let (commit, _) = transaction::decode_commit(&event).unwrap();
        verify_commit(&commit, projection.evidence()).unwrap();
        assert_eq!(
            hex(&Sha256::digest(
                &plan.invocation().transform().unwrap().input
            )),
            "79ae722a5d47cc594d6dc3364740316b656beec595f8f200d87ed2aa3ea4e8c4"
        );
        assert_eq!(
            hex(&Sha256::digest(&output)),
            "2e0e28e49aba4829d20cbab174069fb2a2db3b9e8a81bc272314ebc5fe7296f6"
        );
        assert_eq!(
            hex(&Sha256::digest(projection.command())),
            "fada43e40defafeef69891efb20b4fc6a52c80d2dc5f0cd38290619ffec7f48c"
        );
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }
}
