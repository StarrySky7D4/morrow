import 'dart:typed_data';
import 'package:crypto/crypto.dart';
import 'package:test/test.dart';
import 'package:morrow_core_client/attachment_transfer.dart';

void main() {
  final raw = Uint8List.fromList(List.generate(40000, (i) => i % 251));
  AttachmentPart part(
    int offset,
    int count, {
    Uint8List? bytes,
    Uint8List? digest,
    BigInt? revision,
  }) => AttachmentPart(
    cardId: 'card',
    attachmentId: 'file',
    revision: revision ?? BigInt.one,
    offset: BigInt.from(offset),
    totalLength: BigInt.from(raw.length),
    contentSha256: digest ?? Uint8List.fromList(sha256.convert(raw).bytes),
    bytes: bytes ?? Uint8List.sublistView(raw, offset, offset + count),
  );
  AttachmentTransferVerifier verifier() => AttachmentTransferVerifier(
    cardId: 'card',
    attachmentId: 'file',
    revision: BigInt.one,
  );
  test('publishing requires all ordered bytes and the whole-file digest', () {
    final check = verifier();
    check.add(part(0, 32768));
    expect(check.finish, throwsFormatException);
    check.add(part(32768, 7232));
    check.finish();
    expect(check.finish, throwsStateError);
  });
  test('mixed revisions and reordered data cannot advance the transfer', () {
    final check = verifier();
    expect(() => check.add(part(32768, 7232)), throwsFormatException);
    expect(
      () => check.add(part(0, 32768, revision: BigInt.two)),
      throwsFormatException,
    );
    expect(check.nextOffset, BigInt.zero);
    check.add(part(0, 32768));
    expect(
      () => check.add(part(32768, 7232, digest: Uint8List(32))),
      throwsFormatException,
    );
  });
  test('corrupted complete file fails final verification', () {
    final check = verifier();
    check.add(part(0, 32768, bytes: Uint8List(32768)));
    check.add(part(32768, 7232));
    expect(check.finish, throwsFormatException);
  });
  test('packets own immutable copies', () {
    final original = Uint8List.sublistView(raw, 0, 10);
    final value = part(0, 10, bytes: original);
    original[0] = 99;
    expect(value.bytes[0], 0);
    expect(() => value.bytes[0] = 1, throwsUnsupportedError);
    raw[0] = 0;
  });
  test('empty file still requires a verified packet', () {
    final check = verifier();
    expect(check.finish, throwsFormatException);
    check.add(
      AttachmentPart(
        cardId: 'card',
        attachmentId: 'file',
        revision: BigInt.one,
        offset: BigInt.zero,
        totalLength: BigInt.zero,
        contentSha256: Uint8List.fromList(sha256.convert([]).bytes),
        bytes: Uint8List(0),
      ),
    );
    check.finish();
  });
}
