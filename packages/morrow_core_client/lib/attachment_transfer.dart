import 'dart:convert';
import 'dart:typed_data';
import 'package:crypto/crypto.dart';

const maxAttachmentPartBytes = 32768;
final maxAttachmentBytes = BigInt.from(200 * 1024 * 1024);

/// An owned immutable packet; it is not a complete export until verified.
class AttachmentPart {
  AttachmentPart._(
    this.cardId,
    this.attachmentId,
    this.revision,
    this.offset,
    this.totalLength,
    this.contentSha256,
    this.bytes,
  );
  factory AttachmentPart({
    required String cardId,
    required String attachmentId,
    required BigInt revision,
    required BigInt offset,
    required BigInt totalLength,
    required Uint8List contentSha256,
    required Uint8List bytes,
  }) {
    for (final id in [cardId, attachmentId]) {
      if (id.isEmpty ||
          utf8.encode(id).length > 256 ||
          RegExp(r'[\x00-\x1f\x7f-\x9f/\\:]').hasMatch(id))
        throw const FormatException('Attachment identity');
    }
    if (revision <= BigInt.zero ||
        revision > (BigInt.one << 64) - BigInt.one ||
        offset < BigInt.zero ||
        totalLength < BigInt.zero ||
        totalLength > maxAttachmentBytes ||
        offset > totalLength ||
        contentSha256.length != 32 ||
        bytes.length > maxAttachmentPartBytes ||
        BigInt.from(bytes.length) > totalLength - offset ||
        (bytes.isEmpty && offset != totalLength))
      throw const FormatException('Attachment bounds');
    return AttachmentPart._(
      cardId,
      attachmentId,
      revision,
      offset,
      totalLength,
      Uint8List.fromList(contentSha256).asUnmodifiableView(),
      Uint8List.fromList(bytes).asUnmodifiableView(),
    );
  }
  final String cardId, attachmentId;
  final BigInt revision, offset, totalLength;
  final Uint8List contentSha256, bytes;
}

class _DigestSink implements Sink<Digest> {
  Digest? value;
  @override
  void add(Digest data) {
    value = data;
  }

  @override
  void close() {}
}

/// Feed packets in order while writing to a private staging destination.
/// Publish only after finish succeeds. This verifier holds no full-file buffer.
class AttachmentTransferVerifier {
  AttachmentTransferVerifier({
    required this.cardId,
    required this.attachmentId,
    required this.revision,
  }) {
    _hash = sha256.startChunkedConversion(_digest);
  }
  final String cardId, attachmentId;
  final BigInt revision;
  final _digest = _DigestSink();
  late final ByteConversionSink _hash;
  BigInt _next = BigInt.zero;
  BigInt? _total;
  Uint8List? _expected;
  bool _closed = false;
  BigInt get nextOffset => _next;
  void add(AttachmentPart part) {
    if (_closed) throw StateError('Transfer closed');
    if (part.cardId != cardId ||
        part.attachmentId != attachmentId ||
        part.revision != revision ||
        part.offset != _next)
      throw const FormatException('Mixed or unordered attachment');
    if (_total != null &&
        (part.totalLength != _total || !_same(part.contentSha256, _expected!)))
      throw const FormatException('Attachment metadata changed');
    _total = part.totalLength;
    _expected = part.contentSha256;
    _hash.add(part.bytes);
    _next += BigInt.from(part.bytes.length);
  }

  void finish() {
    if (_closed) throw StateError('Transfer closed');
    if (_total == null || _next != _total)
      throw const FormatException('Incomplete attachment');
    _closed = true;
    _hash.close();
    if (!_same(_digest.value!.bytes, _expected!))
      throw const FormatException('Attachment digest mismatch');
  }
}

bool _same(List<int> a, List<int> b) {
  if (a.length != b.length) return false;
  for (var i = 0; i < a.length; i++) {
    if (a[i] != b[i]) return false;
  }
  return true;
}
