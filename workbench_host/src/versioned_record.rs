//! Trusted, read-only adapter for the two supported idea formats.
//! Format 2 is a task record in its own right; its source format and provenance
//! remain in the validated V2 properties. Decoding is not a guest execution,
//! content authorization, or a migration/commit receipt.
use crate::{Record, Result};
use morrow_core::content::CardRecord;
use morrow_workbench_plugin::{persistence, tasks_v2};
use std::{error::Error, fmt};

pub enum VersionedRecord {
    Legacy(Record),
    Tasks(TaskRecord),
}

pub struct TaskRecord {
    pub id: String,
    pub title: String,
    pub revision: u64,
    pub properties: tasks_v2::Properties,
    pub projection: tasks_v2::Projection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unsupported {
    pub type_id: String,
    pub format_version: u32,
}

impl fmt::Display for Unsupported {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "unsupported content type {} format {}",
            self.type_id, self.format_version
        )
    }
}
impl Error for Unsupported {}

/// Read a validated Card as its actual format. This does not grant access to
/// the Card or imply that its migration evidence was executed or committed.
pub fn decode(card: &CardRecord) -> Result<VersionedRecord> {
    let summary = card.summary();
    if summary.type_id != "org.morrow.idea" || !matches!(summary.format_version, 1 | 2) {
        return Err(Box::new(Unsupported {
            type_id: summary.type_id,
            format_version: summary.format_version,
        }));
    }
    let body = card.body();
    match summary.format_version {
        1 => Ok(VersionedRecord::Legacy(Record {
            idea: persistence::decode(&summary.id, &summary.title, &body)?,
            revision: summary.revision,
        })),
        2 => {
            // The plugin's pure decoder validates all common fields, TaskIds,
            // completion states, origin and source digest. It does not consult
            // the outer Card's attachment list, which we bind here by position.
            let properties = tasks_v2::decode(&summary.id, &summary.title, &body)?;
            let attachments = card.attachments();
            if properties.assets.len() != attachments.len()
                || properties
                    .assets
                    .iter()
                    .zip(&attachments)
                    .any(|(asset, outer)| {
                        asset.id != outer.id
                            || asset.name != outer.display_name
                            || asset.bytes != outer.byte_length
                    })
            {
                return Err("V2 Card attachment metadata mismatch".into());
            }
            // Keep stage and each completion count from the existing V2
            // projection. Legacy checklist/stage inference never runs here.
            let projection = tasks_v2::project(&summary.id, &summary.title, &body)?;
            Ok(VersionedRecord::Tasks(TaskRecord {
                id: summary.id,
                title: summary.title,
                revision: summary.revision,
                properties,
                projection,
            }))
        }
        _ => unreachable!("unsupported versions were rejected above"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use morrow_core::{content::Attachment, content_migration::ContentMigration};
    use morrow_workbench_plugin::{
        Asset, Idea,
        tasks_v2::{Baseline, Completion},
    };

    fn sample() -> (CardRecord, CardRecord) {
        let assets = vec![
            Asset {
                id: "asset-image".into(),
                name: "picture.png".into(),
                kind: "image".into(),
                bytes: 12,
            },
            Asset {
                id: "asset-file".into(),
                name: "notes.txt".into(),
                kind: "file".into(),
                bytes: 23,
            },
        ];
        let idea = Idea {
            id: "versioned-example".into(),
            title: "Task example".into(),
            category: "进行中".into(),
            stage: "计划中".into(),
            todos: vec!["same".into(), "same".into(), "open".into()],
            completed: vec!["same".into()],
            assets,
            ..Default::default()
        };
        let mut source_body = persistence::encode(&idea, None).unwrap();
        // V1 field 100 is unknown; it must remain in Origin.original_properties.
        source_body.extend_from_slice(&[0xa2, 0x06, 0x02, 0x01, 0xff]);
        let attachments = [
            Attachment {
                id: "asset-image".into(),
                display_name: "picture.png".into(),
                media_type: "image/*".into(),
                byte_length: 12,
                sha256: [7; 32],
            },
            Attachment {
                id: "asset-file".into(),
                display_name: "notes.txt".into(),
                media_type: "application/octet-stream".into(),
                byte_length: 23,
                sha256: [8; 32],
            },
        ];
        let source = CardRecord::new_with_attachments(
            &idea.id,
            "org.morrow.idea",
            1,
            &idea.title,
            source_body,
            &attachments,
        )
        .unwrap();
        let mut raw = source.encode();
        // Outer Card field 101 is likewise unknown and retained by migration.
        raw.extend_from_slice(&[0xaa, 0x06, 0x02, 0x02, 0xff]);
        let source = CardRecord::decode(&raw).unwrap();
        let summary = source.summary();
        let baseline = Baseline::capture(&summary.id, summary.revision, &source.body()).unwrap();
        let target_body = tasks_v2::migrate(
            &baseline,
            &summary.id,
            summary.revision,
            &summary.title,
            &source.body(),
        )
        .unwrap();
        let target = ContentMigration {
            operation_id: "migration-example".into(),
            source_card: source.encode(),
            target_format_version: 2,
            body: target_body,
            preview_text: summary.preview_text,
        }
        .propose(&source)
        .unwrap();
        (source, target)
    }

    #[test]
    fn legacy_and_migrated_tasks_are_distinct_read_views() {
        let (source, target) = sample();
        match decode(&source).unwrap() {
            VersionedRecord::Legacy(record) => {
                assert_eq!(record.idea.todos, ["same", "same", "open"]);
                assert_eq!(record.revision, 1);
            }
            VersionedRecord::Tasks(_) => panic!("V1 became V2"),
        }
        let VersionedRecord::Tasks(record) = decode(&target).unwrap() else {
            panic!("V2 became V1");
        };
        assert_eq!(record.id, "versioned-example");
        assert_eq!(record.title, "Task example");
        assert_eq!(record.revision, 2);
        assert_eq!(record.properties.tasks.len(), 3);
        assert_ne!(record.properties.tasks[0].id, record.properties.tasks[1].id);
        assert_eq!(
            record
                .properties
                .tasks
                .iter()
                .map(|task| task.completion)
                .collect::<Vec<_>>(),
            [
                Completion::LegacyAmbiguous as i32,
                Completion::LegacyAmbiguous as i32,
                Completion::Incomplete as i32,
            ]
        );
        assert_eq!(record.projection.stage, "计划中");
        assert_eq!(
            (
                record.projection.complete,
                record.projection.incomplete,
                record.projection.ambiguous
            ),
            (0, 1, 2)
        );
        assert_eq!(
            record.properties.origin.unwrap().original_properties,
            source.body()
        );
        assert!(target.encode().ends_with(&[0xaa, 0x06, 0x02, 0x02, 0xff]));
    }

    #[test]
    fn future_format_and_other_type_are_explicitly_unsupported() {
        let (source, target) = sample();
        let future = ContentMigration {
            operation_id: "future-example".into(),
            source_card: source.encode(),
            target_format_version: 3,
            body: target.body(),
            preview_text: String::new(),
        }
        .propose(&source)
        .unwrap();
        let error = decode(&future).err().unwrap();
        assert_eq!(
            error.downcast_ref::<Unsupported>(),
            Some(&Unsupported {
                type_id: "org.morrow.idea".into(),
                format_version: 3,
            })
        );
        let other = CardRecord::new("other", "org.morrow.other", 1, "Other", vec![]).unwrap();
        assert!(decode(&other).err().unwrap().is::<Unsupported>());
    }

    #[test]
    fn malformed_v2_and_attachment_mismatch_are_rejected() {
        let malformed = CardRecord::new("bad", "org.morrow.idea", 2, "Bad", vec![0xff]).unwrap();
        assert!(decode(&malformed).is_err());
        let (_, target) = sample();
        let correct = target.attachments();
        let mut variants = Vec::new();
        let mut wrong_id = correct.clone();
        wrong_id[0].id = "different".into();
        variants.push(wrong_id);
        let mut wrong_name = correct.clone();
        wrong_name[0].display_name = "changed.png".into();
        variants.push(wrong_name);
        let mut wrong_bytes = correct.clone();
        wrong_bytes[0].byte_length += 1;
        variants.push(wrong_bytes);
        let mut wrong_order = correct.clone();
        wrong_order.swap(0, 1);
        variants.push(wrong_order);
        variants.push(correct[..1].to_vec());
        for attachments in variants {
            let mismatched = CardRecord::new_with_attachments(
                "versioned-example",
                "org.morrow.idea",
                2,
                "Task example",
                target.body(),
                &attachments,
            )
            .unwrap();
            let error = decode(&mismatched).err().unwrap();
            assert_eq!(error.to_string(), "V2 Card attachment metadata mismatch");
        }
    }
}
