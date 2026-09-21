// Controlled Python pipe child: proves Dart routing and delivery ownership.
// Real Rust authorization, durable effects and service exit have separate tests.
import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/generated/host.capnp.dart' as host;
import 'package:morrow_studio/plugins/generated/identity.dart' as contract;
import 'package:morrow_studio/plugins/service_run_control.dart';
import 'package:morrow_studio/plugins/workbench_backend.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';

import 'external_plugin_native_test.dart' show removeTestDirectory;

final _task = Uint8List.fromList(List.filled(32, 11));
final _startSubmission = Uint8List.fromList(List.filled(32, 12));
final _command = Uint8List.fromList(List.filled(32, 13));
final _digest = Uint8List.fromList(List.generate(32, (i) => i + 1));

Uint8List _reply(
  void Function(host.ResponseBuilder) configure, {
  bool readOnly = false,
  String warning = 'original owner',
}) {
  final message = MessageBuilder();
  final r = message.initRoot(host.responseFactory);
  r.version = 1;
  r.digest = Uint8List.fromList(contract.hostDigest);
  r.readOnly = readOnly;
  r.maintenanceWarning = warning;
  configure(r);
  return message.serialize();
}

Uint8List _locale(String value) => _reply((r) {
  r.payload = Uint8List.fromList(utf8.encode(value));
  r.revision = 7;
});

ServiceRunRequest _start() => ServiceRunRequest(
  submission: _startSubmission,
  configId: 'controlled-service',
  configDigest: _digest,
  configRevision: BigInt.one,
  publication: _command,
  publicationRevision: BigInt.one,
  packageId: 'org.example.controlled',
  packageDigest: _digest,
  registryRevision: BigInt.one,
  lifetimeMs: 60000,
  maxJobs: BigInt.from(8),
  maxBytes: BigInt.from(1024 * 1024),
  maxJobBytes: BigInt.from(256 * 1024),
  maxTotalBytes: BigInt.from(1024 * 1024),
);

class _Script {
  _Script(this.business);
  final Uint8List Function(host.RequestReader) business;
  Completer<void>? startGate;
  bool ready = true;
  bool unknown = false;
  final actions = <host.Action>[];
  final commands = <host.RequestReader>[];
  Uint8List? submission;
  Uint8List? result;

  Uint8List _run() => _reply(
    (r) {
      final run = r.initServiceRun();
      run.submission = _startSubmission;
      run.phase = 1;
      run.bind = 1;
      run.address = '127.0.0.1:43210';
      final task = run.initTask();
      task.key = _task;
      task.submission = _startSubmission;
      task.storage = 1;
    },
    readOnly: true,
    warning: 'outer scheduler',
  );

  Uint8List _commandReply(int delivery, {bool reading = false}) => _reply(
    (r) {
      final command = r.initOwnerCommand();
      command.key = _command;
      command.submission = submission!;
      command.delivery = delivery;
      command.started = true;
      if (reading && unknown) command.terminal = 5;
      if (reading && !unknown) r.payload = result!;
    },
    readOnly: true,
    warning: 'outer scheduler',
  );

  Future<Uint8List> respond(host.RequestReader request) async {
    actions.add(request.action!);
    switch (request.action!) {
      case host.Action.page:
        return _reply((_) {});
      case host.Action.serviceRunStart:
        await startGate?.future;
        return _run();
      case host.Action.serviceRunStatus:
        return _run();
      case host.Action.ioStatus:
      case host.Action.ioCancel:
        return _reply(
          (r) {
            final task = r.initIoState();
            task.key = _task;
            task.submission = _startSubmission;
            task.storage = 1;
          },
          readOnly: true,
          warning: 'outer IO scheduler',
        );
      case host.Action.commandSubmit:
        expect(request.ioKey, _task);
        submission = Uint8List.fromList(request.commandSubmission!);
        expect(submission!.length, 32);
        expect(submission!.any((v) => v != 0), isTrue);
        final inner = RustWorkbench.readMessage(
          request.payload!,
        ).getRoot(host.requestFactory);
        expect(inner.version, 1);
        expect(inner.digest, contract.hostDigest);
        commands.add(inner);
        result = business(inner);
        return _commandReply(0);
      case host.Action.commandStatus:
        expect(request.ioKey, _task);
        expect(request.commandKey, _command);
        expect(request.payload ?? Uint8List(0), isEmpty);
        return _commandReply(ready ? 1 : 0);
      case host.Action.commandRead:
        expect(request.ioKey, _task);
        expect(request.commandKey, _command);
        expect(request.payload ?? Uint8List(0), isEmpty);
        return _commandReply(2, reading: true);
      default:
        throw StateError('ordinary action bypassed service: ${request.action}');
    }
  }
}

class _Child {
  _Child(this.directory, this.backend, this.script, this._server, this._stop);
  final Directory directory;
  final RustWorkbench backend;
  final _Script script;
  final Future<void> _server;
  final void Function() _stop;

  static Future<_Child> open(String python, _Script script) async {
    final directory = await Directory.systemTemp.createTemp(
      'morrow-external-service-business-',
    );
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
  if not 0<size<=131072: raise ValueError('request budget')
  request=exact(size)
  temporary=root/('request.%d.tmp'%index)
  temporary.write_bytes(request)
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
 time.sleep(.15)
 (root/'drain-complete').write_text('joined')
except BaseException:
 (root/'child-error').write_text(traceback.format_exc())
 raise
''');
    var stopped = false;
    Object? serverError;
    Future<void> serve() async {
      requests:
      for (var index = 0; !stopped; index++) {
        final request = File('${directory.path}/request.$index.bin');
        while (!await request.exists()) {
          if (stopped) break requests;
          await Future<void>.delayed(const Duration(milliseconds: 2));
        }
        late Uint8List bytes;
        try {
          final reader = RustWorkbench.readMessage(
            await request.readAsBytes(),
          ).getRoot(host.requestFactory);
          bytes = await script.respond(reader);
        } catch (error) {
          serverError ??= error;
          bytes = _reply((r) => r.error = 'controlled child rejected request');
        }
        final temporary = File('${directory.path}/reply.$index.tmp');
        await temporary.writeAsBytes(bytes);
        await temporary.rename('${directory.path}/reply.$index.bin');
      }
      if (serverError != null) throw serverError!;
    }

    final server = serve();
    try {
      final backend = await RustWorkbench.open(
        executable: python,
        package: 'controlled',
        directory: directory,
      );
      return _Child(directory, backend, script, server, () => stopped = true);
    } catch (_) {
      stopped = true;
      await server;
      await removeTestDirectory(directory);
      rethrow;
    }
  }

  Future<void> waitAction(host.Action action) async {
    final deadline = DateTime.now().add(const Duration(seconds: 10));
    while (!script.actions.contains(action)) {
      if (DateTime.now().isAfter(deadline)) {
        fail('did not receive $action: ${script.actions}');
      }
      await Future<void>.delayed(const Duration(milliseconds: 2));
    }
  }

  Future<void> dispose() async {
    try {
      try {
        await backend.close();
      } catch (error) {
        final detail = File('${directory.path}/child-error');
        if (await detail.exists()) {
          throw StateError('$error: ${await detail.readAsString()}');
        }
        rethrow;
      }
    } finally {
      _stop();
      await _server;
      await removeTestDirectory(directory);
    }
  }
}

void main() {
  final python = Platform.environment['MORROW_CLOSE_TEST_PYTHON'];
  for (final mode in [0, 1, 2]) {
    final badOffset = mode == 1;
    final unknownFinish = mode == 2;
    test(
      'large service frame is ordered and ${badOffset
          ? "aborted on bad acknowledgement"
          : unknownFinish
          ? "unknown completion is not retried"
          : "executed once"}',
      () async {
        final body = Uint8List.fromList(List.generate(80000, (i) => i % 251));
        final staged = <int>[];
        String? token;
        var expectedLength = 0;
        var executed = 0;
        var aborted = false;
        Uint8List? finishSubmission;
        late final _Script script;
        script = _Script((r) {
          switch (r.action!) {
            case host.Action.commandFrameBegin:
              token = r.transfer;
              expectedLength = r.totalLength;
              expect(expectedLength, greaterThan(65536));
              expect(r.sha256!.length, 32);
              return _reply((out) => out.offset = 0);
            case host.Action.commandFrameAppend:
              expect(r.transfer, token);
              expect(r.offset, staged.length);
              expect(r.payload!.length, lessThanOrEqualTo(32768));
              staged.addAll(r.payload!);
              return _reply(
                (out) => out.offset = staged.length + (badOffset ? 1 : 0),
              );
            case host.Action.commandFrameFinish:
              expect(r.transfer, token);
              expect(staged.length, expectedLength);
              final inner = RustWorkbench.readMessage(
                Uint8List.fromList(staged),
              ).getRoot(host.requestFactory);
              expect(inner.action, host.Action.service);
              expect(inner.payload, body);
              executed++;
              finishSubmission = Uint8List.fromList(script.submission!);
              script.unknown = unknownFinish;
              return _reply(
                (out) => out.payload = Uint8List.fromList([1, 2, 3]),
              );
            case host.Action.commandFrameAbort:
              expect(r.transfer, token);
              aborted = true;
              staged.clear();
              return _reply((_) {});
            default:
              throw StateError('unexpected inner action ${r.action}');
          }
        });
        final child = await _Child.open(python!, script);
        try {
          await child.backend.startServiceRun(_start());
          if (badOffset) {
            await expectLater(
              child.backend.service(body),
              throwsFormatException,
            );
            final deadline = DateTime.now().add(const Duration(seconds: 5));
            while (!aborted && DateTime.now().isBefore(deadline)) {
              await Future<void>.delayed(const Duration(milliseconds: 10));
            }
            expect(aborted, isTrue);
            expect(executed, 0);
            expect(
              script.commands.where(
                (r) => r.action == host.Action.commandFrameFinish,
              ),
              isEmpty,
            );
          } else if (unknownFinish) {
            try {
              await child.backend.service(body);
              fail('unknown completion was delivered');
            } on ServiceCommandFailure catch (error) {
              expect(error.outcomeUnknown, isTrue);
              expect(error.submission, finishSubmission);
              expect(
                error.submission
                    .map((b) => b.toRadixString(16).padLeft(2, '0'))
                    .join(),
                token,
              );
            }
            final until = DateTime.now().add(const Duration(seconds: 5));
            while (!aborted && DateTime.now().isBefore(until)) {
              await Future<void>.delayed(const Duration(milliseconds: 10));
            }
            expect(aborted, isTrue);
            expect(executed, 1);
            expect(
              script.commands
                  .where((r) => r.action == host.Action.commandFrameFinish)
                  .length,
              1,
            );
          } else {
            expect(await child.backend.service(body), [1, 2, 3]);
            expect(executed, 1);
            expect(script.commands.map((r) => r.action), [
              host.Action.commandFrameBegin,
              host.Action.commandFrameAppend,
              host.Action.commandFrameAppend,
              host.Action.commandFrameAppend,
              host.Action.commandFrameFinish,
            ]);
          }
        } finally {
          await child.dispose();
        }
      },
      skip: python == null,
    );
  }
  test(
    'queued business observes start admission and scheduler passes pending',
    () async {
      final script = _Script((request) {
        expect(request.action, host.Action.readUiLocale);
        return _locale('en');
      })..ready = false;
      script.startGate = Completer<void>();
      final child = await _Child.open(python!, script);
      try {
        final start = child.backend.startServiceRun(_start());
        await child.waitAction(host.Action.serviceRunStart);
        final business = child.backend.readUiLocale();
        script.startGate!.complete();
        await start;
        await child.waitAction(host.Action.commandStatus);
        expect(child.backend.writable, isTrue);
        expect(child.backend.maintenanceWarning, 'original owner');
        await child.backend.ioStatus();
        expect(child.backend.writable, isTrue);
        expect(child.backend.maintenanceWarning, 'original owner');
        await child.backend.cancelIo(_task);
        expect(
          script.actions,
          containsAllInOrder([
            host.Action.serviceRunStart,
            host.Action.commandSubmit,
            host.Action.commandStatus,
            host.Action.ioStatus,
            host.Action.ioCancel,
          ]),
        );
        expect(script.actions, isNot(contains(host.Action.commandRead)));
        script.ready = true;
        expect(await business, 'en');
        expect(script.commands.length, 1);
        expect(
          script.actions.where((a) => a == host.Action.commandRead).length,
          1,
        );
        expect(script.actions, isNot(contains(host.Action.readUiLocale)));
      } finally {
        if (!script.startGate!.isCompleted) script.startGate!.complete();
        await child.dispose();
      }
    },
    skip: python == null,
  );

  test(
    'observed service routes original readers errors and presentation',
    () async {
      final script = _Script(
        (request) => switch (request.action!) {
          host.Action.pluginState => _reply(
            (r) {
              r.sha256 = _digest;
              r.revision = 29;
              r.pluginEnabled = true;
              r.pluginApproved = true;
              r.pluginAvailable = true;
            },
            readOnly: true,
            warning: 'inner maintenance',
          ),
          host.Action.query => _reply(
            (r) {
              r.error = 'original query terminal';
              r.uiCode = 101;
            },
            readOnly: true,
            warning: 'query maintenance',
          ),
          host.Action.readUiLocale => _reply(
            (r) => r.error = 'original business error',
          ),
          _ => throw StateError('unexpected inner ${request.action}'),
        },
      );
      final child = await _Child.open(python!, script);
      try {
        await child.backend.serviceRunStatus();
        final state = await child.backend.pluginState();
        expect(state.digest, _digest);
        expect(state.revision, BigInt.from(29));
        expect(state.enabled && state.approved && state.available, isTrue);
        expect(state.writable, isFalse);
        expect(child.backend.writable, isFalse);
        expect(child.backend.maintenanceWarning, 'inner maintenance');
        await expectLater(
          child.backend.query(
            'all',
            '',
            'query text',
            'newest',
            operation: 'same-op',
          ),
          throwsA(
            isA<QueryFailure>()
                .having((e) => e.terminal, 'terminal', true)
                .having((e) => e.capacity, 'capacity', true)
                .having((e) => e.message, 'message', 'original query terminal'),
          ),
        );
        expect(child.backend.maintenanceWarning, 'query maintenance');
        await expectLater(
          child.backend.readUiLocale(),
          throwsA(
            isA<StateError>().having(
              (e) => e.message,
              'original error',
              'original business error',
            ),
          ),
        );
        expect(
          state.digest,
          _digest,
          reason: 'owned model survives later reply wiping',
        );
        expect(script.commands.map((r) => r.action), [
          host.Action.pluginState,
          host.Action.query,
          host.Action.readUiLocale,
        ]);
        expect(script.commands[1].operation, 'same-op');
      } finally {
        await child.dispose();
      }
    },
    skip: python == null,
  );

  test(
    'unknown command is not retried and oversized ordinary body is unsent',
    () async {
      final body = Uint8List.fromList(utf8.encode('one-time-business-body'));
      final script = _Script((request) {
        expect(request.action, host.Action.service);
        expect(request.payload, body);
        return _reply((r) => r.payload = body);
      })..unknown = true;
      final child = await _Child.open(python!, script);
      try {
        await child.backend.startServiceRun(_start());
        try {
          await child.backend.service(body);
          fail('unknown command delivered a successful result');
        } on ServiceCommandFailure catch (error) {
          expect(error.outcomeUnknown, isTrue);
          expect(error.terminal, OwnerCommandTerminal.unknown);
          expect(error.task, _task);
          expect(error.command, _command);
          expect(error.submission, script.submission);
        }
        final before = List<host.Action>.of(script.actions);
        await Future<void>.delayed(const Duration(milliseconds: 80));
        expect(script.actions, before);
        expect(script.commands.length, 1);
        expect(
          script.actions.where((a) => a == host.Action.commandRead).length,
          1,
        );
        await expectLater(
          child.backend.service(Uint8List(128 * 1024)),
          throwsFormatException,
        );
        expect(
          script.actions,
          before,
          reason: 'outer business frame exceeds 128 KiB',
        );
      } finally {
        await child.dispose();
      }
    },
    skip: python == null,
  );

  test(
    'malformed consumed reply retains original identities without another submission',
    () async {
      final script = _Script((request) {
        expect(request.action, host.Action.readUiLocale);
        return Uint8List(8);
      });
      final child = await _Child.open(python!, script);
      try {
        await child.backend.startServiceRun(_start());
        try {
          await child.backend.readUiLocale();
          fail('malformed consumed frame returned a business value');
        } on ServiceCommandFailure catch (error) {
          expect(error.outcomeUnknown, isTrue);
          expect(error.task, _task);
          expect(error.command, _command);
          expect(error.submission, script.submission);
        }
        expect(child.backend.writable, isTrue);
        expect(child.backend.maintenanceWarning, 'original owner');
        final before = List<host.Action>.of(script.actions);
        await Future<void>.delayed(const Duration(milliseconds: 80));
        expect(script.actions, before);
        expect(script.commands.length, 1);
        expect(
          script.actions.where((a) => a == host.Action.commandRead).length,
          1,
        );
      } finally {
        await child.dispose();
      }
    },
    skip: python == null,
  );

  test(
    'typed issued authentication owns token after routed reply disposal',
    () async {
      final script = _Script((request) {
        if (request.action == host.Action.readUiLocale) return _locale('en');
        expect(request.action, host.Action.serviceAuthenticationIssue);
        expect(request.serviceReference, _digest);
        expect(request.revision, 1);
        expect(request.principalId, 'alice');
        expect(request.serviceDays, 1);
        return _reply((r) {
          final authority = r.initServiceAuthorities(1)[0];
          authority.reference = _digest;
          authority.revision = 2;
          authority.createdMs = 10;
          authority.expiresMs = 1000;
          authority.kind = 1;
          authority.principalId = 'alice';
          r.issuedToken = Uint8List.fromList(utf8.encode('a' * 64));
        });
      });
      final child = await _Child.open(python!, script);
      try {
        await child.backend.startServiceRun(_start());
        final issued = await child.backend.issueServiceAuthentication(
          reference: _digest,
          expectedRevision: BigInt.one,
          principalId: 'alice',
          lifetimeDays: 1,
        );
        final tokenView = Uint8List.sublistView(issued.token.bytes);
        try {
          expect(issued.authority.reference, _digest);
          expect(issued.authority.revision, BigInt.two);
          expect(tokenView, everyElement(97));
          expect(tokenView.length, 64);
          expect(issued.token.toString(), isNot(contains('a' * 64)));
          expect(await child.backend.readUiLocale(), 'en');
          expect(tokenView, everyElement(97));
          expect(script.commands.map((r) => r.action), [
            host.Action.serviceAuthenticationIssue,
            host.Action.readUiLocale,
          ]);
        } finally {
          issued.dispose();
        }
        expect(tokenView, everyElement(0));
        expect(issued.token.isDisposed, isTrue);
      } finally {
        await child.dispose();
      }
    },
    skip: python == null,
  );

  test(
    'close cancels pending routing polls and waits for actual child EOF drain',
    () async {
      final script = _Script((_) => _locale('en'))..ready = false;
      final child = await _Child.open(python!, script);
      try {
        await child.backend.startServiceRun(_start());
        final business = expectLater(
          child.backend.readUiLocale(),
          throwsA(anything),
        );
        await child.waitAction(host.Action.commandStatus);
        await child.backend.close();
        await business;
        expect(
          await File('${child.directory.path}/eof-observed').exists(),
          isTrue,
        );
        expect(
          await File('${child.directory.path}/drain-complete').readAsString(),
          'joined',
        );
        final count = script.actions.length;
        await Future<void>.delayed(const Duration(milliseconds: 80));
        expect(script.actions.length, count);
        expect(script.actions, isNot(contains(host.Action.commandRead)));
        expect(script.commands.length, 1);
        await expectLater(child.backend.readUiLocale(), throwsStateError);
        expect(script.actions.length, count);
      } finally {
        await child.dispose();
      }
    },
    skip: python == null,
  );
}
