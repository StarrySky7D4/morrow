#![deny(unsafe_code)]
use morrow_plugin_sdk::{ui, ui_capnp};

#[test]
fn original_ui_event_accepts_all_transport_byte_alignments() {
    let original = include_bytes!("../../tests/ui_fixtures/event.capnp");
    for offset in 0..8 {
        let mut words = capnp::Word::allocate_zeroed_vec((original.len() + 8).div_ceil(8));
        let storage = capnp::Word::words_to_bytes_mut(&mut words);
        storage[offset..offset + original.len()].copy_from_slice(original);
        let bytes = &storage[offset..offset + original.len()];
        let mut input = bytes;
        let diagnostic = capnp::serialize::read_message_from_flat_slice(
            &mut input,
            capnp::message::ReaderOptions::new(),
        )
        .and_then(|message| {
            message
                .get_root::<ui_capnp::event::Reader>()
                .map(|event| event.get_generation())
        });
        eprintln!(
            "offset={offset} address_mod8={} direct_capnp={diagnostic:?}",
            bytes.as_ptr() as usize % 8
        );
        let event =
            ui::Event::decode(bytes).expect("valid event has no transport alignment requirement");
        assert_eq!(event.generation, u64::MAX);
        assert_eq!(event.text, "从 Dart 编辑🌈");
    }
}

fn each_alignment(bytes: &[u8], mut check: impl FnMut(&[u8])) {
    for offset in 0..8 {
        let mut words = capnp::Word::allocate_zeroed_vec((bytes.len() + 8).div_ceil(8));
        let storage = capnp::Word::words_to_bytes_mut(&mut words);
        storage[offset..offset + bytes.len()].copy_from_slice(bytes);
        let input = &storage[offset..offset + bytes.len()];
        assert_eq!(input.as_ptr() as usize % 8, offset);
        check(input);
    }
}
fn rename() -> morrow_plugin_sdk::protocol::Request {
    morrow_plugin_sdk::protocol::Request {
        request_id: "vector-op".into(),
        card_id: "legacy-123".into(),
        action: morrow_plugin_sdk::protocol::Action::Rename {
            revision: u64::MAX - 1,
            title: "消息 🪷".into(),
        },
    }
}
fn invocation() -> Vec<u8> {
    use sha2::{Digest, Sha256};
    let mut message = capnp::message::Builder::new_default();
    let mut root = message.init_root::<morrow_plugin_sdk::task_capnp::invocation::Builder>();
    root.set_version(
        include_str!("../contracts/task-version.txt")
            .trim()
            .parse()
            .unwrap(),
    );
    root.set_schema_digest(&Sha256::digest(
        include_str!("../contracts/task.capnp")
            .replace("\r\n", "\n")
            .as_bytes(),
    ));
    root.set_task_id("aligned-task");
    root.set_kind(morrow_plugin_sdk::task_capnp::Kind::ContentCommand);
    root.set_command(include_bytes!("../../tests/fixtures/rename-request.capnp"));
    capnp::serialize::write_message_to_words(&message)
}
#[test]
fn original_ui_document_accepts_all_transport_byte_alignments() {
    each_alignment(
        include_bytes!("../../tests/ui_fixtures/document.capnp"),
        |bytes| {
            let doc = ui::Document::decode(bytes).unwrap();
            assert_eq!(doc.nodes().len(), 5);
            assert_eq!(doc.nodes()[2].text, "灵感🌈");
            assert_eq!(doc.encode().unwrap(), bytes);
        },
    );
}
#[test]
fn original_request_and_reply_keep_exact_u64_on_every_alignment() {
    use morrow_plugin_sdk::protocol::{Action, Reply, Request};
    each_alignment(
        include_bytes!("../../tests/fixtures/rename-request.capnp"),
        |bytes| {
            let request = Request::decode(bytes).unwrap();
            assert_eq!(request.request_id, "vector-op");
            assert!(
                matches!(&request.action,Action::Rename {revision,title} if *revision==u64::MAX-1 && title=="消息 🪷")
            );
            assert_eq!(request.encode().unwrap(), bytes);
        },
    );
    each_alignment(
        include_bytes!("../../tests/fixtures/renamed-reply.capnp"),
        |bytes| {
            let Reply::Renamed(receipt) = rename().decode_reply(bytes).unwrap() else {
                panic!("receipt kind")
            };
            assert_eq!(receipt.revision, u64::MAX);
            assert_eq!(receipt.event_id, "event-1");
            assert_eq!(receipt.sha256, [42; 32]);
        },
    );
}
#[test]
fn task_alignment_preserves_original_request_and_completion_correlation() {
    use sha2::{Digest, Sha256};
    let original = invocation();
    each_alignment(&original, |bytes| {
        let task = morrow_plugin_sdk::task::Invocation::decode(bytes).unwrap();
        assert_eq!(task.task_id(), "aligned-task");
        assert_eq!(
            task.command_bytes(),
            include_bytes!("../../tests/fixtures/rename-request.capnp")
        );
        let response = include_bytes!("../../tests/fixtures/renamed-reply.capnp");
        let completion = task.completion(response).unwrap();
        let message =
            capnp::serialize::read_message(&mut completion.as_slice(), Default::default()).unwrap();
        let root = message
            .get_root::<morrow_plugin_sdk::task_capnp::completion::Reader>()
            .unwrap();
        assert_eq!(root.get_task_id().unwrap(), "aligned-task");
        assert_eq!(
            root.get_input_digest().unwrap(),
            Sha256::digest(&original).as_slice()
        );
        assert_eq!(root.get_response().unwrap(), response);
    });
}
#[test]
fn alignment_fix_keeps_truncation_tail_and_allocation_limits() {
    use morrow_plugin_sdk::{
        protocol::{CodecError, Request},
        task::Invocation,
    };
    type Decoder = fn(&[u8]) -> Result<(), CodecError>;
    let task = invocation();
    let cases: [(&[u8], usize, Decoder); 5] = [
        (
            include_bytes!("../../tests/ui_fixtures/event.capnp"),
            ui::MAX_BYTES,
            |b| ui::Event::decode(b).map(|_| ()),
        ),
        (
            include_bytes!("../../tests/ui_fixtures/document.capnp"),
            ui::MAX_BYTES,
            |b| ui::Document::decode(b).map(|_| ()),
        ),
        (
            include_bytes!("../../tests/fixtures/rename-request.capnp"),
            morrow_plugin_sdk::MAX_MESSAGE_BYTES,
            |b| Request::decode(b).map(|_| ()),
        ),
        (
            include_bytes!("../../tests/fixtures/renamed-reply.capnp"),
            morrow_plugin_sdk::MAX_MESSAGE_BYTES,
            |b| rename().decode_reply(b).map(|_| ()),
        ),
        (&task, morrow_plugin_sdk::task::MAX_TASK_BYTES, |b| {
            Invocation::decode(b).map(|_| ())
        }),
    ];
    for (valid, limit, decode) in cases {
        let mut tail = valid.to_vec();
        tail.push(0);
        let mut concatenated = valid.to_vec();
        concatenated.extend_from_slice(valid);
        let forged_single = [0, 0, 0, 0, 255, 255, 255, 255];
        // 511 bounded segment entries whose aggregate body exceeds every reader limit.
        let mut forged_many = 510u32.to_le_bytes().to_vec();
        for _ in 0..511 {
            forged_many.extend_from_slice(&4096u32.to_le_bytes());
        }
        for invalid in [
            &valid[..valid.len() - 1],
            &tail,
            &concatenated,
            &forged_single,
            &forged_many,
        ] {
            each_alignment(invalid, |bytes| assert!(decode(bytes).is_err()));
        }
        assert_eq!(decode(&vec![0; limit + 1]), Err(CodecError::Limit));
    }
}
