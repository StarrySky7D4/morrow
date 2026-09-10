import 'dart:convert';
import 'dart:typed_data';
import 'package:capnproto_dart/capnproto_dart.dart';
import 'generated/runtime.capnp.dart';
import 'generated/contract_identity.dart' as contract;

const maxMessageBytes = 64 * 1024;
final _maxRevision = (BigInt.one << 64) - BigInt.one;
void _identity(String text) {
  if (text.isEmpty ||
      utf8.encode(text).length > 256 ||
      RegExp(r'[\x00-\x1f\x7f-\x9f/\\:]').hasMatch(text)) {
    throw const FormatException('Invalid identity');
  }
}

bool _sameBytes(List<int>? a, List<int> b) {
  if (a == null || a.length != b.length) return false;
  for (var i = 0; i < b.length; i++) {
    if (a[i] != b[i]) return false;
  }
  return true;
}

void _frame(Uint8List bytes) {
  if (bytes.length < 8 || bytes.length > maxMessageBytes)
    throw const FormatException('Message length');
  final data = ByteData.sublistView(bytes);
  final count = data.getUint32(0, Endian.little) + 1;
  if (count > 512) throw const FormatException('Segment limit');
  final header = ((count + 2) ~/ 2) * 8;
  if (header > bytes.length) throw const FormatException('Truncated header');
  var size = header;
  for (var i = 0; i < count; i++) {
    final words = data.getUint32(4 + i * 4, Endian.little);
    if (words > maxMessageBytes ~/ 8)
      throw const FormatException('Segment length');
    size += words * 8;
    if (size > bytes.length) throw const FormatException('Truncated segment');
  }
  if (size != bytes.length) throw const FormatException('Trailing bytes');
}

/// Revisions remain unsigned BigInt in both native Dart and Dart/Wasm.
/// JSON, doubles, and lossy JavaScript Number conversions are never used.
class RenameCommand {
  RenameCommand({
    required this.operationId,
    required this.cardId,
    required this.expectedRevision,
    required this.title,
  }) {
    validate();
  }
  final String operationId, cardId, title;
  final BigInt expectedRevision;
  void validate() {
    _identity(operationId);
    _identity(cardId);
    if (expectedRevision <= BigInt.zero ||
        expectedRevision > _maxRevision ||
        utf8.encode(title).length > 16 * 1024)
      throw const FormatException('Revision or title limit');
  }

  Uint8List encode() {
    validate();
    final builder = MessageBuilder();
    final root = builder.initRoot(requestFactory);
    root.protocolVersion = contract.protocolVersion;
    root.runtimeDigest = Uint8List.fromList(contract.runtimeDigest);
    root.contentDigest = Uint8List.fromList(contract.contentDigest);
    root.operationId = operationId;
    final rename = root.initRenameCard();
    rename.cardId = cardId;
    // Preserve all 64 bits through Dart's signed native integer representation.
    rename.expectedRevision = expectedRevision.toSigned(64).toInt();
    rename.title = title;
    final bytes = builder.serialize();
    _frame(bytes);
    return bytes;
  }

  static RenameCommand decode(Uint8List bytes) {
    _frame(bytes);
    final root = MessageReader.deserialize(
      bytes,
      const MessageReaderOptions(
        traversalLimitInWords: 8192,
        nestingLimit: 16,
        maxSegments: 512,
      ),
    ).getRoot(requestFactory);
    if (root.protocolVersion != contract.protocolVersion ||
        !_sameBytes(root.runtimeDigest, contract.runtimeDigest) ||
        !_sameBytes(root.contentDigest, contract.contentDigest))
      throw const FormatException('Contract mismatch');
    if (root.which != 1) throw const FormatException('Unsupported operation');
    final rename = root.renameCard;
    if (rename == null) throw const FormatException('Missing rename');
    return RenameCommand(
      operationId: root.operationId ?? '',
      cardId: rename.cardId ?? '',
      expectedRevision: BigInt.from(rename.expectedRevision).toUnsigned(64),
      title: rename.title ?? '',
    );
  }
}
