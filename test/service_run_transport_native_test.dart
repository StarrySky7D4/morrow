// Controlled Python protocol child: validates Dart pipe transport, not Rust
// service execution, durable effects, or native authorization implementation.
import 'dart:io';
import 'dart:typed_data';

import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/generated/host.capnp.dart' as host;
import 'package:morrow_studio/plugins/generated/identity.dart' as contract;
import 'package:morrow_studio/plugins/service_run_control.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';

import 'external_plugin_native_test.dart' show removeTestDirectory;

final _task = Uint8List.fromList(List.filled(32, 1));
final _submission = Uint8List.fromList(List.filled(32, 2));
final _command = Uint8List.fromList(List.filled(32, 3));

Uint8List _reply(void Function(host.ResponseBuilder) configure) {
  final message = MessageBuilder();
  final reply = message.initRoot(host.responseFactory);
  reply.version = 1;
  reply.digest = Uint8List.fromList(contract.hostDigest);
  reply.readOnly = true;
  reply.maintenanceWarning = 'scheduler observation must not replace state';
  configure(reply);
  return message.serialize();
}

Uint8List _initial() => _reply((r) {
  r.readOnly = false;
  r.maintenanceWarning = 'original maintenance';
});

Uint8List _runReply() => _reply((r) {
  final run = r.initServiceRun();
  run.submission = _submission;
  run.phase = 1;
  run.bind = 1;
  run.address = '127.0.0.1:43210';
  final task = run.initTask();
  task.key = _task;
  task.submission = _submission;
  task.storage = 1;
});

Uint8List _commandReply(int delivery, {Uint8List? payload}) => _reply((r) {
  final row = r.initOwnerCommand();
  row.key = _command;
  row.submission = _submission;
  row.delivery = delivery;
  row.started = true;
  if (payload != null) r.payload = payload;
});

Uint8List _businessRequest() {
  final message = MessageBuilder();
  final request = message.initRoot(host.requestFactory);
  request.version = 1;
  request.digest = Uint8List.fromList(contract.hostDigest);
  request.action = host.Action.readUiLocale;
  return message.serialize();
}

// Build an actual nested response close enough to the inner ceiling that its
// command envelope exceeds 128 KiB. Assert both bounds in each relevant test.
Uint8List _largeInner() {
  for (var length = 128 * 1024; length > 126 * 1024; length -= 8) {
    final inner = _reply((r) {
      r.maintenanceWarning = '';
      r.payload = Uint8List.fromList(List.filled(length, 0xe9));
    });
    if (inner.length <= 128 * 1024) return inner;
  }
  throw StateError('could not construct bounded nested response');
}

ServiceRunRequest _startRequest() => ServiceRunRequest(
  submission: _submission,
  configId: 'controlled-service',
  configDigest: _task,
  configRevision: BigInt.one,
  publication: _command,
  publicationRevision: BigInt.one,
  packageId: 'org.example.controlled',
  packageDigest: _task,
  registryRevision: BigInt.one,
  lifetimeMs: 60000,
  maxJobs: BigInt.from(8),
  maxBytes: BigInt.from(1024 * 1024),
  maxJobBytes: BigInt.from(256 * 1024),
  maxTotalBytes: BigInt.from(1024 * 1024),
);

class _Child {
  _Child(this.directory, this.backend);
  final Directory directory;
  final RustWorkbench backend;

  static Future<_Child> open(String python, List<Uint8List> replies) async {
    final directory = await Directory.systemTemp.createTemp(
      'morrow-external-service-transport-',
    );
    try {
      final encoded = BytesBuilder(copy: false);
      for (final reply in replies) {
        encoded.add(
          (ByteData(
            4,
          )..setUint32(0, reply.length, Endian.little)).buffer.asUint8List(),
        );
        encoded.add(reply);
      }
      final replyFile = File('${directory.path}/replies.bin');
      await replyFile.writeAsBytes(encoded.takeBytes());
      await File('${directory.path}/workbench.db').writeAsString('''
import pathlib,struct,sys
def exact(stream,n):
 data=b''
 while len(data)<n:
  part=stream.read(n-len(data))
  if not part: raise EOFError()
  data+=part
 return data
with open(sys.argv[1],'rb') as replies, open(__file__+'.requests','wb') as trace:
 try:
  while True:
   header=exact(sys.stdin.buffer,4)
   size=struct.unpack('<I',header)[0]
   if size>131072: raise ValueError('request limit')
   request=exact(sys.stdin.buffer,size)
   trace.write(header+request)
   trace.flush()
   output_header=exact(replies,4)
   output=exact(replies,struct.unpack('<I',output_header)[0])
   sys.stdout.buffer.write(output_header+output)
   sys.stdout.buffer.flush()
 except EOFError:
  pass
''');
      final backend = await RustWorkbench.open(
        executable: python,
        package: replyFile.path,
        directory: directory,
      );
      return _Child(directory, backend);
    } catch (_) {
      await removeTestDirectory(directory);
      rethrow;
    }
  }

  Future<List<host.Action>> actions({
    void Function(int, host.RequestReader)? inspect,
  }) async {
    final bytes = await File(
      '${directory.path}/workbench.db.requests',
    ).readAsBytes();
    final actions = <host.Action>[];
    var offset = 0;
    while (offset < bytes.length) {
      final length = ByteData.sublistView(
        bytes,
        offset,
        offset + 4,
      ).getUint32(0, Endian.little);
      offset += 4;
      final frame = Uint8List.sublistView(bytes, offset, offset + length);
      final request = RustWorkbench.readMessage(
        frame,
      ).getRoot(host.requestFactory);
      inspect?.call(actions.length, request);
      actions.add(request.action!);
      offset += length;
    }
    expect(offset, bytes.length);
    return actions;
  }

  Future<void> close() async {
    try {
      await backend.close();
    } finally {
      await removeTestDirectory(directory);
    }
  }
}

void _originalPresentation(RustWorkbench backend) {
  expect(backend.writable, isTrue);
  expect(backend.maintenanceWarning, 'original maintenance');
}

void main() {
  final python = Platform.environment['MORROW_CLOSE_TEST_PYTHON'];
  test(
    'validated host start error preserves exact diagnostic without retry',
    () async {
      const detail =
          'Service admission denied: declared run budget exceeded (请求预算过大)';
      final child = await _Child.open(python!, [
        _initial(),
        _reply((r) => r.error = detail),
      ]);
      try {
        await expectLater(
          child.backend.startServiceRun(_startRequest()),
          throwsA(
            isA<ServiceRunStartFailure>().having(
              (error) => error.message,
              'original host diagnostic',
              detail,
            ),
          ),
        );
        _originalPresentation(child.backend);
        await Future<void>.delayed(const Duration(milliseconds: 80));
        expect(
          await child.actions(
            inspect: (index, request) {
              if (index == 1) {
                expect(request.serviceRun!.submission, _submission);
                expect(request.serviceRun!.configId, 'controlled-service');
              }
            },
          ),
          [host.Action.page, host.Action.serviceRunStart],
        );
      } finally {
        await child.close();
      }
    },
    skip: python == null,
  );

  for (final invalid in [
    'wrong-digest',
    'malformed',
    'invalid-success',
    'lost-transport',
  ]) {
    test(
      '$invalid start response is never a positively classified host rejection',
      () async {
        final responses = <Uint8List>[_initial()];
        switch (invalid) {
          case 'wrong-digest':
            responses.add(
              _reply((r) {
                r.digest = Uint8List(32);
                r.error = 'untrusted admission denied';
              }),
            );
          case 'malformed':
            responses.add(Uint8List(8));
          case 'invalid-success':
            // Valid outer contract, but no successful run identity/state.
            responses.add(_reply((_) {}));
          case 'lost-transport':
            // The child logs the start, then reaches EOF in its reply script.
            // It cannot establish whether the actual host accepted the start.
            break;
        }
        final child = await _Child.open(python!, responses);
        try {
          await expectLater(
            child.backend.startServiceRun(_startRequest()),
            throwsA(isNot(isA<ServiceRunStartFailure>())),
          );
          _originalPresentation(child.backend);
          await Future<void>.delayed(const Duration(milliseconds: 80));
          expect(await child.actions(), [
            host.Action.page,
            host.Action.serviceRunStart,
          ]);
        } finally {
          await child.close();
        }
      },
      skip: python == null,
    );
  }

  test(
    'controlled child accepts large command-read and owns wiping payload',
    () async {
      final inner = _largeInner();
      final envelope = _commandReply(2, payload: inner);
      expect(inner.length, lessThanOrEqualTo(128 * 1024));
      expect(envelope.length, greaterThan(128 * 1024));
      expect(envelope.length, lessThanOrEqualTo(256 * 1024));
      final child = await _Child.open(python!, [
        _initial(),
        _runReply(),
        _runReply(),
        _commandReply(0),
        _commandReply(1),
        _commandReply(1),
        _commandReply(1),
        envelope,
        _commandReply(2),
        _commandReply(2),
      ]);
      OwnerCommandRead? result;
      try {
        final backend = child.backend;
        _originalPresentation(backend);
        expect(
          (await backend.startServiceRun(_startRequest())).phase,
          ServiceRunPhase.running,
        );
        _originalPresentation(backend);
        expect((await backend.serviceRunStatus(key: _task)).task.key, _task);
        _originalPresentation(backend);
        final input = _businessRequest();
        final original = Uint8List.fromList(input);
        expect(
          (await backend.submitServiceCommand(_task, _submission, input)).key,
          _command,
        );
        expect(
          input,
          original,
          reason: 'caller-owned request must not be wiped',
        );
        _originalPresentation(backend);
        final recovered = await backend.serviceCommandBySubmission(
          _task,
          _submission,
        );
        expect(recovered.key, _command);
        expect(recovered.submission, _submission);
        expect(recovered.delivery, OwnerCommandDelivery.ready);
        _originalPresentation(backend);
        for (var n = 0; n < 2; n++) {
          expect(
            (await backend.serviceCommandStatus(_task, _command)).delivery,
            OwnerCommandDelivery.ready,
          );
          _originalPresentation(backend);
        }
        result = await backend.readServiceCommand(_task, _command);
        expect(result.snapshot.delivery, OwnerCommandDelivery.consumed);
        final held = result.payload!;
        expect(
          held,
          inner,
          reason: 'must survive clearing transport reply arena',
        );
        _originalPresentation(backend);
        result.dispose();
        expect(result.disposed, isTrue);
        expect(result.payload, isNull);
        expect(held.every((byte) => byte == 0), isTrue);
        expect(inner.any((byte) => byte != 0), isTrue);
        result.dispose();
        final again = await backend.readServiceCommand(_task, _command);
        expect(again.payload, isNull);
        again.dispose();
        expect(
          (await backend.cancelServiceCommand(_task, _command)).delivery,
          OwnerCommandDelivery.consumed,
        );
        _originalPresentation(backend);
        var inspectedLookup = false;
        final actions = await child.actions(
          inspect: (index, request) {
            if (index != 4) return;
            inspectedLookup = true;
            expect(request.action, host.Action.commandStatus);
            expect(request.ioKey, _task);
            expect(request.commandSubmission, _submission);
            expect(request.commandKey ?? Uint8List(0), isEmpty);
            expect(
              request.payload ?? Uint8List(0),
              isEmpty,
              reason: 'receipt lookup must not resend the command body',
            );
          },
        );
        expect(inspectedLookup, isTrue);
        expect(actions, [
          host.Action.page,
          host.Action.serviceRunStart,
          host.Action.serviceRunStatus,
          host.Action.commandSubmit,
          host.Action.commandStatus,
          host.Action.commandStatus,
          host.Action.commandStatus,
          host.Action.commandRead,
          host.Action.commandRead,
          host.Action.commandCancel,
        ]);
      } finally {
        result?.dispose();
        await child.close();
      }
    },
    skip: python == null,
  );

  test(
    'controlled child large envelope remains invalid for ordinary reply',
    () async {
      final oversized = _commandReply(2, payload: _largeInner());
      expect(oversized.length, greaterThan(128 * 1024));
      final child = await _Child.open(python!, [_initial(), oversized]);
      try {
        await expectLater(child.backend.pluginState(), throwsFormatException);
        _originalPresentation(child.backend);
        expect(await child.actions(), [
          host.Action.page,
          host.Action.pluginState,
        ]);
      } finally {
        await child.close();
      }
    },
    skip: python == null,
  );

  test(
    'controlled child rejects oversized inner command response',
    () async {
      final oversized = _commandReply(2, payload: Uint8List(128 * 1024 + 1));
      expect(oversized.length, lessThanOrEqualTo(256 * 1024));
      final child = await _Child.open(python!, [_initial(), oversized]);
      try {
        await expectLater(
          child.backend.readServiceCommand(_task, _command),
          throwsFormatException,
        );
        _originalPresentation(child.backend);
        expect(await child.actions(), [
          host.Action.page,
          host.Action.commandRead,
        ]);
      } finally {
        await child.close();
      }
    },
    skip: python == null,
  );
}
