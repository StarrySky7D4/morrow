import 'dart:convert';
import 'dart:typed_data';
import 'package:capnproto_dart/capnproto_dart.dart';
import 'generated/blob_upload_history.capnp.dart' as wire;
import 'generated/blob_upload_history_identity.dart';

enum PublicHistoryOutcome { absent, staged, released, error }

enum PublicHistoryStage { absent, staged, released }

final class PublicHistoryExecution {
  const PublicHistoryExecution._(this.exitCode, this.errorCode, this.hostCalls);
  final int? exitCode, errorCode;
  final int hostCalls;
}

final class PublicHistoryResult {
  const PublicHistoryResult._(
    this.outcome,
    this.errorCode,
    this.unknown,
    this.calls,
    this.wireBytes,
    this.nativeWorkBytes,
    this.execution,
  );
  final PublicHistoryOutcome outcome;
  final int? errorCode;
  final bool unknown;
  final int calls;
  final BigInt wireBytes, nativeWorkBytes;
  final PublicHistoryExecution? execution;
}

/// Selected, historical data only. This object carries no delivery or retry
/// authority and contains no private owner domain, nomination or task key.
final class PublicHistoryPacket {
  const PublicHistoryPacket._({
    required this.queryNonce,
    required this.submission,
    required this.packageId,
    required this.packageDigest,
    required this.registryRevision,
    required this.operationId,
    required this.byteLength,
    required this.wholeSha256,
    required this.handler,
    required this.inputSha256,
    required this.requestBindingSha256,
    required this.reservedAt,
    required this.revision,
    required this.originalResult,
    required this.observedAt,
    required this.acknowledgedAt,
    required this.currentStage,
  });
  static const inputType = 'morrow.blob.upload.history.v1';
  final Uint8List queryNonce,
      submission,
      packageDigest,
      wholeSha256,
      inputSha256,
      requestBindingSha256;
  final String packageId, operationId, handler;
  final BigInt registryRevision, byteLength, reservedAt;
  final int revision;
  final PublicHistoryResult? originalResult;
  final BigInt? observedAt, acknowledgedAt;
  final PublicHistoryStage currentStage;
  bool get originalUnknown => originalResult?.unknown ?? true;

  static bool _same(List<int>? a, List<int> b) {
    if (a == null || a.length != b.length) return false;
    for (var i = 0; i < b.length; i++) {
      if (a[i] != b[i]) return false;
    }
    return true;
  }

  static Uint8List _fixed(Uint8List? bytes, {bool nonzero = false}) {
    if (bytes == null ||
        bytes.length != 32 ||
        (nonzero && bytes.every((v) => v == 0))) {
      throw const FormatException('Invalid public history identity');
    }
    return Uint8List.fromList(bytes).asUnmodifiableView();
  }

  static String _text(String? value) {
    if (value == null ||
        value.isEmpty ||
        utf8.encode(value).length > 256 ||
        value.runes.any(
          (c) =>
              c < 32 || (c >= 127 && c <= 159) || c == 47 || c == 92 || c == 58,
        )) {
      throw const FormatException('Invalid public history text');
    }
    return value;
  }

  static MessageReader _read(Uint8List bytes) {
    if (bytes.length < 8 || bytes.length > 8192) {
      throw const FormatException('Public history packet length');
    }
    final view = ByteData.sublistView(bytes);
    final count = view.getUint32(0, Endian.little) + 1;
    if (count > 512) throw const FormatException('Public history segments');
    var total = ((count + 2) ~/ 2) * 8;
    if (total > bytes.length) {
      throw const FormatException('Public history header');
    }
    for (var i = 0; i < count; i++) {
      total += view.getUint32(4 + i * 4, Endian.little) * 8;
      if (total > bytes.length) {
        throw const FormatException('Public history segment length');
      }
    }
    if (total != bytes.length) {
      throw const FormatException('Trailing public history bytes');
    }
    return MessageReader.deserialize(
      bytes,
      const MessageReaderOptions(
        traversalLimitInWords: 4096,
        nestingLimit: 16,
        maxSegments: 512,
      ),
    );
  }

  /// The expected ID comes from the independently checked task envelope, never
  /// from the packet itself. A matching ID does not authenticate its sender.
  static PublicHistoryPacket decodeTask(Uint8List bytes, String taskId) {
    if (taskId.length != 140 ||
        !RegExp(r'^history-v1\.[0-9a-f]{64}\.[0-9a-f]{64}$').hasMatch(taskId)) {
      throw const FormatException('Invalid public history task ID');
    }
    Uint8List fromHex(String s) => Uint8List.fromList([
      for (var i = 0; i < 64; i += 2)
        int.parse(s.substring(i, i + 2), radix: 16),
    ]);
    return decodeFor(
      bytes,
      fromHex(taskId.substring(11, 75)),
      fromHex(taskId.substring(76)),
    );
  }

  static PublicHistoryPacket decodeFor(
    Uint8List bytes,
    Uint8List expectedSubmission,
    Uint8List expectedNonce,
  ) {
    final submission = _fixed(expectedSubmission, nonzero: true);
    final nonce = _fixed(expectedNonce, nonzero: true);
    final reader = _read(bytes);
    final p = reader.getRoot(wire.packetFactory);
    if (p.raw.dataWords != wire.packetFactory.dataWords ||
        p.raw.ptrWords != wire.packetFactory.ptrWords ||
        p.version != 1 ||
        !_same(p.schemaSha256, historySchemaSha256)) {
      throw const FormatException('Public history contract mismatch');
    }
    if (!_same(p.submission, submission) || !_same(p.queryNonce, nonce)) {
      throw const FormatException('Public history selection mismatch');
    }
    final outcome = p.outcome,
        executionKind = p.execution,
        stage = p.currentStage;
    if (outcome == null ||
        executionKind == null ||
        stage == null ||
        (outcome != wire.Outcome.error && p.errorCode != 0) ||
        (outcome == wire.Outcome.error &&
            (p.errorCode < 1 || p.errorCode > 10)) ||
        (outcome == wire.Outcome.error &&
            p.errorCode == 7 &&
            !p.originalUnknown)) {
      throw const FormatException('Invalid original public history result');
    }
    PublicHistoryExecution? execution;
    switch (executionKind) {
      case wire.Execution.notObserved:
        if (p.exitCode != 0 || p.executionError != 0 || p.hostCalls != 0) {
          throw const FormatException('Unobserved public history execution');
        }
      case wire.Execution.returned:
        if (p.executionError != 0) {
          throw const FormatException('Conflicting public history execution');
        }
        execution = PublicHistoryExecution._(p.exitCode, null, p.hostCalls);
      case wire.Execution.rejected:
        if (p.exitCode != 0 || p.executionError < 1 || p.executionError > 9) {
          throw const FormatException('Invalid public history execution error');
        }
        execution = PublicHistoryExecution._(
          null,
          p.executionError,
          p.hostCalls,
        );
    }
    final revision = p.revision,
        reserved = p.reservedAtBigInt,
        observed = p.observedAtBigInt,
        acknowledged = p.acknowledgedAtBigInt;
    if (revision < 1 ||
        revision > 3 ||
        p.byteLengthBigInt > BigInt.from(16 * 1024 * 1024) ||
        (revision == 1 &&
            (outcome != wire.Outcome.notObserved ||
                !p.originalUnknown ||
                p.calls != 0 ||
                p.wireBytesBigInt != BigInt.zero ||
                p.nativeWorkBytesBigInt != BigInt.zero ||
                execution != null ||
                observed != BigInt.zero ||
                acknowledged != BigInt.zero)) ||
        (revision >= 2 &&
            (outcome == wire.Outcome.notObserved || observed < reserved)) ||
        (revision == 2 && acknowledged != BigInt.zero) ||
        (revision == 3 && acknowledged < observed)) {
      throw const FormatException('Inconsistent public history revision');
    }
    final result = outcome == wire.Outcome.notObserved
        ? null
        : PublicHistoryResult._(
            PublicHistoryOutcome.values[outcome.index - 1],
            outcome == wire.Outcome.error ? p.errorCode : null,
            p.originalUnknown,
            p.calls,
            p.wireBytesBigInt,
            p.nativeWorkBytesBigInt,
            execution,
          );
    return PublicHistoryPacket._(
      queryNonce: nonce,
      submission: submission,
      packageId: _text(p.packageId),
      packageDigest: _fixed(p.packageDigest, nonzero: true),
      registryRevision: p.registryRevisionBigInt,
      operationId: _text(p.operationId),
      byteLength: p.byteLengthBigInt,
      wholeSha256: _fixed(p.wholeSha256),
      handler: _text(p.handler),
      inputSha256: _fixed(p.inputSha256),
      requestBindingSha256: _fixed(p.requestBindingSha256, nonzero: true),
      reservedAt: reserved,
      revision: revision,
      originalResult: result,
      observedAt: revision >= 2 ? observed : null,
      acknowledgedAt: revision == 3 ? acknowledged : null,
      currentStage: PublicHistoryStage.values[stage.index],
    );
  }
}
