// Controlled ordinary Python pipe. This fixture grants no real service authority.
import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';
import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:morrow_studio/plugins/agent_wrapper_codec_native.dart';
import 'package:morrow_studio/plugins/agent_wrapper_models.dart';
import 'package:morrow_studio/plugins/generated/host.capnp.dart' as host;
import 'package:morrow_studio/plugins/generated/identity.dart' as contract;
import 'package:morrow_studio/plugins/service_run_control.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';
import 'external_plugin_native_test.dart' show removeTestDirectory;

final routingTask = Uint8List.fromList(List.filled(32, 11));
final _startSubmission = Uint8List.fromList(List.filled(32, 12));
final _command = Uint8List.fromList(List.filled(32, 13));
final _digest = Uint8List.fromList(List.generate(32, (i) => i + 1));

ServiceRunRequest routingStart() => ServiceRunRequest(
  submission: _startSubmission,
  configId: 'controlled-routing',
  configDigest: _digest,
  configRevision: BigInt.one,
  publication: _command,
  publicationRevision: BigInt.one,
  packageId: 'org.example.routing',
  packageDigest: _digest,
  registryRevision: BigInt.one,
  lifetimeMs: 60000,
  maxJobs: BigInt.from(8),
  maxBytes: BigInt.from(1024 * 1024),
  maxJobBytes: BigInt.from(256 * 1024),
  maxTotalBytes: BigInt.from(1024 * 1024),
);

Uint8List _reply(
  void Function(host.ResponseBuilder) configure, {
  bool readOnly = false,
}) {
  final message = MessageBuilder();
  final r = message.initRoot(host.responseFactory);
  r.version = 1;
  r.digest = Uint8List.fromList(contract.hostDigest);
  r.readOnly = readOnly;
  configure(r);
  return message.serialize();
}

final class ControlledRoutingScript {
  bool businessReady = false;
  final actions = <host.Action>[];
  final events = <String>[];
  final businessActions = <host.Action>[];
  final adminRequests = <AgentWrapperRequest>[];
  final adminFrames = <Uint8List>[];
  final submitted = Completer<void>();
  final statusObserved = Completer<void>();
  Uint8List? _submission;
  Uint8List _run() => _reply((r) {
    final run = r.initServiceRun();
    run.submission = _startSubmission;
    run.phase = 1;
    run.bind = 1;
    run.address = '127.0.0.1:43210';
    final task = run.initTask();
    task.key = routingTask;
    task.submission = _startSubmission;
    task.storage = 1;
  }, readOnly: true);
  Uint8List _commandReply(int delivery, {bool result = false}) => _reply((r) {
    final command = r.initOwnerCommand();
    command.key = _command;
    command.submission = _submission!;
    command.delivery = delivery;
    command.started = true;
    if (result) {
      r.payload = _reply((inner) {
        inner.payload = Uint8List.fromList(utf8.encode('en'));
        inner.revision = 7;
      });
    }
  }, readOnly: true);
  Uint8List respond(Uint8List frame) {
    if (frame.length >= 8 &&
        AgentWrapperValidation.same(
          frame.sublist(0, 8),
          AgentWrapperCodec.prefix,
        )) {
      final request = AgentWrapperCodec.decodeRequest(frame);
      adminFrames.add(Uint8List.fromList(frame));
      adminRequests.add(request);
      events.add('admin:${request.action.name}');
      return AgentWrapperCodec.encodeReply(
        AgentWrapperReply(
          action: request.action,
          status: AgentWrapperStatus.busy,
          id: request.id,
          requestSha256: AgentWrapperCodec.hash(frame),
          result: AgentWrapperResult(
            revisions: AgentWrapperRevisions(
              catalog: BigInt.one,
              manager: BigInt.one,
            ),
          ),
        ),
      );
    }
    final request = RustWorkbench.readMessage(
      frame,
    ).getRoot(host.requestFactory);
    final action = request.action!;
    actions.add(action);
    events.add(action.name);
    switch (action) {
      case host.Action.pageVersioned:
        return _reply((_) {});
      case host.Action.serviceRunStart:
      case host.Action.serviceRunStatus:
        return _run();
      case host.Action.commandSubmit:
        final innerBytes = request.payload!;
        if (innerBytes.length >= 8 &&
            AgentWrapperValidation.same(
              innerBytes.sublist(0, 8),
              AgentWrapperCodec.prefix,
            )) {
          throw StateError('Admin profile was forwarded to a guest command');
        }
        final inner = RustWorkbench.readMessage(
          innerBytes,
        ).getRoot(host.requestFactory);
        if (inner.action != host.Action.readUiLocale) {
          throw StateError('Unexpected forwarded business request');
        }
        businessActions.add(inner.action!);
        _submission = Uint8List.fromList(request.commandSubmission!);
        if (!submitted.isCompleted) submitted.complete();
        return _commandReply(0);
      case host.Action.commandStatus:
        if (!statusObserved.isCompleted) statusObserved.complete();
        return _commandReply(businessReady ? 1 : 0);
      case host.Action.commandRead:
        if (!businessReady) {
          throw StateError('Business result read before release');
        }
        return _commandReply(2, result: true);
      default:
        throw StateError('Unexpected original host action: $action');
    }
  }
}

final class ControlledRoutingChild {
  ControlledRoutingChild._(
    this.directory,
    this.backend,
    this.script,
    this._server,
    this._stop,
  );
  final Directory directory;
  final RustWorkbench backend;
  final ControlledRoutingScript script;
  final Future<void> _server;
  final void Function() _stop;
  static Future<ControlledRoutingChild> open(
    String python,
    ControlledRoutingScript script,
  ) async {
    final directory = await Directory.systemTemp.createTemp(
      'morrow-external-agent-admin-routing-',
    );
    // RustWorkbench.open passes this synthetic script as its ordinary executable argument.
    await File('${directory.path}/workbench.db').writeAsString(r'''
import pathlib,struct,sys,time,traceback
root=pathlib.Path(__file__).parent
def exact(n):
 data=b''
 while len(data)<n:
  part=sys.stdin.buffer.read(n-len(data))
  if not part: raise EOFError()
  data+=part
 return data
try:
 index=0
 while True:
  size=struct.unpack('<I',exact(4))[0]
  if not 0<size<=131072: raise ValueError('request size')
  temporary=root/('request.%d.tmp'%index)
  temporary.write_bytes(exact(size))
  temporary.rename(root/('request.%d.bin'%index))
  reply=root/('reply.%d.bin'%index)
  deadline=time.monotonic()+15
  while True:
   try:
    data=reply.read_bytes()
    break
   except (FileNotFoundError, PermissionError):
    if time.monotonic()>deadline: raise TimeoutError('controlled reply')
    time.sleep(.002)
  sys.stdout.buffer.write(struct.pack('<I',len(data))+data)
  sys.stdout.buffer.flush()
  index+=1
except EOFError:
 (root/'eof-observed').write_text('EOF')
 time.sleep(.05)
 (root/'drain-complete').write_text('joined')
except BaseException:
 (root/'child-error').write_text(traceback.format_exc())
 raise
''');
    var stopped = false;
    Object? serverError;
    Future<void> serve() async {
      for (var index = 0; !stopped; index++) {
        final request = File('${directory.path}/request.$index.bin');
        while (!await request.exists()) {
          if (stopped) return;
          await Future<void>.delayed(const Duration(milliseconds: 2));
        }
        Uint8List bytes;
        try {
          bytes = script.respond(await request.readAsBytes());
        } catch (error) {
          serverError ??= error;
          bytes = _reply(
            (r) => r.error = 'controlled routing fixture rejected frame',
          );
        }
        final temporary = File('${directory.path}/reply.$index.tmp');
        await temporary.writeAsBytes(bytes);
        await temporary.rename('${directory.path}/reply.$index.bin');
      }
    }

    final server = serve();
    try {
      final backend = await RustWorkbench.open(
        executable: python,
        package: 'controlled',
        directory: directory,
      );
      return ControlledRoutingChild._(directory, backend, script, server, () {
        stopped = true;
        if (serverError != null) throw serverError!;
      });
    } catch (_) {
      stopped = true;
      await server;
      await removeTestDirectory(directory);
      rethrow;
    }
  }

  Future<void> close() async {
    script.businessReady = true;
    try {
      await backend.close();
    } finally {
      _stop();
      await _server;
    }
  }

  Future<void> dispose() async {
    var closeFailed = false;
    try {
      await close();
    } catch (_) {
      closeFailed = true;
      rethrow;
    } finally {
      try {
        await removeTestDirectory(directory);
      } catch (error) {
        if (!closeFailed) {
          rethrow;
        }
        stderr.writeln('Secondary controlled routing cleanup failure: $error');
      }
    }
  }
}
