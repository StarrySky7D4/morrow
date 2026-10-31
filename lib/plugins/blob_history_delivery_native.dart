import 'dart:convert';
import 'dart:typed_data';
import 'package:crypto/crypto.dart';
import 'blob_history_delivery_models.dart';
import 'blob_upload_models.dart';
import 'file_task_native.dart' show FileTaskExchange;
import 'generated/host.capnp.dart' as host;
import 'io_task_models.dart';

/// Private Cap'n Proto controls. Each call exchanges once. A lost preparation
/// or approval reply stays unresolved until an explicit read/discovery call.
final class NativeBlobHistoryDeliveryClient
    implements BlobHistoryDeliveryBackend {
  NativeBlobHistoryDeliveryClient(this._exchange);
  final FileTaskExchange _exchange;
  final _preparations = <String>{}, _approvals = <String>{};

  static bool _same(List<int>? a, List<int> b) =>
      a != null &&
      a.length == b.length &&
      Iterable<int>.generate(b.length).every((i) => a[i] == b[i]);
  static bool _empty(Uint8List? value) => value == null || value.isEmpty;
  static String _hex(List<int> value) =>
      value.map((n) => n.toRadixString(16).padLeft(2, '0')).join();
  static Uint8List _id(Uint8List? value) {
    if (value == null) {
      throw const FormatException('Missing disclosure identity');
    }
    HttpTaskValidation.identity(value);
    return Uint8List.fromList(value).asUnmodifiableView();
  }

  static void _text(String value) {
    if (value.isEmpty ||
        utf8.encode(value).length > 256 ||
        value.runes.any((c) => c < 32 || (c >= 127 && c <= 159))) {
      throw const FormatException('Invalid disclosure identity text');
    }
  }

  static void _request(BlobHistoryDeliveryRequest r) {
    _id(r.requestId);
    _id(r.submission);
    BlobUploadValidation.selection(
      r.packageId,
      r.packageDigest,
      r.registryRevision,
    );
    for (final value in [r.packageId, r.handler, r.outputType]) {
      _text(value);
    }
    if (r.timeoutMs < 1 || r.timeoutMs > 120000) {
      throw const FormatException('Invalid disclosure lifetime');
    }
  }

  static void _write(
    BlobHistoryDeliveryRequest r,
    host.BlobHistoryDeliveryRequestBuilder out,
  ) {
    out.requestId = r.requestId;
    out.submission = r.submission;
    out.packageId = r.packageId;
    out.packageDigest = r.packageDigest;
    out.registryRevisionBigInt = r.registryRevision;
    out.handler = r.handler;
    out.outputType = r.outputType;
    out.timeoutMs = r.timeoutMs;
  }

  static void _previewBytes(BlobHistoryDeliveryPreview value) {
    _id(value.key);
    _request(value.request);
    _id(value.packetSha256);
    final prefix = 'history-v1.${_hex(value.request.submission)}.';
    final nonce = value.taskId.startsWith(prefix)
        ? value.taskId.substring(prefix.length)
        : '';
    if (!RegExp(r'^[0-9a-f]{64}$').hasMatch(nonce) ||
        nonce == '0' * 64 ||
        value.packet.isEmpty ||
        value.packet.length > 8192 ||
        !_same(sha256.convert(value.packet).bytes, value.packetSha256)) {
      throw const FormatException('Disclosure preview correlation changed');
    }
    final selected = value.selectedHistory;
    if (!_same(selected.submission, value.request.submission)) {
      throw const FormatException('Disclosure history selection changed');
    }
  }

  static BlobHistoryDeliveryPreview _preview(
    host.ResponseReader reply,
    BlobHistoryDeliveryRequest expected, {
    Uint8List? key,
  }) {
    _receipt(
      reply.blobHistoryDeliveryReceipt,
      allowAbsent: true,
      requireAbsent: true,
    );
    final p = reply.blobHistoryDeliveryPreview, r = p?.request;
    if (p == null ||
        !p.present ||
        r == null ||
        !_same(r.requestId, expected.requestId) ||
        !_same(r.submission, expected.submission) ||
        r.packageId != expected.packageId ||
        !_same(r.packageDigest, expected.packageDigest) ||
        r.registryRevisionBigInt != expected.registryRevision ||
        r.handler != expected.handler ||
        r.outputType != expected.outputType ||
        r.timeoutMs != expected.timeoutMs ||
        (key != null && !_same(p.key, key)) ||
        p.packet == null ||
        p.taskId == null) {
      throw const FormatException('Unexpected disclosure preview');
    }
    final value = BlobHistoryDeliveryPreview(
      key: _id(p.key),
      request: expected,
      taskId: p.taskId!,
      packet: p.packet!,
      packetSha256: _id(p.packetSha256),
    );
    _previewBytes(value);
    return value;
  }

  static void _absentPreview(host.ResponseReader reply) {
    final p = reply.blobHistoryDeliveryPreview;
    if (p == null ||
        p.present ||
        !_empty(p.key) ||
        p.request != null ||
        (p.taskId?.isNotEmpty ?? false) ||
        !_empty(p.packet) ||
        !_empty(p.packetSha256)) {
      throw const FormatException('Unexpected disclosure preview envelope');
    }
  }

  static BlobHistoryDeliveryReceipt? _receipt(
    host.BlobHistoryDeliveryReceiptReader? r, {
    Uint8List? key,
    Uint8List? requestId,
    String? expectedOutputType,
    bool allowAbsent = false,
    bool requireAbsent = false,
  }) {
    if (r == null) {
      throw const FormatException('Missing disclosure receipt envelope');
    }
    if (!r.found) {
      if (!allowAbsent ||
          !_empty(r.key) ||
          !_empty(r.requestId) ||
          r.consumed ||
          r.dispatched ||
          r.finished ||
          r.acknowledged ||
          r.close != 0 ||
          r.error != 0 ||
          r.deliveryError != 0 ||
          r.hasExecution ||
          r.executionFault != 0 ||
          r.exitCode != 0 ||
          r.hostCalls != 0 ||
          r.fuelRemainingBigInt != BigInt.zero ||
          r.hasPluginFailure ||
          r.pluginFailureCode != 0 ||
          r.hasOutput ||
          !_empty(r.output) ||
          (r.outputType?.isNotEmpty ?? false) ||
          r.closeFailureKind != 0 ||
          r.closeCoreError != 0 ||
          (r.closeDetail?.isNotEmpty ?? false)) {
        throw const FormatException('Contradictory absent disclosure receipt');
      }
      return null;
    }
    if (requireAbsent ||
        (key != null && !_same(r.key, key)) ||
        (requestId != null && !_same(r.requestId, requestId)) ||
        r.close > 4 ||
        r.error > 12 ||
        r.deliveryError > 12 ||
        r.executionFault > 9 ||
        r.pluginFailureCode > 3 ||
        r.closeFailureKind > 2 ||
        r.closeCoreError > 16 ||
        (!r.consumed &&
            (r.dispatched ||
                r.finished ||
                r.acknowledged ||
                r.close != 0 ||
                r.error != 0 ||
                r.hasExecution)) ||
        (r.acknowledged && (!r.finished || (r.close != 0 && r.close != 2))) ||
        (r.hasExecution && !r.dispatched) ||
        (r.close == 0 && (r.dispatched || r.hasExecution)) ||
        (!r.hasExecution &&
            (r.executionFault != 0 ||
                r.exitCode != 0 ||
                r.hostCalls != 0 ||
                r.fuelRemainingBigInt != BigInt.zero ||
                r.hasPluginFailure ||
                r.hasOutput)) ||
        (r.executionFault != 0 && r.exitCode != 0) ||
        ((r.executionFault != 0 || r.exitCode != 0) && r.error != 8) ||
        (r.error == 8 &&
            (!r.hasExecution || (r.executionFault == 0 && r.exitCode == 0))) ||
        (r.error == 9 && r.deliveryError == 0 && !r.hasPluginFailure) ||
        (r.error == 12 && r.close != 3 && r.close != 4) ||
        (r.finished &&
            r.error == 0 &&
            (r.close != 2 ||
                !r.hasExecution ||
                (r.deliveryError == 0 && !r.hasOutput))) ||
        (!r.hasPluginFailure && r.pluginFailureCode != 0) ||
        (r.hasPluginFailure && r.error == 0) ||
        (r.deliveryError != 0 && (r.hasPluginFailure || r.hasOutput))) {
      throw const FormatException('Contradictory original disclosure receipt');
    }
    BlobHistoryCloseFailure? closeFailure;
    if (r.closeFailureKind == 1) {
      if (r.close != 3 ||
          r.closeCoreError == 0 ||
          r.error == 0 ||
          (r.closeCoreError != 2 && (r.closeDetail?.isNotEmpty ?? false)) ||
          utf8.encode(r.closeDetail ?? '').length > 256) {
        throw const FormatException('Invalid original close refusal');
      }
      closeFailure = BlobHistoryCloseFailure(
        BlobHistoryCloseFailureKind.core,
        r.closeCoreError,
        r.closeCoreError == 2 ? (r.closeDetail ?? '') : null,
      );
    } else if (r.closeFailureKind == 2) {
      if (r.close != 4 ||
          r.error == 0 ||
          r.closeCoreError != 0 ||
          (r.closeDetail?.isNotEmpty ?? false)) {
        throw const FormatException('Invalid original close unwind');
      }
      closeFailure = const BlobHistoryCloseFailure(
        BlobHistoryCloseFailureKind.unwind,
        null,
        null,
      );
    } else if (r.close == 3 ||
        r.closeCoreError != 0 ||
        (r.closeDetail?.isNotEmpty ?? false)) {
      throw const FormatException('Missing original close failure');
    }
    if (r.finished && (r.close == 3 || r.close == 4) && r.error == 0) {
      throw const FormatException('Unresolved close has no original error');
    }
    Uint8List? output;
    String? outputType;
    if (r.hasOutput) {
      outputType = r.outputType;
      final bytes = r.output;
      if (!r.finished ||
          r.close != 2 ||
          r.error != 0 ||
          r.deliveryError != 0 ||
          r.hasPluginFailure ||
          r.hostCalls != 0 ||
          outputType == null ||
          bytes == null ||
          bytes.length > 8192 ||
          (expectedOutputType != null && outputType != expectedOutputType)) {
        throw const FormatException('Unexpected disclosure output');
      }
      _text(outputType);
      output = bytes;
    } else if (!_empty(r.output) || (r.outputType?.isNotEmpty ?? false)) {
      throw const FormatException('Undeclared disclosure output');
    }
    return BlobHistoryDeliveryReceipt(
      key: _id(r.key),
      requestId: _id(r.requestId),
      consumed: r.consumed,
      dispatched: r.dispatched,
      finished: r.finished,
      acknowledged: r.acknowledged,
      close: BlobHistoryDeliveryClose.values[r.close],
      closeFailure: closeFailure,
      error: r.error == 0 ? null : BlobHistoryDeliveryError.values[r.error - 1],
      deliveryError: r.deliveryError == 0
          ? null
          : BlobHistoryDeliveryError.values[r.deliveryError - 1],
      hasExecution: r.hasExecution,
      executionFault: IoExecutionFault.values[r.executionFault],
      exitCode: r.exitCode,
      hostCalls: r.hostCalls,
      fuelRemaining: r.fuelRemainingBigInt,
      pluginFailureCode: r.hasPluginFailure ? r.pluginFailureCode : null,
      outputType: outputType,
      output: output,
    );
  }

  static void _reserve(Set<String> seen, Uint8List id) {
    final value = _hex(id);
    if (seen.length >= 1024 || !seen.add(value)) {
      throw StateError(
        'Disclosure already submitted; read or discover the original',
      );
    }
  }

  static BlobHistoryDeliveryReceipt _applied(
    host.Action action,
    BlobHistoryDeliveryReceipt value,
  ) {
    if ((action == host.Action.blobHistoryDeliveryApprove ||
            action == host.Action.blobHistoryDeliveryCancel) &&
        (!value.consumed || !value.finished)) {
      throw const FormatException('Disclosure decision was not observed');
    }
    if (action == host.Action.blobHistoryDeliveryAcknowledge &&
        !value.acknowledged) {
      throw const FormatException(
        'Disclosure acknowledgement was not observed',
      );
    }
    return value;
  }

  @override
  Future<BlobHistoryDeliveryPreview> prepareBlobHistoryDelivery(
    BlobHistoryDeliveryRequest request,
  ) {
    _request(request);
    _reserve(_preparations, request.requestId);
    return _exchange(
      host.Action.blobHistoryDeliveryPrepare,
      clearReply: true,
      configure: (r) => _write(request, r.initBlobHistoryDeliveryRequest()),
      decode: (r) => _preview(r, request),
    );
  }

  @override
  Future<BlobHistoryDeliveryPreview> previewBlobHistoryDelivery(
    Uint8List key,
    BlobHistoryDeliveryRequest expected,
  ) {
    final exact = _id(key);
    _request(expected);
    return _exchange(
      host.Action.blobHistoryDeliveryPreview,
      clearReply: true,
      configure: (r) => r.blobHistoryDeliveryKey = exact,
      decode: (r) => _preview(r, expected, key: exact),
    );
  }

  @override
  Future<BlobHistoryDeliveryReceipt> approveBlobHistoryDelivery(
    BlobHistoryDeliveryPreview reviewed,
  ) {
    _previewBytes(reviewed);
    _reserve(_approvals, reviewed.key);
    return _exchange(
      host.Action.blobHistoryDeliveryApprove,
      clearReply: true,
      configure: (r) {
        r.blobHistoryDeliveryKey = reviewed.key;
        final a = r.initBlobHistoryDeliveryApproval();
        a.packetSha256 = reviewed.packetSha256;
        a.packageDigest = reviewed.request.packageDigest;
        a.registryRevisionBigInt = reviewed.request.registryRevision;
      },
      decode: (r) {
        _absentPreview(r);
        return _applied(
          host.Action.blobHistoryDeliveryApprove,
          _receipt(
            r.blobHistoryDeliveryReceipt,
            key: reviewed.key,
            requestId: reviewed.request.requestId,
            expectedOutputType: reviewed.request.outputType,
          )!,
        );
      },
    );
  }

  Future<BlobHistoryDeliveryReceipt> _byKey(
    host.Action action,
    Uint8List key,
    Uint8List requestId, {
    String? expectedOutputType,
  }) {
    final exact = _id(key), request = _id(requestId);
    if (expectedOutputType != null) _text(expectedOutputType);
    return _exchange(
      action,
      clearReply: true,
      configure: (r) => r.blobHistoryDeliveryKey = exact,
      decode: (r) {
        _absentPreview(r);
        return _applied(
          action,
          _receipt(
            r.blobHistoryDeliveryReceipt,
            key: exact,
            requestId: request,
            expectedOutputType: expectedOutputType,
          )!,
        );
      },
    );
  }

  @override
  Future<BlobHistoryDeliveryReceipt> readBlobHistoryDelivery(
    Uint8List key,
    Uint8List requestId, {
    String? expectedOutputType,
  }) => _byKey(
    host.Action.blobHistoryDeliveryRead,
    key,
    requestId,
    expectedOutputType: expectedOutputType,
  );
  @override
  Future<BlobHistoryDeliveryReceipt> cancelBlobHistoryDelivery(
    Uint8List key,
    Uint8List requestId,
  ) => _byKey(host.Action.blobHistoryDeliveryCancel, key, requestId);
  @override
  Future<BlobHistoryDeliveryReceipt> acknowledgeBlobHistoryDelivery(
    BlobHistoryDeliveryReceipt original,
  ) {
    if (!original.canAcknowledge) {
      throw StateError('Original disclosure is unresolved');
    }
    return _byKey(
      host.Action.blobHistoryDeliveryAcknowledge,
      original.key,
      original.requestId,
      expectedOutputType: original.outputType,
    ).then((value) {
      // Acknowledgement changes only its own receipt flag. A new recipient
      // veto may hide output, but cannot rewrite the original execution/close.
      if (value.consumed != original.consumed ||
          value.dispatched != original.dispatched ||
          value.finished != original.finished ||
          value.close != original.close ||
          value.error != original.error ||
          value.hasExecution != original.hasExecution ||
          value.executionFault != original.executionFault ||
          value.exitCode != original.exitCode ||
          value.hostCalls != original.hostCalls ||
          value.fuelRemaining != original.fuelRemaining ||
          value.closeFailure?.kind != original.closeFailure?.kind ||
          value.closeFailure?.coreError != original.closeFailure?.coreError ||
          value.closeFailure?.detail != original.closeFailure?.detail) {
        throw const FormatException('Original disclosure outcome changed');
      }
      if (original.deliveryError == null &&
          value.deliveryError == null &&
          (value.pluginFailureCode != original.pluginFailureCode ||
              value.outputType != original.outputType ||
              ((value.output == null) != (original.output == null)) ||
              (original.output != null &&
                  !_same(value.output, original.output!)))) {
        throw const FormatException('Original disclosure payload changed');
      }
      return value;
    });
  }

  @override
  Future<BlobHistoryDeliveryReceipt?> discoverBlobHistoryDelivery(
    Uint8List requestId, {
    String? expectedOutputType,
  }) {
    final exact = _id(requestId);
    if (expectedOutputType != null) _text(expectedOutputType);
    return _exchange(
      host.Action.blobHistoryDeliveryDiscover,
      clearReply: true,
      configure: (r) => r.blobHistoryDeliveryRequestId = exact,
      decode: (r) {
        _absentPreview(r);
        return _receipt(
          r.blobHistoryDeliveryReceipt,
          requestId: exact,
          expectedOutputType: expectedOutputType,
          allowAbsent: true,
        );
      },
    );
  }
}
