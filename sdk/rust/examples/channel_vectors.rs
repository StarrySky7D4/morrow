//! Emit deterministic native-codec goldens; all sources and bytes are synthetic.
use morrow_plugin_sdk::channel::{
    self, Action, Budget, Directory, Endpoint, Frame, Kind, Request, Response, Status,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::path::PathBuf::from(std::env::args().nth(1).ok_or("output directory")?);
    std::fs::create_dir_all(&out)?;
    let service = morrow_plugin_sdk::service::Request::encode(
        1,
        &morrow_plugin_sdk::service::Invocation {
            service: "guard-service".into(),
            handler: "guard-handler".into(),
            principal: "local".into(),
            method: "GET".into(),
            target: "/".into(),
            headers: vec![],
            body: vec![],
        },
    )
    .unwrap();
    std::fs::write(out.join("service.request.capnp"), service.bytes())?;
    let mut request = Request {
        call_id: [1; 32],
        reference: [2; 32],
        source_epoch: [3; 32],
        action: Action::Receive {
            last_acked: 0,
            credit_bytes: 65536,
        },
    };
    let frame = Frame {
        sequence: 1,
        source_epoch: [3; 32],
        bytes: (0..65536usize).map(|i| (i % 251) as u8).collect(),
        cursor: vec![0, 255, 1, 2],
    };
    let response = Response {
        call_id: request.call_id,
        request_sha256: request.digest().unwrap(),
        reference: request.reference,
        source_epoch: request.source_epoch,
        status: Status::Frame,
        frame: Some(frame.clone()),
        last_acked: 0,
        accepted_sequence: 0,
        resource_reclaimed: false,
    };
    std::fs::write(out.join("receive.request.capnp"), request.encode().unwrap())?;
    std::fs::write(
        out.join("receive.response.capnp"),
        response.encode().unwrap(),
    )?;
    std::fs::write(out.join("frame.capnp"), frame.encode().unwrap())?;
    std::fs::write(out.join("frame.sha256"), frame.digest().unwrap())?;
    request.action = Action::Ack {
        sequence: 1,
        frame_sha256: frame.digest().unwrap(),
        cursor: frame.cursor.clone(),
    };
    std::fs::write(out.join("ack.request.capnp"), request.encode().unwrap())?;
    request.action = Action::Send {
        sequence: 1,
        bytes: frame.bytes,
    };
    std::fs::write(out.join("send.request.capnp"), request.encode().unwrap())?;
    request.action = Action::Close;
    std::fs::write(out.join("close.request.capnp"), request.encode().unwrap())?;
    request.action = Action::Query;
    std::fs::write(out.join("query.request.capnp"), request.encode().unwrap())?;
    let directory = Directory {
        scope_sha256: [4; 32],
        channels: vec![Endpoint {
            reference: [2; 32],
            source_epoch: [3; 32],
            kind: Kind::ByteStream,
            budget: Budget::default(),
        }],
    };
    std::fs::write(out.join("directory.capnp"), directory.encode().unwrap())?;
    std::fs::write(out.join("schema.sha256"), channel::schema_digest())?;
    Ok(())
}
