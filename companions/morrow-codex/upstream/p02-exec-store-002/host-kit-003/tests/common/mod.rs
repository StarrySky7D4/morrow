#![allow(dead_code)]
use morrow_agent_host_contract::{agent_host_capnp as wire, fake::*, *};
pub fn fixture() -> FakeHost {
    FakeHost::new(FixtureIdentity::default(), b"first-second".to_vec()).unwrap()
}
pub fn request(request_id: u64, f: impl FnOnce(wire::frame::Builder<'_>)) -> Vec<u8> {
    let mut msg = frame(request_id, "fixture-session", 1);
    f(msg.get_root::<wire::frame::Builder>().unwrap());
    encode(&msg).unwrap()
}
pub fn resource(mut r: wire::resource_ref::Builder<'_>, name: &str) {
    r.set_namespace("fixture");
    r.set_id(name);
    r.set_revision(9007199254740993);
    r.set_byte_length(0);
    r.set_sha256(&digest(b"fixture-reference"));
}
pub fn open(request_id: u64, body: &[u8]) -> Vec<u8> {
    request(request_id, |f| {
        let mut s = f.init_stream();
        s.set_attempt_id("attempt-1");
        let mut o = s.init_open();
        o.set_request_bytes(body.len() as u64);
        o.set_request_digest(&digest(body));
        resource(o.reborrow().init_destination(), "destination");
        let mut d = o.init_deadline();
        d.set_domain(ClockDomain::MonotonicMillis);
        d.set_clock_id("fixture-clock");
        d.set_value(1000);
    })
}
pub fn chunk(request_id: u64, offset: u64, body: &[u8]) -> Vec<u8> {
    request(request_id, |f| {
        let mut s = f.init_stream();
        s.set_attempt_id("attempt-1");
        let mut c = s.init_write_chunk();
        c.set_offset(offset);
        c.set_bytes(body);
    })
}
pub fn stream(request_id: u64, action: impl FnOnce(wire::stream::Builder<'_>)) -> Vec<u8> {
    request(request_id, |f| {
        let mut s = f.init_stream();
        s.set_attempt_id("attempt-1");
        action(s);
    })
}
pub fn read(request_id: u64, offset: u64, max: u32) -> Vec<u8> {
    stream(request_id, |s| {
        let mut r = s.init_read();
        r.set_expected_offset(offset);
        r.set_max_bytes(max);
    })
}
pub fn append(request_id: u64, epoch: u64, sequence: u64, tail: u64, payload: &[u8]) -> Vec<u8> {
    request(request_id, |f| {
        let mut e = f.init_event();
        e.set_writer_epoch(epoch);
        let mut b = e.init_append_batch();
        b.set_producer_sequence(sequence);
        b.set_expected_tail(tail);
        let mut events = b.init_events(1);
        let mut v = events.reborrow().get(0);
        v.set_turn_id("turn-1");
        v.set_item_id("item-1");
        v.set_part_id("part-1");
        v.set_attempt_id("attempt-1");
        v.set_semantic_kind("opaque.delta");
        v.set_payload(payload);
        v.set_source_digest(&digest(payload));
    })
}
pub fn propose(request_id: u64) -> Vec<u8> {
    request(request_id, |f| {
        let mut t = f.init_tool();
        t.set_operation_id("operation-1");
        let mut p = t.init_propose();
        p.set_root_operation("root-operation");
        p.set_attempt_id("attempt-1");
        p.set_input_digest(&digest(b"fixed-input"));
        p.set_tool_schema_digest(&digest(b"fixture-tool"));
        p.set_executor_artifact(&FixtureIdentity::default().artifact_digest);
        resource(p.init_source(), "source");
    })
}
pub fn claim(request_id: u64, permit: &[u8]) -> Vec<u8> {
    request(request_id, |f| {
        let mut t = f.init_tool();
        t.set_operation_id("operation-1");
        t.set_claim(permit);
    })
}
pub fn report(request_id: u64, state: ToolState) -> Vec<u8> {
    request(request_id, |f| {
        let mut t = f.init_tool();
        t.set_operation_id("operation-1");
        let mut r = t.init_report();
        r.set_state(state);
        r.set_exit_code(-1);
        r.set_output_closed(false);
        r.set_output_digest(&digest(b"partial-output"));
    })
}
pub fn reply(msg: &Message) -> wire::reply::Reader<'_> {
    match msg
        .get_root::<wire::frame::Reader>()
        .unwrap()
        .which()
        .unwrap()
    {
        wire::frame::Which::Reply(v) => v.unwrap(),
        _ => panic!("not a reply"),
    }
}
pub fn error(msg: &Message) -> ErrorCode {
    assert!(reply(msg).get_qualification_only());
    match reply(msg).which().unwrap() {
        wire::reply::Which::Error(v) => v.unwrap().get_code().unwrap(),
        _ => panic!("not an error"),
    }
}
pub fn stream_reply(msg: &Message) -> wire::stream_receipt::Reader<'_> {
    match reply(msg).which().unwrap() {
        wire::reply::Which::Stream(v) => v.unwrap(),
        _ => panic!("not stream"),
    }
}
pub fn event_reply(msg: &Message) -> wire::event_receipt::Reader<'_> {
    match reply(msg).which().unwrap() {
        wire::reply::Which::Event(v) => v.unwrap(),
        _ => panic!("not event"),
    }
}
pub fn tool_reply(msg: &Message) -> wire::tool_receipt::Reader<'_> {
    match reply(msg).which().unwrap() {
        wire::reply::Which::Tool(v) => v.unwrap(),
        _ => panic!("not tool"),
    }
}
pub fn exchange(host: &mut FakeHost, bytes: &[u8]) -> Message {
    exchange_checked(host, bytes).unwrap()
}
pub fn attach(host: &mut FakeHost) {
    exchange(host, &hello(&FixtureIdentity::default(), 1));
}
