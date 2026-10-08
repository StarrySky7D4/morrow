use crate::{process_control_capnp as wire, *};
use capnp::{
    message::{Builder, HeapAllocator, ReaderOptions},
    serialize,
};
fn bad(_: impl std::fmt::Debug) -> Error {
    Error::Invalid
}
fn text(value: capnp::Result<capnp::text::Reader<'_>>) -> Result<String> {
    Ok(value.map_err(bad)?.to_str().map_err(bad)?.to_owned())
}
fn bytes32(value: capnp::Result<&[u8]>) -> Result<[u8; 32]> {
    value.map_err(bad)?.try_into().map_err(bad)
}
fn read(bytes: &[u8]) -> Result<capnp::message::Reader<serialize::OwnedSegments>> {
    if bytes.is_empty() || bytes.len() > MAX_FRAME_BYTES {
        return Err(Error::Limit);
    }
    let mut input = bytes;
    let message = serialize::read_message(
        &mut input,
        ReaderOptions {
            traversal_limit_in_words: Some(MAX_FRAME_BYTES / 2),
            nesting_limit: 16,
        },
    )
    .map_err(bad)?;
    if !input.is_empty() {
        return Err(Error::Invalid);
    }
    Ok(message)
}
fn finish(message: &Builder<HeapAllocator>) -> Result<Vec<u8>> {
    let bytes = serialize::write_message_to_words(message);
    if bytes.len() > MAX_FRAME_BYTES {
        Err(Error::Limit)
    } else {
        Ok(bytes)
    }
}
fn identity(version: u16, digest: capnp::Result<&[u8]>) -> Result<()> {
    if version != VERSION || digest.map_err(bad)? != SCHEMA_DIGEST {
        Err(Error::Contract)
    } else {
        Ok(())
    }
}
fn set_query(mut out: wire::read_query::Builder<'_>, query: ReadQuery) {
    out.set_after_seq(query.after_seq);
    out.set_max_bytes(query.max_bytes);
    out.set_max_events(query.max_events);
    out.set_wait_ms(query.wait_ms)
}
fn query(value: capnp::Result<wire::read_query::Reader<'_>>) -> Result<ReadQuery> {
    let r = value.map_err(bad)?;
    Ok(ReadQuery {
        after_seq: r.get_after_seq(),
        max_bytes: r.get_max_bytes(),
        max_events: r.get_max_events(),
        wait_ms: r.get_wait_ms(),
    })
}
impl Request {
    pub fn encode(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let mut message = Builder::new_default();
        {
            let mut out = message.init_root::<wire::request::Builder<'_>>();
            out.set_version(VERSION);
            out.set_schema_sha256(&SCHEMA_DIGEST);
            out.set_request_id(self.request_id.as_str());
            out.set_handle(&self.handle);
            out.set_generation(self.generation);
            match &self.action {
                Action::Discover => out.set_discover(()),
                Action::Read(q) => set_query(out.init_read(), *q),
                Action::Events(q) => set_query(out.init_events(), *q),
                Action::Write(chunk) => out.set_write(chunk),
                Action::CloseInput => out.set_close_input(()),
                Action::Interrupt => out.set_interrupt(()),
                Action::Terminate => out.set_terminate(()),
                Action::Resize { rows, cols } => {
                    let mut size = out.init_resize();
                    size.set_rows(*rows);
                    size.set_cols(*cols)
                }
            }
        }
        finish(&message)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let message = read(bytes)?;
        let root = message
            .get_root::<wire::request::Reader<'_>>()
            .map_err(bad)?;
        identity(root.get_version(), root.get_schema_sha256())?;
        if root.total_size().map_err(bad)?.cap_count != 0 {
            return Err(Error::Invalid);
        }
        let request_id = text(root.get_request_id())?;
        let handle = bytes32(root.get_handle())?;
        let generation = root.get_generation();
        let action = match root.which().map_err(bad)? {
            wire::request::Discover(()) => Action::Discover,
            wire::request::Read(v) => Action::Read(query(v)?),
            wire::request::Events(v) => Action::Events(query(v)?),
            wire::request::Write(v) => Action::Write(v.map_err(bad)?.to_vec()),
            wire::request::CloseInput(()) => Action::CloseInput,
            wire::request::Interrupt(()) => Action::Interrupt,
            wire::request::Terminate(()) => Action::Terminate,
            wire::request::Resize(v) => {
                let v = v.map_err(bad)?;
                Action::Resize {
                    rows: v.get_rows(),
                    cols: v.get_cols(),
                }
            }
        };
        let request = Self::new(request_id, handle, generation, action)?;
        if request.encode()? != bytes {
            return Err(Error::Invalid);
        }
        Ok(request)
    }
}
fn set_caps(mut r: wire::capabilities::Builder<'_>, c: Capabilities) {
    r.set_read(c.read);
    r.set_events(c.events);
    r.set_write(c.write);
    r.set_close_input(c.close_input);
    r.set_interrupt(c.interrupt);
    r.set_terminate(c.terminate);
    r.set_resize_pty(c.resize_pty)
}
fn caps(r: wire::capabilities::Reader<'_>) -> Capabilities {
    Capabilities {
        read: r.get_read(),
        events: r.get_events(),
        write: r.get_write(),
        close_input: r.get_close_input(),
        interrupt: r.get_interrupt(),
        terminate: r.get_terminate(),
        resize_pty: r.get_resize_pty(),
    }
}
fn set_page(mut out: wire::output_page::Builder<'_>, p: &OutputPage) {
    out.set_next_seq(p.next_seq);
    out.set_floor_seq(p.floor_seq);
    out.set_gap(p.gap);
    out.set_exited(p.exited);
    out.set_has_exit_code(p.exit_code.is_some());
    out.set_exit_code(p.exit_code.unwrap_or_default());
    out.set_closed(p.closed);
    out.set_has_failure(p.failure.is_some());
    out.set_failure(p.failure.as_deref().unwrap_or_default());
    let mut events = out.init_events(p.events.len() as u32);
    for (i, e) in p.events.iter().enumerate() {
        let mut item = events.reborrow().get(i as u32);
        item.set_seq(e.seq);
        match &e.kind {
            EventKind::Output { stream, chunk } => {
                let mut r = item.init_output();
                r.set_stream(match stream {
                    OutputStream::Stdout => wire::OutputStream::Stdout,
                    OutputStream::Stderr => wire::OutputStream::Stderr,
                    OutputStream::Pty => wire::OutputStream::Pty,
                });
                r.set_chunk(chunk)
            }
            EventKind::Exited {
                exit_code,
                sandbox_denied,
            } => {
                let mut r = item.init_exited();
                r.set_exit_code(*exit_code);
                r.set_has_sandbox_denied(sandbox_denied.is_some());
                r.set_sandbox_denied(sandbox_denied.unwrap_or_default())
            }
            EventKind::Closed => item.set_closed(()),
        }
    }
}
fn page(p: wire::output_page::Reader<'_>) -> Result<OutputPage> {
    let list = p.get_events().map_err(bad)?;
    if list.len() as usize > MAX_EVENTS {
        return Err(Error::Limit);
    }
    let mut events = Vec::new();
    for e in list.iter() {
        let kind = match e.which().map_err(bad)? {
            wire::process_event::Output(v) => {
                let v = v.map_err(bad)?;
                let stream = match v.get_stream().map_err(bad)? {
                    wire::OutputStream::Stdout => OutputStream::Stdout,
                    wire::OutputStream::Stderr => OutputStream::Stderr,
                    wire::OutputStream::Pty => OutputStream::Pty,
                };
                EventKind::Output {
                    stream,
                    chunk: v.get_chunk().map_err(bad)?.to_vec(),
                }
            }
            wire::process_event::Exited(v) => {
                let v = v.map_err(bad)?;
                EventKind::Exited {
                    exit_code: v.get_exit_code(),
                    sandbox_denied: v.get_has_sandbox_denied().then(|| v.get_sandbox_denied()),
                }
            }
            wire::process_event::Closed(()) => EventKind::Closed,
        };
        events.push(ProcessEvent {
            seq: e.get_seq(),
            kind,
        })
    }
    Ok(OutputPage {
        events,
        next_seq: p.get_next_seq(),
        floor_seq: p.get_floor_seq(),
        gap: p.get_gap(),
        exited: p.get_exited(),
        exit_code: p.get_has_exit_code().then(|| p.get_exit_code()),
        closed: p.get_closed(),
        failure: if p.get_has_failure() {
            Some(text(p.get_failure())?)
        } else {
            None
        },
    })
}
fn failure(e: Error) -> wire::Failure {
    match e {
        Error::Invalid => wire::Failure::Invalid,
        Error::Contract => wire::Failure::Contract,
        Error::Limit => wire::Failure::Limit,
        Error::Correlation => wire::Failure::Correlation,
        Error::Denied => wire::Failure::Denied,
        Error::Conflict => wire::Failure::Conflict,
        Error::NotFound => wire::Failure::NotFound,
        Error::Unknown => wire::Failure::Unknown,
        Error::Unsupported => wire::Failure::Unsupported,
        Error::Closed => wire::Failure::Closed,
    }
}
fn error(e: wire::Failure) -> Error {
    match e {
        wire::Failure::Invalid => Error::Invalid,
        wire::Failure::Contract => Error::Contract,
        wire::Failure::Limit => Error::Limit,
        wire::Failure::Correlation => Error::Correlation,
        wire::Failure::Denied => Error::Denied,
        wire::Failure::Conflict => Error::Conflict,
        wire::Failure::NotFound => Error::NotFound,
        wire::Failure::Unknown => Error::Unknown,
        wire::Failure::Unsupported => Error::Unsupported,
        wire::Failure::Closed => Error::Closed,
    }
}
impl Reply {
    pub fn encode(&self) -> Result<Vec<u8>> {
        valid_id(&self.request_id)?;
        if self.generation == 0 {
            return Err(Error::Invalid);
        }
        if let ReplyBody::Page(p) = &self.body {
            p.validate()?
        }
        let mut message = Builder::new_default();
        {
            let mut out = message.init_root::<wire::reply::Builder<'_>>();
            out.set_version(VERSION);
            out.set_schema_sha256(&SCHEMA_DIGEST);
            out.set_request_id(self.request_id.as_str());
            out.set_request_sha256(&self.request_sha256);
            out.set_generation(self.generation);
            match &self.body {
                ReplyBody::Capabilities(c) => set_caps(out.init_capabilities(), *c),
                ReplyBody::Page(p) => set_page(out.init_page(), p),
                ReplyBody::Accepted => out.set_accepted(()),
                ReplyBody::Rejected(e) => out.set_rejected(failure(*e)),
            }
        }
        finish(&message)
    }
    pub fn decode_for(request: &Request, bytes: &[u8]) -> Result<Self> {
        let message = read(bytes)?;
        let root = message.get_root::<wire::reply::Reader<'_>>().map_err(bad)?;
        identity(root.get_version(), root.get_schema_sha256())?;
        if root.total_size().map_err(bad)?.cap_count != 0 {
            return Err(Error::Invalid);
        }
        let request_id = text(root.get_request_id())?;
        let request_sha256 = bytes32(root.get_request_sha256())?;
        let generation = root.get_generation();
        if request_id != request.request_id
            || request_sha256 != request.digest()?
            || generation != request.generation
        {
            return Err(Error::Correlation);
        }
        let body = match root.which().map_err(bad)? {
            wire::reply::Capabilities(c) => ReplyBody::Capabilities(caps(c.map_err(bad)?)),
            wire::reply::Page(p) => ReplyBody::Page(page(p.map_err(bad)?)?),
            wire::reply::Accepted(()) => ReplyBody::Accepted,
            wire::reply::Rejected(e) => ReplyBody::Rejected(error(e.map_err(bad)?)),
        };
        match (&request.action, &body) {
            (_, ReplyBody::Rejected(_)) | (Action::Discover, ReplyBody::Capabilities(_)) => (),
            (Action::Read(q) | Action::Events(q), ReplyBody::Page(p)) => p.validate_for(*q)?,
            (action, ReplyBody::Accepted) if action.is_mutation() => (),
            _ => return Err(Error::Correlation),
        }
        let reply = Self {
            request_id,
            request_sha256,
            generation,
            body,
        };
        if reply.encode()? != bytes {
            return Err(Error::Invalid);
        }
        Ok(reply)
    }
}
