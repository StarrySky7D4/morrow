//! Typed SSE scenarios reuse the audited ordinary bounded HTTP peer unchanged.
#[path = "managed_sse_support.rs"]
mod original;
pub use original::{HEAD, Observations, Server, WAIT, until};
pub const BODY: &[u8] = b"{\"synthetic\":true}";
pub fn complete_stream_parts() -> Vec<Vec<u8>> {
    let first = format!("id: {}\r\nevent: delta\r\ndata: ", "i".repeat(300));
    vec![[first.as_bytes(),b"\xe4"].concat(),b"\xbd\xa0\r".to_vec(),
   "\ndata: second\r\n\r\nid:\nretry: 0\ndata:\n\nid: ignored\0suffix\nretry: 18446744073709551615\nevent: 更新\ndata: 最终 🙂\n\ndata: [DONE]\n\ndata: after-DONE\n\n".as_bytes().to_vec()]
}
pub fn first_event_only() -> Vec<Vec<u8>> {
    vec![
        format!(
            "id: {}\nevent: delta\ndata: 你\ndata: second\n\n",
            "i".repeat(300)
        )
        .into_bytes(),
    ]
}
