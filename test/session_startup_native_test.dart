import 'dart:io';
import 'dart:typed_data';
import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/generated/host.capnp.dart' as host;
import 'package:morrow_studio/plugins/generated/identity.dart' as contract;
import 'package:morrow_studio/plugins/workbench_native.dart';
import 'package:morrow_studio/plugins/session_coordinator.dart';

void main() {
  final python = Platform.environment['MORROW_CLOSE_TEST_PYTHON'];
  test(
    'startup publishes original failure while independently supervised child still holds its exit gate',
    () async {
      final directory = await Directory.systemTemp.createTemp(
        'morrow-session-start-',
      );
      final session = SessionCoordinator(
        interactionDeadline: const Duration(milliseconds: 20),
      );
      Process? process;
      try {
        final message = MessageBuilder();
        final r = message.initRoot(host.responseFactory);
        r.version = 1;
        r.digest = Uint8List.fromList(contract.hostDigest);
        r.error = 'original library failure';
        final payload = message.serialize();
        await File('${directory.path}/reply.bin').writeAsBytes([
          ...(ByteData(
            4,
          )..setUint32(0, payload.length, Endian.little)).buffer.asUint8List(),
          ...payload,
        ]);
        await File('${directory.path}/workbench.db').writeAsString('''
import pathlib,struct,sys,time
root=pathlib.Path(__file__).parent
n=struct.unpack('<I',sys.stdin.buffer.read(4))[0]
sys.stdin.buffer.read(n)
sys.stdout.buffer.write((root/'reply.bin').read_bytes());sys.stdout.buffer.flush()
sys.stdin.buffer.read()
deadline=time.monotonic()+20
while not (root/'release').exists() and time.monotonic()<deadline: time.sleep(.01)
''');
        await session
            .run(() async {
              await expectLater(
                RustWorkbench.open(
                  executable: python!,
                  package: 'fixture',
                  directory: directory,
                  onStarted: (backend) {
                    process = backend.process;
                    session.attach(
                      process: backend.process,
                      library: directory.path,
                      exited: backend.process.exitCode,
                      close: backend.close,
                    );
                  },
                ),
                throwsA(
                  isA<StateError>().having(
                    (e) => e.message,
                    'original error',
                    'original library failure',
                  ),
                ),
              );
            })
            .timeout(const Duration(seconds: 5));
        session.close();
        await Future<void>.delayed(const Duration(milliseconds: 40));
        expect(session.phase, SessionPhase.closingUnconfirmed);
        expect(session.mayRecover, isFalse);
        await expectLater(session.run(() async {}), throwsStateError);
        await File(
          '${directory.path}/release',
        ).writeAsString('supervisor release');
        expect(await process!.exitCode.timeout(const Duration(seconds: 5)), 0);
        await session.close();
        expect(session.mayRecover, isTrue);
      } finally {
        await File('${directory.path}/release').writeAsString('cleanup');
        if (process != null) {
          await process!.exitCode.timeout(const Duration(seconds: 25));
        }
        session.dispose();
        await directory.delete(recursive: true);
      }
    },
    skip: python == null || !Platform.isWindows,
    timeout: const Timeout(Duration(seconds: 35)),
  );
}
