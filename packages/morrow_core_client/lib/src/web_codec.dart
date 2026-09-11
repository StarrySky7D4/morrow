import 'dart:convert';
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
