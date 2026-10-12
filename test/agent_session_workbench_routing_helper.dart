// SOURCE ONLY synthetic responses; no native host, authority, database or credentials.
import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';
import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:morrow_studio/plugins/agent_session_codec_native.dart';
import 'package:morrow_studio/plugins/generated/host.capnp.dart' as host;
import 'package:morrow_studio/plugins/generated/identity.dart' as identity;
import 'package:morrow_studio/plugins/workbench_native.dart';
import 'agent_wrapper_workbench_routing_helper.dart' as legacy;

final pipeTask = Uint8List.fromList(List.filled(32, 41));
final pipeAttachment = BigInt.parse('10535140504318566128');
final pipeToken = BigInt.parse('10535140504318566134');
final pipeGeneration = BigInt.parse('10535140504318566129');

final class _Reader extends StructReader {
  _Reader(super.raw);
  BigInt u64(int offset) =>
      (BigInt.from(getUint32Field(offset + 4).toUnsigned(32)) << 32) |
      BigInt.from(getUint32Field(offset).toUnsigned(32));
}

final class _Builder extends StructBuilder {
  _Builder(super.raw);
  @override
  _Reader asReader() => _Reader(rawToReader());
  void u64(int offset, BigInt value) {
    setUint32Field(offset, (value & BigInt.from(0xffffffff)).toInt());
    setUint32Field(offset + 4, (value >> 32).toInt());
  }
}

final class _Factory extends StructFactory<_Reader, _Builder> {
  _Factory(this.dataWords, this.ptrWords);
  @override
  final int dataWords, ptrWords;
  @override
  StructSchemaInfo get schema => StructSchemaInfo(
    id: 0,
    displayName: 'synthetic-private-session',
    shortName: 'Synthetic',
    dataWords: dataWords,
    pointerWords: ptrWords,
    fields: const [],
  );
  @override
  _Reader fromRawReader(RawStructReader r) => _Reader(r);
  @override
  _Builder fromRawBuilder(RawStructBuilder r) => _Builder(r);
}

Uint8List hostReply({String? locale}) {
  final message = MessageBuilder();
  final r = message.initRoot(host.responseFactory);
  r.version = 1;
  r.digest = Uint8List.fromList(identity.hostDigest);
  if (locale != null) r.payload = Uint8List.fromList(utf8.encode(locale));
  return message.serialize();
}

final class PipeReply {
  PipeReply(this.bytes, {this.drop = false, this.idle = false});
  final Uint8List bytes;
  final bool drop, idle;
}

final class SessionPipeScript {
  final legacy.ControlledRoutingScript business =
      legacy.ControlledRoutingScript();
  final privateOps = <int>[];
  final privateFrames = <Uint8List>[];
  int reads = 0;
  bool malformedPrivate = false, largePrivate = false, tooLargePrivate = false;
  bool largeLegacy = false, dropRead = false, idleAfterAttach = false;
  PipeReply respond(Uint8List frame) {
    if (frame.length >= 8 &&
        AgentSessionCodec.prefix.asMap().entries.every(
          (e) => frame[e.key] == e.value,
        )) {
      final q = MessageReader.deserialize(
        Uint8List.sublistView(frame, 8),
      ).getRoot(_Factory(10, 6));
      final op = q.getUint16Field(2);
      privateOps.add(op);
      privateFrames.add(Uint8List.fromList(frame));
      final message = MessageBuilder();
      final r = message.initRoot(_Factory(6, 9));
      r.setUint16Field(0, 1);
      r.setUint16Field(2, op);
      r.setDataField(0, AgentSessionCodec.schema);
      r.setDataField(1, q.getDataField(1)!);
      r.setDataField(2, q.getDataField(2)!);
      r.setDataField(3, AgentSessionCodec.hash(frame));
      r.setDataField(4, Uint8List(0));
      r.setDataField(5, Uint8List(32));
      r.u64(8, op == 1 ? pipeAttachment : q.u64(8));
      r.setBoolField(48, true);
      switch (op) {
        case 1:
          r.setUint16Field(4, 6);
          break;
        case 2:
          r.setUint16Field(4, 7);
          r.u64(16, pipeToken);
          break;
        case 4:
          r.setUint16Field(4, 11);
          r.u64(16, reads < 2 ? pipeToken : BigInt.zero);
          r.u64(32, reads < 2 ? BigInt.zero : pipeGeneration);
          break;
        case 5:
          reads++;
          r.u64(16, q.u64(16));
          r.setUint16Field(4, reads == 1 ? 8 : 9);
          if (reads > 1) r.u64(32, pipeGeneration);
          break;
        case 7:
          r.setUint16Field(4, 12);
          r.setBoolField(48, false);
          break;
        default:
          throw StateError('Fixture rejects unplanned private operation $op');
      }
      var bytes = Uint8List.fromList([
        ...AgentSessionCodec.prefix,
        ...message.serialize(),
      ]);
      if (malformedPrivate) bytes[0] ^= 1;
      // These are invalid codec payloads, only probing transport acceptance.
      if (largePrivate) bytes = Uint8List(300 * 1024);
      if (tooLargePrivate) bytes = Uint8List(384 * 1024 + 1);
      return PipeReply(
        bytes,
        drop: dropRead && op == 5,
        idle: idleAfterAttach && op == 1,
      );
    }
    final request = RustWorkbench.readMessage(
      frame,
    ).getRoot(host.requestFactory);
    if (request.action == host.Action.readUiLocale)
      return PipeReply(
        largeLegacy ? Uint8List(300 * 1024) : hostReply(locale: 'en'),
      );
    return PipeReply(business.respond(frame));
  }
}

// Retain every unique fixture and raw frame. Never remove any directory or file.
final class SessionPipeChild {
  SessionPipeChild._(
    this.directory,
    this.backend,
    this.script,
    this._server,
    this._stop,
  );
  final Directory directory;
  final RustWorkbench backend;
  final SessionPipeScript script;
  final Future<void> _server;
  final void Function() _stop;
  bool _closed = false;
  static Future<SessionPipeChild> open(
    String python,
    Directory evidenceParent,
    SessionPipeScript script,
  ) async {
    if (!File(python).isAbsolute || !await File(python).exists())
      throw ArgumentError('Supply an absolute ordinary Python executable');
    if (!evidenceParent.isAbsolute || !await evidenceParent.exists())
      throw ArgumentError(
        'Supply an existing independent synthetic evidence parent',
      );
    final directory = await evidenceParent.createTemp('original-session-pipe-');
    await File(
      '${directory.path}/workbench.db',
    ).writeAsString(_python, mode: FileMode.writeOnly);
    var stopped = false;
    Object? error;
    Future<void> serve() async {
      for (var index = 0; !stopped; index++) {
        final input = File('${directory.path}/request.$index.bin');
        while (!await input.exists()) {
          if (stopped) return;
          await Future<void>.delayed(const Duration(milliseconds: 2));
        }
        try {
          final bytes = await input.readAsBytes();
          if (bytes.isEmpty || bytes.length > 128 * 1024)
            throw StateError('Synthetic input cap');
          final reply = script.respond(bytes);
          if (reply.bytes.length > 384 * 1024 + 1)
            throw StateError('Synthetic output cap');
          await File(
            '${directory.path}/reply.$index.bin',
          ).writeAsBytes(reply.bytes, mode: FileMode.writeOnly, flush: true);
          final temporary = File('${directory.path}/reply.$index.tmp');
          await temporary.writeAsString(
            jsonEncode({'drop': reply.drop, 'idle': reply.idle}),
            mode: FileMode.writeOnly,
            flush: true,
          );
          await temporary.rename('${directory.path}/reply.$index.json');
        } catch (e) {
          error ??= e;
          final marker = File('${directory.path}/reply.$index.tmp');
          await marker.writeAsString(
            '{"drop":true,"idle":false}',
            mode: FileMode.writeOnly,
            flush: true,
          );
          await marker.rename('${directory.path}/reply.$index.json');
        }
      }
    }

    final server = serve();
    try {
      final backend = await RustWorkbench.open(
        executable: python,
        package: 'synthetic-session-pipe',
        directory: directory,
      );
      return SessionPipeChild._(directory, backend, script, server, () {
        stopped = true;
        if (error != null) throw error!;
      });
    } catch (_) {
      stopped = true;
      await server;
      rethrow;
    }
  }

  Future<void> emitIdle(Uint8List bytes) async {
    if (bytes.length > 384 * 1024 + 1)
      throw ArgumentError('Synthetic idle cap');
    final f = File('${directory.path}/idle.tmp');
    await f.writeAsBytes(bytes, flush: true);
    await f.rename('${directory.path}/idle.bin');
    await waitMarker('idle-sent');
  }

  Future<void> waitMarker(String name) async {
    final timer = Stopwatch()..start();
    while (!await File('${directory.path}/$name').exists()) {
      if (timer.elapsed > const Duration(seconds: 10))
        throw TimeoutException('Synthetic marker $name');
      await Future<void>.delayed(const Duration(milliseconds: 2));
    }
  }

  Future<void> close() async {
    if (_closed) return;
    _closed = true;
    script.business.businessReady = true;
    try {
      await backend.close();
    } finally {
      _stop();
      await _server;
    }
  }
}

const _python = r'''
import pathlib,struct,sys,time,json
root=pathlib.Path(__file__).parent
def exact(n):
 data=b''
 while len(data)<n:
  part=sys.stdin.buffer.read(n-len(data))
  if not part: raise EOFError()
  data+=part
 return data
def wait(path):
 deadline=time.monotonic()+15
 while True:
  try:return path.read_bytes()
  except (FileNotFoundError,PermissionError):
   if time.monotonic()>deadline:raise TimeoutError('synthetic fixture deadline')
   time.sleep(.002)
def send(data):
 sys.stdout.buffer.write(struct.pack('<I',len(data))+data);sys.stdout.buffer.flush()
try:
 index=0
 while True:
  size=struct.unpack('<I',exact(4))[0]
  if not 0<size<=131072:raise ValueError('synthetic request cap')
  temporary=root/('request.%d.tmp'%index);temporary.write_bytes(exact(size));temporary.rename(root/('request.%d.bin'%index))
  control=json.loads(wait(root/('reply.%d.json'%index)))
  if control['drop']:
   (root/'deliberate-eof').write_text('synthetic missing reply');break
  data=wait(root/('reply.%d.bin'%index))
  if len(data)>393217:raise ValueError('synthetic output cap')
  send(data)
  if control['idle']:
   data=wait(root/'idle.bin')
   if len(data)>393217:raise ValueError('synthetic idle cap')
   send(data);(root/'idle-sent').write_text('sent')
  index+=1
except EOFError:
 (root/'eof-observed').write_text('EOF')
except BaseException as error:
 (root/'child-error').write_text(type(error).__name__);raise
finally:
 (root/'drain-complete').write_text('synthetic process finally')
''';
