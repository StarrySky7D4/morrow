use morrow_core::{
    content::CardSummary,
    response::{Failure, Outcome, Response},
    runtime::{Command, ReadAttachment, RenameRequest},
    task::Invocation,
    transaction::Lookup,
};
use morrow_plugin_sdk::task::Invocation as Guest;
#[test]
fn independent_host_and_guest_task_codecs_agree_for_all_content_commands() {
    let pairs = vec![
        (
            Command::Rename(RenameRequest {
                operation_id: "操作-1".into(),
                card_id: "卡片-A".into(),
                expected_revision: u64::MAX,
                title: "新标题🌈".into(),
            }),
            Outcome::Rejected(Failure::RevisionConflict),
        ),
        (
            Command::ReadSummary {
                request_id: "read".into(),
                card_id: "card".into(),
            },
            Outcome::Summary(CardSummary {
                id: "card".into(),
                type_id: "note".into(),
                format_version: 1,
                revision: u64::MAX,
                title: "彩色".into(),
                preview_text: "摘要".into(),
            }),
        ),
        (
            Command::QueryOperation {
                request_id: "query".into(),
                card_id: "card".into(),
                operation_id: "missing".into(),
            },
            Outcome::OperationResult {
                card_id: "card".into(),
                operation_id: "missing".into(),
                result: Lookup::Absent,
            },
        ),
        (
            Command::ReadAttachment(ReadAttachment {
                request_id: "part".into(),
                card_id: "card".into(),
                attachment_id: "blob".into(),
                expected_revision: u64::MAX,
                offset: 5,
                length: 3,
            }),
            Outcome::AttachmentChunk(morrow_core::attachment::AttachmentChunk {
                card_id: "card".into(),
                attachment_id: "blob".into(),
                revision: u64::MAX,
                offset: 5,
                total_length: 8,
                content_sha256: [7; 32],
                bytes: vec![1, 2, 3],
            }),
        ),
    ];
    for (command, outcome) in pairs {
        let input = Invocation::new("task-不同", &command).unwrap();
        let guest = Guest::decode(input.bytes()).unwrap();
        assert_eq!(guest.task_id(), input.task_id());
        assert_eq!(guest.command_bytes(), input.command_bytes());
        assert_eq!(
            guest.request().unwrap().encode().unwrap(),
            command.encode().unwrap()
        );
        let response = Response {
            request_id: command.request_id().into(),
            outcome,
        };
        let bytes = response.encode().unwrap();
        let completion = guest.completion(&bytes).unwrap();
        assert_eq!(completion, input.completion(&bytes).unwrap());
        assert_eq!(
            input.verify_completion(&completion, &bytes).unwrap(),
            response
        );
    }
}
#[test]
fn task_contract_and_provenance_reject_changed_ids_payloads_and_forged_success() {
    let cmd = Command::ReadSummary {
        request_id: "read".into(),
        card_id: "card".into(),
    };
    let a = Invocation::new("a", &cmd).unwrap();
    let b = Invocation::new("b", &cmd).unwrap();
    let actual = Response {
        request_id: "read".into(),
        outcome: Outcome::Rejected(Failure::Denied),
    }
    .encode()
    .unwrap();
    assert!(
        a.verify_completion(&b.completion(&actual).unwrap(), &actual)
            .is_err()
    );
    let forged = Response {
        request_id: "read".into(),
        outcome: Outcome::Summary(CardSummary {
            id: "card".into(),
            type_id: "note".into(),
            format_version: 1,
            revision: 1,
            title: "forged".into(),
            preview_text: "".into(),
        }),
    }
    .encode()
    .unwrap();
    assert!(
        a.verify_completion(&a.completion(&forged).unwrap(), &actual)
            .is_err()
    );
    let wrong = Response {
        request_id: "other".into(),
        outcome: Outcome::Rejected(Failure::Denied),
    }
    .encode()
    .unwrap();
    assert!(
        Guest::decode(a.bytes())
            .unwrap()
            .completion(&wrong)
            .is_err()
    );
    for version in [0, 4] {
        let mut m = capnp::message::Builder::new_default();
        let mut r = m.init_root::<morrow_core::task_capnp::invocation::Builder>();
        r.set_version(version);
        r.set_schema_digest(&morrow_core::task::schema_digest());
        r.set_task_id("a");
        r.set_command(&cmd.encode().unwrap());
        let bytes = capnp::serialize::write_message_to_words(&m);
        assert!(Invocation::decode(&bytes).is_err());
        assert!(Guest::decode(&bytes).is_err());
    }
    for bytes in [
        vec![],
        a.bytes()[..a.bytes().len() - 1].to_vec(),
        [a.bytes(), &[0; 8]].concat(),
        vec![0; 131073],
    ] {
        assert!(Invocation::decode(&bytes).is_err());
        assert!(Guest::decode(&bytes).is_err());
    }
}
