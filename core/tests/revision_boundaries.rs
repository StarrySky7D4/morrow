use morrow_core::{content::{CardRecord, DESCRIPTOR}, Error};
use prost::Message;
use prost_reflect::{DescriptorPool, DynamicMessage, Value};

// Standalone valid records exercise the real content transition, not a
// modified signed database or a bypass of historical store verification.
#[test]
fn adjacent_u64_revisions_and_maximum_have_exact_transition_semantics() {
    let descriptor = DescriptorPool::decode(DESCRIPTOR).unwrap()
        .get_message_by_name("morrow.content.v1.Card").unwrap();
    let seed = CardRecord::new("boundary", "note", 1, "before", vec![]).unwrap();
    for revision in [1, (1u64<<53)+1, (1u64<<53)+2, (1u64<<63)+1, u64::MAX-1, u64::MAX] {
        let mut message = DynamicMessage::decode(descriptor.clone(),seed.encode().as_slice()).unwrap();
        message.set_field_by_name("revision",Value::U64(revision));
        let original = message.encode_to_vec();
        let card = CardRecord::decode(&original).unwrap();
        assert_eq!(card.summary().revision,revision);
        assert!(matches!(card.with_title(revision-1,"stale"),Err(Error::RevisionConflict)));
        if revision==u64::MAX {
            assert!(card.with_title(revision,"overflow").is_err());
        } else {
            let next=card.with_title(revision,"after").unwrap();
            assert_eq!(next.summary().revision,revision+1);
            assert_eq!(next.summary().title,"after");
        }
        assert_eq!(card.encode(),original);
    }
}
