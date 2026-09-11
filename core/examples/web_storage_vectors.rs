use morrow_core::{
    content::{CardRecord, DESCRIPTOR},
    envelope,
};
use prost::Message;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let pool = prost_reflect::DescriptorPool::decode(DESCRIPTOR)?;
    let desc = pool.get_message_by_name("morrow.content.v1.Card").unwrap();
    match args.as_slice() {
        [command, path] if command == "generate" => {
            let mut raw =
                CardRecord::new("high", "unknown.type", 1, "original", vec![0, 255, 7])?.encode();
            raw.extend_from_slice(&[0xa0, 0x06, 0x7b]);
            let mut message = prost_reflect::DynamicMessage::decode(desc, raw.as_slice())?;
            message.set_field_by_name("revision", prost_reflect::Value::U64(u64::MAX - 1));
            std::fs::write(
                path,
                envelope::encode(&CardRecord::decode(&message.encode_to_vec())?)?,
            )?;
        }
        [command, path] if command == "verify" => {
            let bytes = std::fs::read(path)?;
            let card = envelope::decode(&bytes)?;
            if card.summary().revision != u64::MAX
                || card.summary().title != "网页提交 🧭"
                || card.body() != [0, 255, 7]
            {
                return Err("browser export differs".into());
            }
            let message = prost_reflect::DynamicMessage::decode(desc, card.encode().as_slice())?;
            let mut unknown = Vec::new();
            for field in message.unknown_fields() {
                field.encode(&mut unknown);
            }
            if unknown != [0xa0, 0x06, 0x7b] {
                return Err("unknown field lost".into());
            }
            println!(
                "PASS: native -> OPFS mutation -> native, full UInt64 and unknown-field preservation."
            );
        }
        _ => return Err("usage: web_storage_vectors generate|verify <file>".into()),
    }
    Ok(())
}
