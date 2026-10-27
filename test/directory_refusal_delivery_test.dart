import 'dart:typed_data';
import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/generated/host.capnp.dart' as wire;
import 'package:morrow_studio/plugins/directory_task_models.dart';
import 'package:morrow_studio/plugins/directory_task_native.dart';
import 'package:morrow_studio/plugins/directory_task_session.dart';
import 'package:morrow_studio/plugins/io_task_models.dart';
import 'directory_task_test_support.dart';

wire.DirectoryResultReader refusal(
  int fault, {
  void Function(wire.DirectoryResultBuilder)? alter,
}) {
  final message = MessageBuilder();
  final row = message.initRoot(wire.directoryResultFactory);
  row.kind = 4;
  row.commandFault = fault;
  alter?.call(row);
  return MessageReader.deserialize(
    message.serialize(),
  ).getRoot(wire.directoryResultFactory);
}

class RefusalBackend extends DirectoryFake {
  RefusalBackend(this.fault, {this.loseCapture = false});
  final DirectoryCommandFault fault;
  final bool loseCapture;
  @override
  Future<DirectoryTaskRead> readDirectory(Uint8List key) async {
    reads++;
    state = directoryState(
      request,
      storage: IoStoragePhase.stopping,
      delivery: IoDeliveryPhase.consumed,
    );
    if (loseCapture) throw StateError('original reply lost after consumption');
    return DirectoryTaskRead(
      snapshot: state,
      result: DirectoryTaskRefused(fault),
    );
  }
}

void main() {
  test(
    'definitive refusal crosses an actual Capnp serialization unchanged',
    () {
      for (var code = 1; code <= 3; code++) {
        final decoded =
            NativeDirectoryTaskClient.decodeResult(refusal(code), guest: false)
                as DirectoryTaskRefused;
        expect(decoded.fault, DirectoryCommandFault.values[code - 1]);
        expect(
          () => NativeDirectoryTaskClient.decodeResult(
            refusal(code),
            guest: true,
          ),
          throwsFormatException,
        );
      }
    },
  );

  test(
    'refusal cannot carry success, guest bytes, unknown or extra metadata',
    () {
      for (final alter in <void Function(wire.DirectoryResultBuilder)>[
        (r) => r.entries = 1,
        (r) => r.metadataBytesBigInt = BigInt.one,
        (r) => r.frame = Uint8List.fromList([1]),
        (r) => r.unknown = true,
        (r) => r.executionFault = 8,
        (r) => r.exitCode = 1,
        (r) => r.hostCalls = 1,
        (r) => r.kind = 0,
        (r) => r.kind = 1,
        (r) => r.kind = 2,
        (r) => r.kind = 3,
        (r) => r.kind = 5,
      ]) {
        expect(
          () => NativeDirectoryTaskClient.decodeResult(
            refusal(1, alter: alter),
            guest: false,
          ),
          throwsFormatException,
        );
      }
      for (final code in [0, 4, 65535]) {
        expect(
          () => NativeDirectoryTaskClient.decodeResult(
            refusal(code),
            guest: false,
          ),
          throwsFormatException,
        );
      }
    },
  );

  for (final fault in DirectoryCommandFault.values) {
    test(
      'delivered $fault stops before guest but waits for original reclamation',
      () async {
        final b = RefusalBackend(fault);
        final s = DirectoryTaskSession.forBackend(b, b);
        await s.refresh();
        await s.start(directoryRequest());
        await s.execute();
        final expected = fault == DirectoryCommandFault.cancelled
            ? DirectoryTaskPhase.cancelled
            : DirectoryTaskPhase.failed;
        expect(s.phase, expected);
        expect(
          [b.starts, b.reads, b.runs, b.guestReads, b.cancels],
          [1, 1, 0, 0, 0],
        );
        expect(s.result, isNull);
        expect(s.captured, isNull);
        expect(s.canStart, isFalse);
        expect(s.canAcknowledge, isFalse);
        await s.execute();
        await s.advance();
        expect([b.starts, b.reads, b.runs], [1, 1, 0]);
        b.state = directoryState(b.request, storage: IoStoragePhase.reclaimed);
        await s.refresh();
        expect(s.phase, expected);
        expect(s.canAcknowledge, isTrue);
        await s.acknowledge();
        expect(s.history.single.phase, expected);
        expect(s.canStart, isTrue);
        expect([b.acks, b.starts, b.reads, b.runs], [1, 1, 1, 0]);
      },
    );
  }

  test('lost capture delivery remains unknown and is never re-read', () async {
    final b = RefusalBackend(
      DirectoryCommandFault.cancelled,
      loseCapture: true,
    );
    final s = DirectoryTaskSession.forBackend(b, b);
    await s.refresh();
    await s.start(directoryRequest());
    await s.execute();
    expect(s.phase, DirectoryTaskPhase.unknown);
    expect(s.notice, DirectoryTaskNotice.readUnknown);
    await s.advance();
    await s.execute();
    expect([b.starts, b.reads, b.runs, b.guestReads], [1, 1, 0, 0]);
  });
}
