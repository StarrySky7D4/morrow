import 'dart:typed_data';
import 'blob_upload_history_models.dart';
import 'blob_upload_models.dart';
import 'file_task_native.dart' show FileTaskExchange;
import 'generated/host.capnp.dart' as host;
import 'io_task_models.dart';

final class NativeBlobUploadHistoryClient implements BlobUploadHistoryBackend {
  NativeBlobUploadHistoryClient(this._exchange);
  final FileTaskExchange _exchange;

  static int _compare(Uint8List a, Uint8List b) {
    for (var i = 0; i < a.length; i++) {
      if (a[i] != b[i]) return a[i].compareTo(b[i]);
    }
    return 0;
  }

  static Uint8List _identity(Uint8List value) {
    HttpTaskValidation.identity(value);
    return Uint8List.fromList(value).asUnmodifiableView();
  }

  @override
  Future<BlobUploadHistoryPage> listBlobUploadHistory({
    Uint8List? after,
    int limit = 16,
  }) {
    if (limit < 1 || limit > 16) {
      throw const FormatException('Invalid history page size');
    }
    final cursor = after == null ? null : _identity(after);
    return _exchange(
      host.Action.blobUploadHistoryList,
      clearReply: true,
      configure: (r) {
        final query = r.initBlobUploadHistoryQuery();
        query.limit = limit;
        if (cursor != null) query.after = cursor;
      },
      decode: (reply) {
        if (reply.blobUploadCurrentStage != 0) {
          throw const FormatException('Unexpected history stage');
        }
        final rows = reply.blobUploadHistory;
        if (rows == null || rows.length > limit) {
          throw const FormatException('Invalid history page');
        }
        final records = [for (final row in rows) decodeRecord(row)];
        Uint8List? previous = cursor;
        for (final row in records) {
          if (previous != null && _compare(previous, row.submission) >= 0) {
            throw const FormatException('History page order changed');
          }
          previous = row.submission;
        }
        final next = reply.blobUploadHistoryCursor;
        if (records.isEmpty) {
          if (next != null && next.isNotEmpty) {
            throw const FormatException('Unexpected empty-page cursor');
          }
        } else if (next == null ||
            next.length != 32 ||
            _compare(next, records.last.submission) != 0) {
          throw const FormatException('History cursor mismatch');
        }
        return BlobUploadHistoryPage(records, records.isEmpty ? null : next);
      },
    );
  }

  @override
  Future<BlobUploadHistoryInspection> inspectBlobUploadHistory(
    Uint8List submission,
  ) {
    final exact = _identity(submission);
    return _exchange(
      host.Action.blobUploadHistoryInspect,
      clearReply: true,
      configure: (r) => r.initBlobUploadHistoryQuery().submission = exact,
      decode: (reply) {
        final rows = reply.blobUploadHistory;
        if (rows == null ||
            rows.length != 1 ||
            reply.blobUploadCurrentStage < 1 ||
            reply.blobUploadCurrentStage > 3 ||
            (reply.blobUploadHistoryCursor?.isNotEmpty ?? false)) {
          throw const FormatException('Invalid exact history response');
        }
        final record = decodeRecord(rows[0]);
        if (_compare(exact, record.submission) != 0) {
          throw const FormatException('History submission mismatch');
        }
        return BlobUploadHistoryInspection(
          record,
          BlobUploadObservation.values[reply.blobUploadCurrentStage - 1],
        );
      },
    );
  }

  static BlobUploadHistoryRecord decodeRecord(
    host.BlobUploadHistoryReader row,
  ) {
    Uint8List bytes(Uint8List? value, {bool identity = true}) {
      if (value == null || value.length != 32) {
        throw const FormatException('Invalid history digest');
      }
      return identity
          ? _identity(value)
          : Uint8List.fromList(value).asUnmodifiableView();
    }

    String text(String? value) {
      if (value == null) {
        throw const FormatException('Missing history identity');
      }
      BlobUploadValidation.text(value);
      return value;
    }

    final revision = row.revision;
    final reserved = row.reservedAtBigInt,
        observed = row.observedAtBigInt,
        acknowledged = row.acknowledgedAtBigInt;
    if (revision < 1 ||
        revision > 3 ||
        row.resultRecorded != (revision >= 2) ||
        row.acknowledged != (revision == 3) ||
        (!row.resultRecorded && observed != BigInt.zero) ||
        (!row.acknowledged && acknowledged != BigInt.zero) ||
        (row.resultRecorded && observed < reserved) ||
        (row.acknowledged && acknowledged < observed) ||
        row.byteLengthBigInt > BigInt.from(16 * 1024 * 1024)) {
      throw const FormatException('Inconsistent upload history');
    }
    BlobUploadResult? result;
    final value = row.originalResult;
    if (row.resultRecorded) {
      if (value == null ||
          value.kind < 1 ||
          value.kind > 2 ||
          value.phase > 3 ||
          value.error > 10 ||
          value.executionFault > 9 ||
          value.calls > 1024 ||
          value.hostCalls > 1024 ||
          value.wireBytesBigInt > BigInt.from(64 * 1024 * 1024) ||
          value.nativeWorkBytesBigInt > BigInt.from(128 * 1024 * 1024) ||
          (value.kind == 1 &&
              (value.phase == 0 ||
                  value.error != 0 ||
                  !value.hasExecution ||
                  value.executionFault != 0 ||
                  value.exitCode != 0)) ||
          (value.kind == 2 && (value.phase != 0 || value.error == 0)) ||
          (value.error == 7 && !value.unknown) ||
          (!value.hasExecution &&
              (value.executionFault != 0 ||
                  value.exitCode != 0 ||
                  value.hostCalls != 0)) ||
          (value.executionFault != 0 && value.exitCode != 0)) {
        throw const FormatException('Inconsistent original upload result');
      }
      result = BlobUploadResult(
        phase: value.phase == 0
            ? null
            : BlobUploadObservation.values[value.phase - 1],
        error: value.error == 0
            ? null
            : BlobUploadError.values[value.error - 1],
        unknown: value.unknown,
        calls: value.calls,
        wireBytes: value.wireBytesBigInt,
        nativeWorkBytes: value.nativeWorkBytesBigInt,
        hasExecution: value.hasExecution,
        executionFault: IoExecutionFault.values[value.executionFault],
        exitCode: value.exitCode,
        hostCalls: value.hostCalls,
      );
    } else if (value != null &&
        (value.kind != 0 ||
            value.phase != 0 ||
            value.error != 0 ||
            value.unknown ||
            value.calls != 0 ||
            value.wireBytesBigInt != BigInt.zero ||
            value.nativeWorkBytesBigInt != BigInt.zero ||
            value.hasExecution ||
            value.executionFault != 0 ||
            value.exitCode != 0 ||
            value.hostCalls != 0)) {
      throw const FormatException('Unexpected historical result');
    }
    return BlobUploadHistoryRecord(
      submission: bytes(row.submission),
      packageId: text(row.packageId),
      packageDigest: bytes(row.packageDigest),
      registryRevision: row.registryRevisionBigInt,
      nominationRef: bytes(row.nominationRef),
      ownerDomain: bytes(row.ownerDomain),
      operationId: text(row.operationId),
      byteLength: row.byteLengthBigInt,
      wholeSha256: bytes(row.wholeSha256, identity: false),
      handler: text(row.handler),
      inputSha256: bytes(row.inputSha256, identity: false),
      bindingSha256: bytes(row.bindingSha256),
      reservedAt: reserved,
      revision: revision,
      originalResult: result,
      observedAt: row.resultRecorded ? observed : null,
      acknowledgedAt: row.acknowledged ? acknowledged : null,
    );
  }
}
