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

class ReadSummaryCommand {
  ReadSummaryCommand({required this.requestId, required this.cardId}) {
    _identity(requestId);
    _identity(cardId);
  }
  final String requestId, cardId;
  Uint8List encode() => _query(requestId, (root) => root.readSummary = cardId);
}

class QueryOperationCommand {
  QueryOperationCommand({
    required this.requestId,
    required this.cardId,
    required this.operationId,
  }) {
    _identity(requestId);
    _identity(cardId);
    _identity(operationId);
  }
  final String requestId, cardId, operationId;
  Uint8List encode() => _query(requestId, (root) {
    final query = root.initQueryOperation();
    query.cardId = cardId;
    query.operationId = operationId;
  });
}

Uint8List _query(String id, void Function(RequestBuilder) fill) {
  final message = MessageBuilder();
  final root = message.initRoot(requestFactory);
  root.protocolVersion = contract.protocolVersion;
  root.runtimeDigest = Uint8List.fromList(contract.runtimeDigest);
  root.contentDigest = Uint8List.fromList(contract.contentDigest);
  root.operationId = id;
  fill(root);
  final bytes = message.serialize();
  _frame(bytes);
  return bytes;
}

class RuntimeReply {
  RuntimeReply._({
    required this.kind,
    this.revision,
    this.cardId,
    this.typeId,
    this.formatVersion,
    this.title,
    this.previewText,
    this.failure,
    this.operationId,
    this.eventId,
    this.contentSha256,
    this.resultState,
  });
  final String kind;
  final BigInt? revision;
  final String? cardId,
      typeId,
      title,
      previewText,
      failure,
      operationId,
      eventId,
      resultState;
  final int? formatVersion;
  final Uint8List? contentSha256;
  static RuntimeReply decode(Uint8List bytes, {required String requestId}) {
    _frame(bytes);
    _identity(requestId);
    final root = MessageReader.deserialize(
      bytes,
      const MessageReaderOptions(
        traversalLimitInWords: 8192,
        nestingLimit: 16,
        maxSegments: 512,
      ),
    ).getRoot(responseFactory);
    if (root.protocolVersion != contract.protocolVersion ||
        !_sameBytes(root.runtimeDigest, contract.runtimeDigest) ||
        !_sameBytes(root.contentDigest, contract.contentDigest))
      throw const FormatException('Contract mismatch');
    if (root.requestId != requestId)
      throw const FormatException('Response correlation mismatch');
    switch (root.which) {
      case 1:
        return _receipt(root.renamed, kind: 'renamed', operationId: requestId);
      case 2:
        final summary = root.summary;
        if (summary == null ||
            summary.protocolVersion != contract.protocolVersion)
          throw const FormatException('Summary contract');
        final id = summary.cardId ?? '',
            type = summary.typeId ?? '',
            title = summary.title ?? '',
            preview = summary.previewText ?? '';
        _identity(id);
        _identity(type);
        final revision = BigInt.from(summary.revision).toUnsigned(64);
        if (revision == BigInt.zero ||
            summary.formatVersion == 0 ||
            utf8.encode(title).length > 16384 ||
            utf8.encode(preview).length > 16384)
          throw const FormatException('Summary limit');
        return RuntimeReply._(
          kind: 'summary',
          cardId: id,
          typeId: type,
          formatVersion: summary.formatVersion,
          revision: revision,
          title: title,
          previewText: preview,
        );
      case 3:
        final failure = root.rejected;
        if (failure == null) throw const FormatException('Unknown failure');
        return RuntimeReply._(
          kind: 'rejected',
          failure: failure.name[0].toUpperCase() + failure.name.substring(1),
        );
      case 4:
        final result = root.operationResult;
        if (result == null)
          throw const FormatException('Missing operation result');
        final card = result.cardId ?? '', operation = result.operationId ?? '';
        _identity(card);
        _identity(operation);
        if (result.which == 0)
          return RuntimeReply._(
            kind: 'operationResult',
            cardId: card,
            operationId: operation,
            resultState: 'absentSnapshot',
          );
        if (result.which != 1)
          throw const FormatException('Unknown operation state');
        return _receipt(
          result.locallyCommitted,
          kind: 'operationResult',
          operationId: operation,
          cardId: card,
        );
      default:
        throw const FormatException('Unsupported response');
    }
  }

  static RuntimeReply _receipt(
    CommitReceiptReader? receipt, {
    required String kind,
    required String operationId,
    String? cardId,
  }) {
    if (receipt == null) throw const FormatException('Missing receipt');
    final card = receipt.cardId ?? '', event = receipt.eventId ?? '';
    _identity(card);
    _identity(event);
    final revision = BigInt.from(receipt.revision).toUnsigned(64),
        digest = receipt.contentSha256;
    if (receipt.operationId != operationId ||
        (cardId != null && card != cardId) ||
        revision == BigInt.zero ||
        digest?.length != 32)
      throw const FormatException('Receipt mismatch');
    return RuntimeReply._(
      kind: kind,
      cardId: card,
      operationId: operationId,
      eventId: event,
      revision: revision,
      contentSha256: Uint8List.fromList(digest!),
      resultState: kind == 'operationResult' ? 'locallyCommitted' : null,
    );
  }
}
