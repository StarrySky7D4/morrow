import 'dart:convert';
import '../attachment_transfer.dart';
import 'dart:js_interop';
import 'dart:typed_data';

const maxMessageBytes = 64 * 1024;
@JS('BigInt')
external JSBigInt _bigInt(JSString value);
@JS('String')
external JSString _string(JSAny value);
@JS('morrowCodec.rename_encode')
external JSUint8Array _encode(
  JSString operation,
  JSString card,
  JSBigInt revision,
  JSString title,
);
@JS('morrowCodec.rename_decode')
external _Decoded _decode(JSUint8Array bytes);
extension type _Decoded._(JSObject _) implements JSObject {
  external JSString get operation_id;
  external JSString get card_id;
  external JSString get title;
  external JSBigInt get revision;
  external void free();
}
void _identity(String value) {
  if (value.isEmpty ||
      utf8.encode(value).length > 256 ||
      RegExp(r'[\x00-\x1f\x7f-\x9f/\\:]').hasMatch(value))
    throw const FormatException('Invalid identity');
}

/// Uses the shared Rust codec; BigInt crosses JS/Wasm without a Number conversion.
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
        expectedRevision > (BigInt.one << 64) - BigInt.one ||
        utf8.encode(title).length > 16 * 1024)
      throw const FormatException('Revision or title limit');
  }

  Uint8List encode() {
    validate();
    try {
      return Uint8List.fromList(
        _encode(
          operationId.toJS,
          cardId.toJS,
          _bigInt(expectedRevision.toString().toJS),
          title.toJS,
        ).toDart,
      );
    } catch (error) {
      throw FormatException('Rust codec unavailable or rejected input: $error');
    }
  }

  static RenameCommand decode(Uint8List bytes) {
    if (bytes.isEmpty || bytes.length > maxMessageBytes)
      throw const FormatException('Message length');
    _Decoded? value;
    try {
      value = _decode(bytes.toJS);
      return RenameCommand(
        operationId: value.operation_id.toDart,
        cardId: value.card_id.toDart,
        expectedRevision: BigInt.parse(_string(value.revision).toDart),
        title: value.title.toDart,
      );
    } catch (error) {
      throw FormatException('Invalid runtime message: $error');
    } finally {
      value?.free();
    }
  }
}

@JS('morrowCodec.read_encode')
external JSUint8Array _readEncode(JSString requestId, JSString cardId);
@JS('morrowCodec.response_decode')
external _Reply _replyDecode(JSUint8Array bytes);
extension type _Reply._(JSObject _) implements JSObject {
  external JSString get request_id;
  external JSString get kind;
  external JSBigInt? get revision;
  external JSString? get title;
  external JSString? get failure;
  external JSString? get result_state;
  external JSString? get operation_id;
  external JSString? get card_id;
  external JSString? get attachment_id;
  external JSBigInt? get offset;
  external JSBigInt? get total_length;
  external JSUint8Array? get content_sha256;
  external JSUint8Array? get bytes;
  external void free();
}

class ReadSummaryCommand {
  ReadSummaryCommand({required this.requestId, required this.cardId}) {
    _identity(requestId);
    _identity(cardId);
  }
  final String requestId, cardId;
  Uint8List encode() =>
      Uint8List.fromList(_readEncode(requestId.toJS, cardId.toJS).toDart);
}

class RuntimeReply {
  RuntimeReply._(
    this.kind,
    this.revision,
    this.title,
    this.failure,
    this.resultState,
    this.operationId,
    this.cardId,
    this.attachmentPart,
  );
  final String kind;
  final BigInt? revision;
  final String? title, failure, resultState, operationId, cardId;
  final AttachmentPart? attachmentPart;
  static RuntimeReply decode(Uint8List bytes, {required String requestId}) {
    if (bytes.isEmpty || bytes.length > maxMessageBytes)
      throw const FormatException('Response length');
    final reply = _replyDecode(bytes.toJS);
    try {
      if (reply.request_id.toDart != requestId)
        throw const FormatException('Response correlation mismatch');
      return RuntimeReply._(
        reply.kind.toDart,
        reply.revision == null
            ? null
            : BigInt.parse(_string(reply.revision!).toDart),
        reply.title?.toDart,
        reply.failure?.toDart,
        reply.result_state?.toDart,
        reply.operation_id?.toDart,
        reply.card_id?.toDart,
        reply.kind.toDart == 'attachmentChunk'
            ? AttachmentPart(
                cardId: reply.card_id!.toDart,
                attachmentId: reply.attachment_id!.toDart,
                revision: BigInt.parse(_string(reply.revision!).toDart),
                offset: BigInt.parse(_string(reply.offset!).toDart),
                totalLength: BigInt.parse(_string(reply.total_length!).toDart),
                contentSha256: reply.content_sha256!.toDart,
                bytes: reply.bytes!.toDart,
              )
            : null,
      );
    } finally {
      reply.free();
    }
  }
}

@JS('morrowCodec.query_encode')
external JSUint8Array _queryEncode(
  JSString requestId,
  JSString cardId,
  JSString operationId,
);

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
  Uint8List encode() => Uint8List.fromList(
    _queryEncode(requestId.toJS, cardId.toJS, operationId.toJS).toDart,
  );
}

@JS('morrowCodec.attachment_encode')
external JSUint8Array _attachmentEncode(
  JSString requestId,
  JSString cardId,
  JSString attachmentId,
  JSBigInt revision,
  JSBigInt offset,
  JSNumber length,
);

class ReadAttachmentCommand {
  ReadAttachmentCommand({
    required this.requestId,
    required this.cardId,
    required this.attachmentId,
    required this.expectedRevision,
    required this.offset,
    this.length = maxAttachmentPartBytes,
  }) {
    _identity(requestId);
    _identity(cardId);
    _identity(attachmentId);
    if (expectedRevision <= BigInt.zero ||
        expectedRevision > (BigInt.one << 64) - BigInt.one ||
        offset < BigInt.zero ||
        offset > maxAttachmentBytes ||
        length <= 0 ||
        length > maxAttachmentPartBytes)
      throw const FormatException('Attachment request bounds');
  }
  final String requestId, cardId, attachmentId;
  final BigInt expectedRevision, offset;
  final int length;
  Uint8List encode() => Uint8List.fromList(
    _attachmentEncode(
      requestId.toJS,
      cardId.toJS,
      attachmentId.toJS,
      _bigInt(expectedRevision.toString().toJS),
      _bigInt(offset.toString().toJS),
      length.toJS,
    ).toDart,
  );
}
