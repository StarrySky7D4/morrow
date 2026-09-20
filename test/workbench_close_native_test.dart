import 'dart:io';
import 'dart:typed_data';
import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/generated/host.capnp.dart' as host;
import 'package:morrow_studio/plugins/generated/identity.dart' as contract;
import 'package:morrow_studio/plugins/workbench_native.dart';
import 'external_plugin_native_test.dart' show removeTestDirectory;

void main() {
  final python = Platform.environment['MORROW_CLOSE_TEST_PYTHON'];
  for (final serviceRequest in [false, true]) {
    test(
      'closed input fences ${serviceRequest ? "service" : "ordinary"} requests without unhandled reply errors',
      () async {
        final dir = await Directory.systemTemp.createTemp(
          'morrow-external-close-',
        );
        RustWorkbench? backend;
        try {
          final message = MessageBuilder();
          final response = message.initRoot(host.responseFactory);
          response.version = 1;
          response.digest = Uint8List.fromList(contract.hostDigest);
          final bytes = message.serialize();
          final header = ByteData(4)..setUint32(0, bytes.length, Endian.little);
          final reply = File('${dir.path}/reply.bin');
          await reply.writeAsBytes([...header.buffer.asUint8List(), ...bytes]);
          await File('${dir.path}/workbench.db').writeAsString('''
import pathlib,struct,sys,time
def exact(n):
 data=b''
 while len(data)<n:
  part=sys.stdin.buffer.read(n-len(data))
  if not part: raise EOFError()
  data+=part
 return data
try:
 while True:
  size=struct.unpack('<I',exact(4))[0]
  exact(size)
  sys.stdout.buffer.write(pathlib.Path(sys.argv[1]).read_bytes())
  sys.stdout.buffer.flush()
except EOFError:
 time.sleep(1)
''');
          backend = await RustWorkbench.open(
            executable: python!,
            package: reply.path,
            directory: dir,
          );
          await backend.process.stdin.close();
          final attempted = serviceRequest
              ? backend.issueServiceAuthentication(
                  reference: Uint8List(0),
                  expectedRevision: BigInt.zero,
                  principalId: 'failure-test',
                  lifetimeDays: 1,
                )
              : backend.pluginPage();
          await expectLater(attempted, throwsStateError);
          await expectLater(
            backend.pluginPage(),
            throwsA(
              isA<StateError>().having(
                (error) => error.message,
                'fenced channel',
                'Content service transport outcome is unknown',
              ),
            ),
          );
          // The test zone also fails on a second, unhandled pending-reply error.
          await Future<void>.delayed(const Duration(milliseconds: 25));
        } finally {
          await backend?.close();
          await removeTestDirectory(dir);
        }
      },
      skip: python == null,
    );
  }
  test(
    'real child process close waits beyond five seconds and reports nonzero exit',
    () async {
      for (final exitCode in [0, 7]) {
        final dir = await Directory.systemTemp.createTemp(
          'morrow-external-close-',
        );
        RustWorkbench? backend;
        try {
          final message = MessageBuilder();
          final response = message.initRoot(host.responseFactory);
          response.version = 1;
          response.digest = Uint8List.fromList(contract.hostDigest);
          final bytes = message.serialize();
          final header = ByteData(4)..setUint32(0, bytes.length, Endian.little);
          final reply = File('${dir.path}/reply.bin');
          await reply.writeAsBytes([...header.buffer.asUint8List(), ...bytes]);
          // A controlled protocol process, not a fake proof of Storage recovery.
          // open invokes the executable with workbench.db as its first argument.
          await File('${dir.path}/workbench.db').writeAsString('''
import pathlib,struct,sys,time
def exact(n):
 data=b''
 while len(data)<n:
  part=sys.stdin.buffer.read(n-len(data))
  if not part: raise EOFError()
  data+=part
 return data
try:
 while True:
  size=struct.unpack('<I',exact(4))[0]
  exact(size)
  sys.stdout.buffer.write(pathlib.Path(sys.argv[1]).read_bytes())
  sys.stdout.buffer.flush()
except EOFError:
 time.sleep(${exitCode == 0 ? 6 : 0})
 pathlib.Path(__file__+'.exited').write_text('joined')
 sys.exit($exitCode)
''');
          backend = await RustWorkbench.open(
            executable: python!,
            package: reply.path,
            directory: dir,
          );
          var completed = false;
          final closing = backend.close();
          expect(identical(closing, backend.close()), isTrue);
          await expectLater(backend.ioStatus(), throwsStateError);
          if (exitCode == 0) {
            final observed = closing.then((_) {
              completed = true;
            });
            await Future<void>.delayed(const Duration(milliseconds: 5300));
            expect(completed, isFalse);
            await observed.timeout(const Duration(seconds: 5));
            expect(await backend.process.exitCode, 0);
          } else {
            await expectLater(closing, throwsStateError);
            expect(await backend.process.exitCode, 7);
          }
          expect(
            await File('${dir.path}/workbench.db.exited').readAsString(),
            'joined',
          );
        } finally {
          if (backend != null) {
            try {
              await backend.close();
            } catch (_) {
              /* Expected exit-7 case above. */
            }
          }
          await removeTestDirectory(dir);
        }
      }
    },
    skip: python == null,
    timeout: const Timeout(Duration(seconds: 30)),
  );
}
