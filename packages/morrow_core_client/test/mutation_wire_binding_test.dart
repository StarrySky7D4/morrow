import 'dart:io';
import 'dart:typed_data';
import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:test/test.dart';
import '../../../lib/plugins/generated/host.capnp.dart' as host;
import '../../../lib/plugins/generated/identity.dart' as identity;

void main() {
  test(
    'mutation fields preserve uint64 command and offset without narrowing',
    () {
      final message = MessageBuilder();
      final request = message.initRoot(host.requestFactory);
      request.version = 1;
      request.digest = Uint8List.fromList(identity.hostDigest);
      request.action = host.Action.mutationSubmit;
      request.mutationCommandIdBigInt = BigInt.parse('18446744073709551615');
      final command = request.initMutationCommand();
      command.kind = 2;
      command.offsetBigInt = BigInt.parse('9007199254740993');
      command.submission = Uint8List.fromList(List.filled(32, 9));
      command.bytes = Uint8List.fromList([0, 128, 255]);
      final frame = message.serialize();
      final decoded = MessageReader.deserialize(
        frame,
      ).getRoot(host.requestFactory);
      expect(decoded.action, host.Action.mutationSubmit);
      expect(
        decoded.mutationCommandIdBigInt,
        BigInt.parse('18446744073709551615'),
      );
      expect(
        decoded.mutationCommand!.offsetBigInt,
        BigInt.parse('9007199254740993'),
      );
      expect(decoded.mutationCommand!.bytes, [0, 128, 255]);
    },
  );
  final fixture = Platform.environment['MORROW_MUTATION_WIRE_FIXTURES'];
  test(
    'generated Dart binding decodes original Rust mutation result frames',
    () {
      final files = Directory(fixture!)
          .listSync()
          .whereType<File>()
          .where((file) => file.path.endsWith('.bin'))
          .toList();
      final kinds = <int>{};
      for (final file in files) {
        final frame = file.readAsBytesSync();
        expect(frame.length, lessThanOrEqualTo(128 * 1024));
        final response = MessageReader.deserialize(
          frame,
        ).getRoot(host.responseFactory);
        expect(response.version, 1);
        expect(response.digest, identity.hostDigest);
        expect(response.error ?? '', isEmpty);
        expect(response.ioState!.key!.length, 32);
        final result = response.mutationResult!;
        kinds.add(result.kind);
        if (result.kind == 1) {
          expect(result.reference!.length, 32);
          expect(response.mutationState!.selected, isTrue);
        }
        if (result.kind == 2 || result.kind == 6) {
          expect(result.record, isNotEmpty);
          expect(result.operationId, 'wire-create');
          expect(result.phase, result.kind == 2 ? 1 : 3);
          expect(result.effect, 0);
        }
        if (result.kind == 11) {
          expect(response.mutationState!.kind, 9);
          expect(result.plan, isNotEmpty);
          expect(result.phase, 0);
          expect(result.effect, 0);
          expect(result.record ?? [], isEmpty);
          expect(result.outcome ?? [], isEmpty);
        }
        if (result.kind == 12) {
          expect(response.mutationState!.kind, 10);
          expect(response.mutationState!.selected, isFalse);
          expect(response.mutationState!.terminal, result.done);
          expect(result.plans!.length, inInclusiveRange(1, 8));
          expect(result.scanned, inInclusiveRange(result.plans!.length, 8));
          expect(result.checkpoint?.length ?? 0, result.done ? 0 : 32);
          expect(result.phase, 0);
          expect(result.effect, 0);
          expect(result.record ?? [], isEmpty);
          expect(result.outcome ?? [], isEmpty);
        }
        if (result.kind == 4) {
          expect(result.phase, 3);
          expect(result.operationId, 'wire-create');
          expect(result.effect, 1);
          expect(result.osCode, 0);
          expect(result.outcome, isNotEmpty);
        }
      }
      expect(kinds, containsAll([1, 2, 4, 6, 10, 11, 12]));
    },
    skip: fixture == null ? 'requires real Rust mutation fixtures' : false,
  );
}
