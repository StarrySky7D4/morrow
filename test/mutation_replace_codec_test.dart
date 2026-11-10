import 'dart:typed_data';
import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/generated/host.capnp.dart' as wire;
import 'package:morrow_studio/plugins/mutation_task_codec_native.dart';
import 'package:morrow_studio/plugins/mutation_task_models.dart';

MutationResult? replacement({
  void Function(wire.MutationResultBuilder)? alter,
}) {
  final message = MessageBuilder();
  final row = message.initRoot(wire.mutationResultFactory);
  row.kind = 13;
  row.phase = 3;
  row.effect = 1;
  row.operationId = 'replace-codec';
  row.outcome = Uint8List.fromList([1, 2, 3]);
  alter?.call(row);
  return MutationTaskCodec.result(
    MessageReader.deserialize(
      message.serialize(),
    ).getRoot(wire.mutationResultFactory),
  );
}

void main() {
  test(
    'replacement wire result preserves success and explicit OS rejection',
    () {
      final success = replacement()!;
      expect(success.kind, MutationResultKind.replaced);
      expect(success.phase, MutationPhase.observed);
      expect(success.effect, MutationEffect.osSucceeded);
      expect(success.operationId, 'replace-codec');
      expect(success.outcome, [1, 2, 3]);
      final rejected = replacement(
        alter: (row) {
          row.effect = 2;
          row.osCode = 5;
        },
      )!;
      expect(rejected.kind, MutationResultKind.replaced);
      expect(rejected.effect, MutationEffect.osRejected);
      expect(rejected.osCode, 5);
    },
  );
  test(
    'replacement decoder refuses incomplete or contradictory effect evidence',
    () {
      for (final alter in <void Function(wire.MutationResultBuilder)>[
        (row) => row.phase = 2,
        (row) => row.outcome = Uint8List(0),
        (row) => row.operationId = '',
        (row) => row.effect = 0,
        (row) => row.effect = 2,
        (row) => row.osCode = 5,
        (row) => row.kind = 14,
      ]) {
        expect(() => replacement(alter: alter), throwsFormatException);
      }
    },
  );
}
