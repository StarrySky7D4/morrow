"""Versioned owner derivative; original002 remains frozen, no runtime invocation."""
import pathlib,hashlib,json,shutil
ROOT=pathlib.Path(__file__).resolve().parents[1];OLD=ROOT/'native_session_owner_002';NEW=ROOT/'native_session_stream_001';assert not NEW.exists()
shutil.copytree(OLD,NEW,ignore=shutil.ignore_patterns('target'))
(NEW/'provenance').mkdir();sources={str(p.relative_to(OLD)).replace('\\','/'):hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(OLD.rglob('*')) if p.is_file() and 'target' not in p.relative_to(OLD).parts}
(NEW/'provenance/origin.json').write_text(json.dumps({'source':'native_session_owner_002','files':sources,'status':'work in progress; old inputs unchanged'},indent=2)+'\n')
manifest=(NEW/'Cargo.toml').read_text().replace('morrow-native-session-owner','morrow-native-session-stream').replace('morrow-native-owner-host','morrow-native-stream-host').replace('version = "0.2.0-experimental.1"','version = "0.3.0-experimental.1"')
manifest=manifest.replace('morrow-native-session-wire = { path = "../contracts/experimental/agent_host_v2_capnp" }','morrow-native-http-stream-wire = { path = "../contracts/experimental/agent_host_v3_http_stream" }\nmorrow-network-node-stream = { path = "../network_node_stream_001" }\nmorrow-native-pipe-win = { path = "../native_pipe_win_001" }\ntokio-util = { version = "=0.7.19", features = ["rt"] }\nurl = "=2.5.8"')
(NEW/'Cargo.toml').write_text(manifest)
p=NEW/'src/main.rs';p.write_text(p.read_text().replace('morrow_native_session_owner','morrow_native_session_stream'))
(NEW/'README.md').write_text('''# M03 native stream owner — WORK IN PROGRESS

This new version derives from frozen native_session_owner_002. It is not compiled, ready,
or accepted as a native HTTP runtime. Dependency source/lock kits for transport, v3 codec
and Windows owned-I/O platform are separately frozen. The old owner, its evidence and all
original source directories remain unchanged. Implementation and producer/joint execution
must complete before any end-to-end or release claim.
''')
print(NEW)
