//! Typed, pinned Cap'n Proto transport for the derived Windows parent/Runner.
//! Parsing conveys data only. Existing owners still decide admission and closure.
use crate::ipc_framed::*;
use crate::runner_ipc_capnp as wire;
use anyhow::{Result, ensure};
use codex_protocol::models::{ManagedFileSystemPermissions, PermissionProfile};
use codex_protocol::permissions::{
    FileSystemAccessMode, FileSystemPath, FileSystemSandboxEntry,
    FileSystemSandboxEntryMissingPathBehavior, FileSystemSpecialPath, NetworkSandboxPolicy,
};
use codex_utils_absolute_path::AbsolutePathBuf;
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};

const MAGIC: u64 = 0x31435049524f4d57;
const WIRE_REVISION: u16 = 1;
pub(crate) const MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;
const MAX_TEXT_BYTES: usize = 128 * 1024;
const MAX_ITEMS: u32 = 4096;
fn invalid() -> anyhow::Error {
    anyhow::anyhow!("Windows Runner Cap'n Proto frame rejected")
}

/// Charge every field/list before copying it into the message. Each has a bound;
/// conservative overhead also bounds padding, pointers and segment tables.
struct Budget(usize);
impl Budget {
    fn add(&mut self, n: usize) -> Result<()> {
        self.0 = self.0.checked_add(n).ok_or_else(invalid)?;
        ensure!(self.0 <= MAX_FRAME_BYTES, "Runner frame budget exceeded");
        Ok(())
    }
    fn text(&mut self, s: &str) -> Result<()> {
        ensure!(s.len() <= MAX_TEXT_BYTES, "Runner text limit exceeded");
        self.add(s.len() + 16)
    }
    fn items(&mut self, n: usize) -> Result<u32> {
        ensure!(n <= MAX_ITEMS as usize, "Runner list limit exceeded");
        self.add(n * 64)?;
        Ok(n as u32)
    }
}
fn text(r: capnp::text::Reader<'_>) -> Result<String> {
    ensure!(r.len() <= MAX_TEXT_BYTES, "Runner text limit exceeded");
    Ok(r.to_str().map_err(|_| invalid())?.to_owned())
}
fn array32(r: &[u8]) -> Result<[u8; 32]> {
    r.try_into().map_err(|_| invalid())
}
fn path(p: &Path) -> Result<&str> {
    p.to_str().ok_or_else(invalid)
}
fn count(n: u32) -> Result<usize> {
    ensure!(n <= MAX_ITEMS, "Runner list limit exceeded");
    Ok(n as usize)
}
fn put_texts(
    mut b: capnp::text_list::Builder<'_>,
    values: &[String],
    budget: &mut Budget,
) -> Result<()> {
    for (i, s) in values.iter().enumerate() {
        budget.text(s)?;
        b.set(i as u32, s.as_str());
    }
    Ok(())
}
fn get_texts(r: capnp::text_list::Reader<'_>) -> Result<Vec<String>> {
    let mut values = Vec::with_capacity(count(r.len())?);
    for s in r {
        values.push(text(s?)?);
    }
    Ok(values)
}
fn put_optional_text(
    mut b: wire::maybe_text::Builder<'_>,
    value: &Option<String>,
    budget: &mut Budget,
) -> Result<()> {
    match value {
        None => b.set_none(()),
        Some(s) => {
            budget.text(s)?;
            b.set_some(s.as_str());
        }
    }
    Ok(())
}
fn get_optional_text(r: wire::maybe_text::Reader<'_>) -> Result<Option<String>> {
    Ok(match r.which()? {
        wire::maybe_text::None(()) => None,
        wire::maybe_text::Some(s) => Some(text(s?)?),
    })
}
fn put_optional_u64(mut b: wire::maybe_u64::Builder<'_>, value: Option<u64>) {
    match value {
        None => b.set_none(()),
        Some(n) => b.set_some(n),
    }
}
fn get_optional_u64(r: wire::maybe_u64::Reader<'_>) -> Result<Option<u64>> {
    Ok(match r.which()? {
        wire::maybe_u64::None(()) => None,
        wire::maybe_u64::Some(n) => Some(n),
    })
}
fn put_hello(mut b: wire::hello::Builder<'_>, h: &ControlHello) {
    b.set_nonce(&h.nonce);
    b.set_schema(&h.schema);
}
fn get_hello(r: wire::hello::Reader<'_>) -> Result<ControlHello> {
    Ok(ControlHello {
        nonce: array32(r.get_nonce()?)?,
        schema: array32(r.get_schema()?)?,
    })
}
fn put_controls(mut b: wire::controls::Builder<'_>, c: RunnerControlCapabilities) {
    b.set_close_stdin(c.close_stdin);
    b.set_resize(c.resize);
}
fn get_controls(r: wire::controls::Reader<'_>) -> RunnerControlCapabilities {
    RunnerControlCapabilities {
        close_stdin: r.get_close_stdin(),
        resize: r.get_resize(),
    }
}
fn put_control(mut b: wire::control_request::Builder<'_>, c: &ControlRequest) {
    b.set_id(c.id);
    b.set_nonce(&c.nonce);
    b.set_rows(c.rows);
    b.set_cols(c.cols);
    b.set_args_digest(&c.args_digest);
    b.set_operation(match c.op {
        ControlOperation::CloseStdin => wire::ControlOperation::CloseStdin,
        ControlOperation::Resize => wire::ControlOperation::Resize,
    });
}
fn get_control(r: wire::control_request::Reader<'_>) -> Result<ControlRequest> {
    Ok(ControlRequest {
        id: r.get_id(),
        nonce: array32(r.get_nonce()?)?,
        rows: r.get_rows(),
        cols: r.get_cols(),
        args_digest: array32(r.get_args_digest()?)?,
        op: match r.get_operation()? {
            wire::ControlOperation::CloseStdin => ControlOperation::CloseStdin,
            wire::ControlOperation::Resize => ControlOperation::Resize,
        },
    })
}
fn put_network(n: NetworkSandboxPolicy) -> wire::Network {
    match n {
        NetworkSandboxPolicy::Restricted => wire::Network::Restricted,
        NetworkSandboxPolicy::Enabled => wire::Network::Enabled,
    }
}
fn get_network(n: wire::Network) -> NetworkSandboxPolicy {
    match n {
        wire::Network::Restricted => NetworkSandboxPolicy::Restricted,
        wire::Network::Enabled => NetworkSandboxPolicy::Enabled,
    }
}
fn put_path(
    mut b: wire::fs_path::Builder<'_>,
    p: &FileSystemPath,
    budget: &mut Budget,
) -> Result<()> {
    match p {
        FileSystemPath::Path { path } => {
            let s = path.to_string();
            budget.text(&s)?;
            b.set_uri(s.as_str());
        }
        FileSystemPath::GlobPattern { pattern } => {
            budget.text(pattern)?;
            b.set_glob(pattern.as_str());
        }
        FileSystemPath::Special { value } => {
            let mut b = b.init_special();
            match value {
                FileSystemSpecialPath::Root => b.set_root(()),
                FileSystemSpecialPath::Minimal => b.set_minimal(()),
                FileSystemSpecialPath::Tmpdir => b.set_tmpdir(()),
                FileSystemSpecialPath::SlashTmp => b.set_slash_tmp(()),
                FileSystemSpecialPath::ProjectRoots { subpath } => {
                    put_optional_text(b.init_project_roots(), subpath, budget)?
                }
                FileSystemSpecialPath::Unknown { path, subpath } => {
                    let mut b = b.init_unknown();
                    budget.text(path)?;
                    b.set_path(path.as_str());
                    put_optional_text(b.init_subpath(), subpath, budget)?;
                }
            }
        }
    }
    Ok(())
}
fn get_path(r: wire::fs_path::Reader<'_>) -> Result<FileSystemPath> {
    Ok(match r.which()? {
        wire::fs_path::Uri(s) => FileSystemPath::Path {
            path: text(s?)?.parse().map_err(|_| invalid())?,
        },
        wire::fs_path::Glob(s) => FileSystemPath::GlobPattern { pattern: text(s?)? },
        wire::fs_path::Special(s) => FileSystemPath::Special {
            value: match s?.which()? {
                wire::special_path::Root(()) => FileSystemSpecialPath::Root,
                wire::special_path::Minimal(()) => FileSystemSpecialPath::Minimal,
                wire::special_path::Tmpdir(()) => FileSystemSpecialPath::Tmpdir,
                wire::special_path::SlashTmp(()) => FileSystemSpecialPath::SlashTmp,
                wire::special_path::ProjectRoots(s) => FileSystemSpecialPath::ProjectRoots {
                    subpath: get_optional_text(s?)?,
                },
                wire::special_path::Unknown(s) => {
                    let s = s?;
                    FileSystemSpecialPath::Unknown {
                        path: text(s.get_path()?)?,
                        subpath: get_optional_text(s.get_subpath()?)?,
                    }
                }
            },
        },
    })
}
fn put_profile(
    mut b: wire::permission_profile::Builder<'_>,
    p: &PermissionProfile,
    budget: &mut Budget,
) -> Result<()> {
    match p {
        PermissionProfile::Disabled => b.set_disabled(()),
        PermissionProfile::External { network } => b.set_external(put_network(*network)),
        PermissionProfile::Managed {
            file_system,
            network,
        } => {
            let mut b = b.init_managed();
            b.set_network(put_network(*network));
            let mut fs = b.init_file_system();
            match file_system {
                ManagedFileSystemPermissions::Unrestricted => fs.set_unrestricted(()),
                ManagedFileSystemPermissions::Restricted {
                    entries,
                    glob_scan_max_depth,
                } => {
                    let mut fs = fs.init_restricted();
                    put_optional_u64(
                        fs.reborrow().init_glob_scan_max_depth(),
                        glob_scan_max_depth.map(|n| n.get() as u64),
                    );
                    let mut list = fs.init_entries(budget.items(entries.len())?);
                    for (i, entry) in entries.iter().enumerate() {
                        let mut b = list.reborrow().get(i as u32);
                        b.set_access(match entry.access {
                            FileSystemAccessMode::Read => wire::Access::Read,
                            FileSystemAccessMode::Write => wire::Access::Write,
                            FileSystemAccessMode::Deny => wire::Access::Deny,
                        });
                        b.set_missing(match entry.missing_path_behavior {
                            None => wire::MissingPath::Unspecified,
                            Some(FileSystemSandboxEntryMissingPathBehavior::Skip) => {
                                wire::MissingPath::Skip
                            }
                        });
                        put_path(b.init_path(), &entry.path, budget)?;
                    }
                }
            }
        }
    }
    Ok(())
}
fn get_profile(r: wire::permission_profile::Reader<'_>) -> Result<PermissionProfile> {
    Ok(match r.which()? {
        wire::permission_profile::Disabled(()) => PermissionProfile::Disabled,
        wire::permission_profile::External(n) => PermissionProfile::External {
            network: get_network(n?),
        },
        wire::permission_profile::Managed(m) => {
            let m = m?;
            let file_system = match m.get_file_system()?.which()? {
                wire::managed_fs::Unrestricted(()) => ManagedFileSystemPermissions::Unrestricted,
                wire::managed_fs::Restricted(fs) => {
                    let fs = fs?;
                    let list = fs.get_entries()?;
                    let mut entries = Vec::with_capacity(count(list.len())?);
                    for item in list {
                        entries.push(FileSystemSandboxEntry {
                            path: get_path(item.get_path()?)?,
                            access: match item.get_access()? {
                                wire::Access::Read => FileSystemAccessMode::Read,
                                wire::Access::Write => FileSystemAccessMode::Write,
                                wire::Access::Deny => FileSystemAccessMode::Deny,
                            },
                            missing_path_behavior: match item.get_missing()? {
                                wire::MissingPath::Unspecified => None,
                                wire::MissingPath::Skip => {
                                    Some(FileSystemSandboxEntryMissingPathBehavior::Skip)
                                }
                            },
                        });
                    }
                    let glob_scan_max_depth = get_optional_u64(fs.get_glob_scan_max_depth()?)?
                        .map(|n| {
                            NonZeroUsize::new(usize::try_from(n).map_err(|_| invalid())?)
                                .ok_or_else(invalid)
                        })
                        .transpose()?;
                    ManagedFileSystemPermissions::Restricted {
                        entries,
                        glob_scan_max_depth,
                    }
                }
            };
            PermissionProfile::Managed {
                file_system,
                network: get_network(m.get_network()?),
            }
        }
    })
}
fn put_spawn(
    mut b: wire::spawn_request::Builder<'_>,
    s: &SpawnRequest,
    budget: &mut Budget,
) -> Result<()> {
    put_texts(
        b.reborrow().init_command(budget.items(s.command.len())?),
        &s.command,
        budget,
    )?;
    let cwd = path(&s.cwd)?;
    budget.text(cwd)?;
    b.set_cwd(cwd);
    let codex_home = path(&s.codex_home)?;
    budget.text(codex_home)?;
    b.set_codex_home(codex_home);
    let real_codex_home = path(&s.real_codex_home)?;
    budget.text(real_codex_home)?;
    b.set_real_codex_home(real_codex_home);
    // HashMap iteration is never part of the authenticated wire representation.
    let mut env: Vec<_> = s.env.iter().collect();
    env.sort_unstable_by(|a, b| a.0.cmp(b.0));
    let mut list = b.reborrow().init_env(budget.items(env.len())?);
    for (i, (key, value)) in env.into_iter().enumerate() {
        budget.text(key)?;
        budget.text(value)?;
        let mut e = list.reborrow().get(i as u32);
        e.set_key(key.as_str());
        e.set_value(value.as_str());
    }
    put_profile(
        b.reborrow().init_permission_profile(),
        &s.permission_profile,
        budget,
    )?;
    let mut roots = b
        .reborrow()
        .init_workspace_roots(budget.items(s.workspace_roots.len())?);
    for (i, root) in s.workspace_roots.iter().enumerate() {
        let value = path(root.as_path())?;
        budget.text(value)?;
        roots.set(i as u32, value);
    }
    put_texts(
        b.reborrow().init_cap_sids(budget.items(s.cap_sids.len())?),
        &s.cap_sids,
        budget,
    )?;
    put_optional_text(
        b.reborrow().init_network_proxy_restricting_sid(),
        &s.network_proxy_restricting_sid,
        budget,
    )?;
    put_optional_u64(b.reborrow().init_timeout_ms(), s.timeout_ms);
    b.set_tty(s.tty);
    b.set_stdin_open(s.stdin_open);
    put_optional_text(
        b.init_private_desktop_name(),
        &s.private_desktop_name,
        budget,
    )?;
    Ok(())
}
fn get_spawn(r: wire::spawn_request::Reader<'_>) -> Result<SpawnRequest> {
    let list = r.get_env()?;
    let mut env = HashMap::with_capacity(count(list.len())?);
    for entry in list {
        ensure!(
            env.insert(text(entry.get_key()?)?, text(entry.get_value()?)?)
                .is_none(),
            "Duplicate Runner environment key"
        );
    }
    let roots = get_texts(r.get_workspace_roots()?)?
        .into_iter()
        .map(|s| AbsolutePathBuf::from_absolute_path(PathBuf::from(s)).map_err(|_| invalid()))
        .collect::<Result<Vec<_>>>()?;
    Ok(SpawnRequest {
        command: get_texts(r.get_command()?)?,
        cwd: PathBuf::from(text(r.get_cwd()?)?),
        env,
        permission_profile: get_profile(r.get_permission_profile()?)?,
        workspace_roots: roots,
        codex_home: PathBuf::from(text(r.get_codex_home()?)?),
        real_codex_home: PathBuf::from(text(r.get_real_codex_home()?)?),
        cap_sids: get_texts(r.get_cap_sids()?)?,
        network_proxy_restricting_sid: get_optional_text(r.get_network_proxy_restricting_sid()?)?,
        timeout_ms: get_optional_u64(r.get_timeout_ms()?)?,
        tty: r.get_tty(),
        stdin_open: r.get_stdin_open(),
        private_desktop_name: get_optional_text(r.get_private_desktop_name()?)?,
    })
}

pub(crate) fn encode_frame_payload(frame: &FramedMessage) -> Result<Vec<u8>> {
    ensure!(
        matches!(frame.version, 6 | 7 | 8),
        "Unsupported Runner control version"
    );
    let mut message = capnp::message::Builder::new_default();
    let mut b = message.init_root::<wire::envelope::Builder<'_>>();
    b.set_magic(MAGIC);
    b.set_wire_revision(WIRE_REVISION);
    b.set_schema(&crate::wire_schema_pins::RUNNER_SCHEMA);
    b.set_control_version(frame.version);
    let mut budget = Budget(1024);
    match &frame.message {
        Message::Hello { payload } => put_hello(b.init_hello(), payload),
        Message::HelloAck { payload } => {
            let mut b = b.init_hello_ack();
            put_hello(b.reborrow().init_hello(), &payload.hello);
            put_controls(b.init_controls(), payload.controls);
        }
        Message::ControlRequest { payload } => put_control(b.init_control_request(), payload),
        Message::ControlResult { payload } => {
            let mut b = b.init_control_result();
            put_control(b.reborrow().init_request(), &payload.request);
            b.set_status(match payload.status {
                ControlStatus::Applied => wire::ControlStatus::Applied,
                ControlStatus::Rejected => wire::ControlStatus::Rejected,
                ControlStatus::Unsupported => wire::ControlStatus::Unsupported,
                ControlStatus::Unknown => wire::ControlStatus::Unknown,
            });
            let mut n = b.init_os_error();
            match payload.os_error {
                None => n.set_none(()),
                Some(v) => n.set_some(v),
            }
        }
        Message::SpawnRequest { payload } => {
            put_spawn(b.init_spawn_request(), payload, &mut budget)?
        }
        Message::SpawnReady { payload } => {
            let mut b = b.init_spawn_ready();
            b.set_process_id(payload.process_id);
            put_controls(b.init_controls(), payload.controls);
        }
        Message::Output { payload } => {
            ensure!(
                payload.data_b64.len() <= MAX_FRAME_BYTES,
                "Runner output limit exceeded"
            );
            let data = decode_bytes(&payload.data_b64)?;
            budget.add(data.len())?;
            let mut b = b.init_output();
            b.set_data(&data);
            b.set_stream(match payload.stream {
                OutputStream::Stdout => wire::OutputStream::Stdout,
                OutputStream::Stderr => wire::OutputStream::Stderr,
            });
        }
        Message::Stdin { payload } => {
            ensure!(
                payload.data_b64.len() <= MAX_FRAME_BYTES,
                "Runner stdin limit exceeded"
            );
            let data = decode_bytes(&payload.data_b64)?;
            budget.add(data.len())?;
            b.set_stdin(&data);
        }
        Message::CloseStdin { .. } => b.set_close_stdin(()),
        Message::Resize { payload } => {
            let mut b = b.init_resize();
            b.set_rows(payload.rows);
            b.set_cols(payload.cols);
        }
        Message::Exit { payload } => {
            let mut b = b.init_exit();
            b.set_exit_code(payload.exit_code);
            b.set_timed_out(payload.timed_out);
        }
        Message::Error { payload } => {
            budget.text(&payload.message)?;
            let mut b = b.init_error();
            b.set_message(payload.message.as_str());
            b.set_stage(match payload.stage {
                ErrorStage::ReadSpawnRequest => wire::ErrorStage::ReadSpawnRequest,
                ErrorStage::SpawnChild => wire::ErrorStage::SpawnChild,
                ErrorStage::WriteSpawnReady => wire::ErrorStage::WriteSpawnReady,
                ErrorStage::TerminalWitness => wire::ErrorStage::TerminalWitness,
            });
            let mut n = b.init_windows_error_code();
            match payload.windows_error_code {
                None => n.set_none(()),
                Some(v) => n.set_some(v),
            }
        }
        Message::Terminate { .. } => b.set_terminate(()),
    }
    let words = message.into_reader().canonicalize()?;
    let segments = [capnp::Word::words_to_bytes(&words)];
    let bytes = capnp::serialize::write_message_segments_to_words(&segments.as_slice());
    ensure!(
        bytes.len() <= MAX_FRAME_BYTES,
        "Runner frame limit exceeded"
    );
    Ok(bytes)
}

fn decode_inner(bytes: &[u8]) -> Result<FramedMessage> {
    ensure!(
        !bytes.is_empty() && bytes.len() <= MAX_FRAME_BYTES,
        "Runner frame size rejected"
    );
    let options = capnp::message::ReaderOptions {
        // A tiny nested v8 payload must not inherit the general 8 MiB budget.
        // Permit the canonical-layout pass and typed reads while bounding
        // repeated-pointer amplification by the actual received byte length.
        traversal_limit_in_words: Some((bytes.len() / 8).saturating_mul(4)),
        nesting_limit: 16,
    };
    let mut remaining = bytes;
    let message = capnp::serialize::read_message_from_flat_slice(&mut remaining, options)?;
    ensure!(remaining.is_empty(), "Trailing Runner frame bytes");
    ensure!(message.is_canonical()?, "Noncanonical Runner segments");
    let r = message.get_root::<wire::envelope::Reader<'_>>()?;
    ensure!(
        r.get_magic() == MAGIC
            && r.get_wire_revision() == WIRE_REVISION
            && r.get_schema()? == crate::wire_schema_pins::RUNNER_SCHEMA,
        "Runner wire identity mismatch"
    );
    let version = r.get_control_version();
    ensure!(
        matches!(version, 6 | 7 | 8),
        "Unsupported Runner control version"
    );
    let message = match r.which()? {
        wire::envelope::Hello(v) => Message::Hello {
            payload: get_hello(v?)?,
        },
        wire::envelope::HelloAck(v) => {
            let v = v?;
            Message::HelloAck {
                payload: ControlHelloAck {
                    hello: get_hello(v.get_hello()?)?,
                    controls: get_controls(v.get_controls()?),
                },
            }
        }
        wire::envelope::ControlRequest(v) => Message::ControlRequest {
            payload: get_control(v?)?,
        },
        wire::envelope::ControlResult(v) => {
            let v = v?;
            Message::ControlResult {
                payload: ControlResult {
                    request: get_control(v.get_request()?)?,
                    status: match v.get_status()? {
                        wire::ControlStatus::Applied => ControlStatus::Applied,
                        wire::ControlStatus::Rejected => ControlStatus::Rejected,
                        wire::ControlStatus::Unsupported => ControlStatus::Unsupported,
                        wire::ControlStatus::Unknown => ControlStatus::Unknown,
                    },
                    os_error: match v.get_os_error()?.which()? {
                        wire::maybe_i32::None(()) => None,
                        wire::maybe_i32::Some(n) => Some(n),
                    },
                },
            }
        }
        wire::envelope::SpawnRequest(v) => Message::SpawnRequest {
            payload: Box::new(get_spawn(v?)?),
        },
        wire::envelope::SpawnReady(v) => {
            let v = v?;
            Message::SpawnReady {
                payload: SpawnReady {
                    process_id: v.get_process_id(),
                    controls: get_controls(v.get_controls()?),
                },
            }
        }
        wire::envelope::Output(v) => {
            let v = v?;
            Message::Output {
                payload: OutputPayload {
                    data_b64: encode_bytes(v.get_data()?),
                    stream: match v.get_stream()? {
                        wire::OutputStream::Stdout => OutputStream::Stdout,
                        wire::OutputStream::Stderr => OutputStream::Stderr,
                    },
                },
            }
        }
        wire::envelope::Stdin(v) => Message::Stdin {
            payload: StdinPayload {
                data_b64: encode_bytes(v?),
            },
        },
        wire::envelope::CloseStdin(()) => Message::CloseStdin {
            payload: EmptyPayload {},
        },
        wire::envelope::Resize(v) => {
            let v = v?;
            Message::Resize {
                payload: ResizePayload {
                    rows: v.get_rows(),
                    cols: v.get_cols(),
                },
            }
        }
        wire::envelope::Exit(v) => {
            let v = v?;
            Message::Exit {
                payload: ExitPayload {
                    exit_code: v.get_exit_code(),
                    timed_out: v.get_timed_out(),
                },
            }
        }
        wire::envelope::Error(v) => {
            let v = v?;
            Message::Error {
                payload: ErrorPayload {
                    message: text(v.get_message()?)?,
                    stage: match v.get_stage()? {
                        wire::ErrorStage::ReadSpawnRequest => ErrorStage::ReadSpawnRequest,
                        wire::ErrorStage::SpawnChild => ErrorStage::SpawnChild,
                        wire::ErrorStage::WriteSpawnReady => ErrorStage::WriteSpawnReady,
                        wire::ErrorStage::TerminalWitness => ErrorStage::TerminalWitness,
                    },
                    windows_error_code: match v.get_windows_error_code()?.which()? {
                        wire::maybe_u32::None(()) => None,
                        wire::maybe_u32::Some(n) => Some(n),
                    },
                },
            }
        }
        wire::envelope::Terminate(()) => Message::Terminate {
            payload: EmptyPayload {},
        },
    };
    let result = FramedMessage { version, message };
    // A pinned wire has one exact encoding: reject unknown fields, omitted
    // explicit fields, extra segments, duplicate maps and nonzero padding.
    ensure!(
        encode_frame_payload(&result)? == bytes,
        "Noncanonical Runner frame"
    );
    Ok(result)
}
pub(crate) fn decode_frame_payload(bytes: &[u8]) -> Result<FramedMessage> {
    // Never carry parser details or untrusted payload strings into diagnostics.
    decode_inner(bytes).map_err(|_| invalid())
}

#[cfg(test)]
#[path = "runner_wire_tests.rs"]
mod tests;
