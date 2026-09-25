import 'dart:typed_data';
import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/editor_commit_proof_codec.dart';
import 'package:morrow_studio/plugins/generated/host.capnp.dart' as wire;
import 'package:morrow_studio/plugins/versioned_content_codec.dart';

wire.ResponseReader reply({
  String id = 'card',
  String operation = 'original',
  BigInt? source,
  BigInt? committed,
  BigInt? outer,
  int digestLength = 32,
  bool present = true,
  bool payload = false,
}) {
  final message = MessageBuilder();
  final response = message.initRoot(wire.responseFactory);
  final revision = committed ?? (source ?? BigInt.zero) + BigInt.one;
  response.revisionBigInt = outer ?? revision;
  if (payload) response.payload = Uint8List.fromList([1]);
  if (present) {
    final proof = response.initEditorCommitProof();
    proof.id = id;
    proof.operation = operation;
    proof.digest = Uint8List.fromList(List.filled(digestLength, 73));
    proof.sourceRevisionBigInt = source ?? BigInt.zero;
    proof.committedRevisionBigInt = revision;
  }
  return MessageReader.deserialize(
    message.serialize(),
  ).getRoot(wire.responseFactory);
}

void main() {
  test('first Create and large unsigned historical revisions stay exact', () {
    for (final source in [
      BigInt.zero,
      (BigInt.one << 63) - BigInt.one,
      VersionedContentCodec.maxU64 - BigInt.one,
    ]) {
      final proof = EditorCommitProofCodec.decode(
        reply(source: source),
        id: 'card',
        operation: 'original',
      );
      expect(proof.sourceRevision, source);
      expect(proof.committedRevision, source + BigInt.one);
      expect(proof.digest, List.filled(32, 73));
      expect(() => proof.digest[0] = 0, throwsUnsupportedError);
    }
  });
  test(
    'absent, mismatched, malformed and nonconsecutive receipts are rejected',
    () {
      final malformed = [
        reply(present: false),
        reply(id: 'other'),
        reply(operation: 'other'),
        reply(digestLength: 0),
        reply(digestLength: 31),
        reply(digestLength: 33),
        reply(committed: BigInt.two),
        reply(outer: BigInt.two),
        reply(payload: true),
        reply(source: VersionedContentCodec.maxU64, committed: BigInt.zero),
      ];
      for (final value in malformed) {
        expect(
          () => EditorCommitProofCodec.decode(
            value,
            id: 'card',
            operation: 'original',
          ),
          throwsFormatException,
        );
      }
    },
  );
  test('invalid request identities fail locally', () {
    for (final invalid in ['', 'other/card', 'bad:operation', 'x' * 257]) {
      expect(
        () => EditorCommitProofCodec.validateIdentity(invalid, 'original'),
        throwsFormatException,
      );
      expect(
        () => EditorCommitProofCodec.validateIdentity('card', invalid),
        throwsFormatException,
      );
    }
  });
}
