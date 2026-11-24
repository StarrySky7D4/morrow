import 'dart:typed_data';

import 'package:crypto/crypto.dart';

import 'generated/host.capnp.dart' as host;
import 'channel_task_models.dart';
import 'channel_task_codec_native.dart';

typedef ChannelTaskExchange = Future<T> Function<T>(
  host.Action action, {
  void Function(host.RequestBuilder)? configure,
  required T Function(host.ResponseReader) decode,
  required bool clearReply,
});

/// Calls only the original local host; never uses business/service forwarding.
final class NativeChannelTaskClient
    implements
        ChannelTaskBackend,
        ChangesCheckpointBackend,
        ChangesWaitBackend {
  NativeChannelTaskClient(this._exchange, this._supported);
  final ChannelTaskExchange _exchange;
  final bool Function() _supported;
  @override
  bool get supportsLocalChannels => _supported();
  void _requireSupported() {
    if (!supportsLocalChannels) {
      throw StateError('Local channels require the supervised Windows host');
    }
  }

  ChannelTaskSnapshot _snapshot(host.ResponseReader row, {Uint8List? key}) {
    final value = ChannelTaskCodec.snapshot(row.channelState);
    if (key != null && !ChannelValidation.same(value.key, key)) {
      throw const FormatException('Local channel job identity changed');
    }
    return value;
  }

  @override
  Future<ReviewedChangesProgress> inspectChangesCheckpoint(
    String packageId,
    Uint8List digest,
    BigInt revision,
    ChangesWindowReview selection,
  ) {
    _requireSupported();
    return _exchange(
      host.Action.channelChangesCheckpoint,
      configure: (r) {
        r.id = packageId;
        r.sha256 = ChannelValidation.identity(digest);
        r.revisionBigInt = revision;
        final query = r.initChangesCheckpointQuery();
        ChannelTaskCodec.writeReviewedCards(selection, query);
      },
      decode: (reply) {
        final info = reply.changesCheckpoint;
        if (info == null ||
            info.bookmarkState >= ChangesBookmarkState.values.length) {
          throw const FormatException('Invalid changes checkpoint reply');
        }
        final state = ChangesBookmarkState.values[info.bookmarkState];
        final bytes = info.latest;
        final latest = bytes == null || bytes.isEmpty
            ? null
            : ChannelValidation.changesCursor(bytes);
        final provided = selection.previousCursor;
        if ((provided == null) != (state == ChangesBookmarkState.notSupplied) ||
            (state == ChangesBookmarkState.retained && latest == null) ||
            (provided != null &&
                (state == ChangesBookmarkState.current) !=
                    (latest != null &&
                        ChannelValidation.same(provided, latest)))) {
          throw const FormatException(
            'Changes checkpoint does not match the reviewed bookmark',
          );
        }
        return ReviewedChangesProgress(state, latest, info.pending);
      },
      clearReply: true,
    );
  }

  @override
  Future<ChangesWaitStatus> startChangesWait(ChannelPrepareRequest request) {
    _requireSupported();
    if (request.changes == null)
      throw ArgumentError('Select cards for a changes wait');
    return _exchange(
      host.Action.channelChangesWait,
      clearReply: true,
      configure: (r) =>
          ChannelTaskCodec.writePrepare(request, r.initChannelPrepare()),
      decode: (r) => _waitStatus(r, request.submission),
    );
  }

  @override
  Future<ChangesWaitStatus> changesWaitStatus(
    Uint8List submission, {
    bool cancel = false,
  }) {
    _requireSupported();
    final key = ChannelValidation.identity(submission);
    return _exchange(
      cancel
          ? host.Action.channelChangesWaitCancel
          : host.Action.channelChangesWaitStatus,
      clearReply: true,
      configure: (r) => r.channelKey = key,
      decode: (r) => _waitStatus(r, key),
    );
  }

  ChangesWaitStatus _waitStatus(host.ResponseReader reply, Uint8List key) {
    final wait = reply.changesWait;
    if (wait == null ||
        wait.phase >= ChangesWaitPhase.values.length ||
        !ChannelValidation.same(wait.submission ?? Uint8List(0), key)) {
      throw const FormatException('Invalid changes wait receipt');
    }
    final phase = ChangesWaitPhase.values[wait.phase];
    final bytes = wait.preparedKey;
    final prepared = bytes == null || bytes.isEmpty
        ? null
        : ChannelValidation.identity(bytes);
    if ((phase == ChangesWaitPhase.prepared) != (prepared != null)) {
      throw const FormatException('Missing original prepared channel');
    }
    return ChangesWaitStatus(phase, prepared);
  }

  @override
  Future<ChannelTaskSnapshot> claimChangesWait(Uint8List submission) {
    _requireSupported();
    final key = ChannelValidation.identity(submission);
    return _exchange(
      host.Action.channelChangesWaitClaim,
      clearReply: true,
      configure: (r) => r.channelKey = key,
      decode: (r) {
        final current = _snapshot(r);
        if (!ChannelValidation.same(current.submission, key)) {
          throw const FormatException(
            'The prepared channel belongs to a different wait',
          );
        }
        return current;
      },
    );
  }

  @override
  Future<ChannelTaskSnapshot> prepareChannel(ChannelPrepareRequest request) {
    _requireSupported();
    return _exchange(
      request.changes != null
          ? host.Action.channelPrepareChanges
          : request.compatibilityBudget
          ? host.Action.channelPrepareWithCompatibilityBudget
          : host.Action.channelPrepare,
      clearReply: true,
      configure: (row) =>
          ChannelTaskCodec.writePrepare(request, row.initChannelPrepare()),
      decode: (row) {
        final result = _snapshot(row);
        if (!ChannelValidation.same(result.submission, request.submission)) {
          throw const FormatException('Channel submission identity changed');
        }
        return result;
      },
    );
  }

  @override
  Future<ChannelTaskSnapshot> appendChannel(
    Uint8List key,
    ChannelSourceFrame frame,
  ) {
    _requireSupported();
    final expected = ChannelValidation.identity(key);
    return _exchange(
      host.Action.channelAppend,
      clearReply: true,
      configure: (row) {
        final out = row.initChannelAppend();
        out.key = expected;
        out.sequenceBigInt = frame.sequence;
        out.bytes = frame.bytes;
        out.cursor = frame.cursor;
      },
      decode: (row) => _snapshot(row, key: expected),
    );
  }

  @override
  Future<ChannelTaskSnapshot> runChannel(Uint8List key, Uint8List input) {
    _requireSupported();
    final expected = ChannelValidation.identity(key);
    if (input.length > 65536) {
      throw const FormatException('Channel task input exceeds limit');
    }
    final frozen = ChannelValidation.owned(input);
    return _exchange(
      host.Action.channelRun,
      clearReply: true,
      configure: (row) {
        final out = row.initChannelRun();
        out.key = expected;
        out.input = frozen;
      },
      decode: (row) => _snapshot(row, key: expected),
    );
  }

  Future<ChannelTaskSnapshot> _key(host.Action action, Uint8List key) {
    _requireSupported();
    final expected = ChannelValidation.identity(key);
    return _exchange(
      action,
      clearReply: true,
      configure: (row) => row.channelKey = expected,
      decode: (row) => _snapshot(row, key: expected),
    );
  }

  @override
  Future<ChannelTaskSnapshot> statusChannel(Uint8List key) =>
      _key(host.Action.channelStatus, key);
  @override
  Future<ChannelTaskSnapshot> closeChannel(Uint8List key) =>
      _key(host.Action.channelClose, key);
  @override
  Future<ChannelSentFrame?> readChannelSent(
    Uint8List key,
    BigInt sequence,
  ) async {
    _requireSupported();
    final expected = ChannelValidation.identity(key);
    ChannelValidation.u64(sequence);
    if (sequence == BigInt.zero) {
      throw const FormatException('Channel sent sequence must be positive');
    }
    final collected = BytesBuilder(copy: true);
    Uint8List? originalSha;
    int? total;
    var offset = 0;
    // At most two bounded, read-only receipt queries. These never resend a
    // payload, retry a task or grant another source.
    for (var attempt = 0; attempt < 2; attempt++) {
      final wantedOffset = offset;
      final chunk = await _exchange<_ChannelSentChunk?>(
        host.Action.channelReadSent,
        clearReply: true,
        configure: (row) {
          row.channelKey = expected;
          row.channelSequenceBigInt = sequence;
          row.channelOffset = wantedOffset;
          row.channelLimit = 32768;
        },
        decode: (row) {
          _snapshot(row, key: expected);
          final sent = row.channelSent;
          if (sent == null) {
            throw const FormatException('Missing local peer receipt');
          }
          final bytes = sent.bytes ?? Uint8List(0);
          if (!sent.present) {
            if (bytes.isNotEmpty ||
                sent.totalBytes != 0 ||
                (sent.bytesSha256?.isNotEmpty ?? false)) {
              throw const FormatException('Absent peer receipt contains data');
            }
            return null;
          }
          if (sent.sequenceBigInt != sequence ||
              sent.offset != wantedOffset ||
              sent.totalBytes < 1 ||
              sent.totalBytes > 65536 ||
              wantedOffset >= sent.totalBytes ||
              bytes.isEmpty ||
              bytes.length !=
                  (sent.totalBytes - wantedOffset > 32768
                      ? 32768
                      : sent.totalBytes - wantedOffset)) {
            throw const FormatException('Invalid bounded native peer receipt');
          }
          return _ChannelSentChunk(
            ChannelValidation.owned(bytes),
            ChannelValidation.identity(sent.bytesSha256 ?? []),
            sent.totalBytes,
          );
        },
      );
      if (chunk == null) {
        if (offset != 0) {
          throw const FormatException('Native peer receipt became unconfirmed');
        }
        return null;
      }
      if ((total != null && total != chunk.total) ||
          (originalSha != null &&
              !ChannelValidation.same(originalSha, chunk.sha256))) {
        throw const FormatException('Native peer receipt identity changed');
      }
      total = chunk.total;
      originalSha = chunk.sha256;
      collected.add(chunk.bytes);
      offset += chunk.bytes.length;
      if (offset == total) {
        final original = collected.takeBytes();
        if (!ChannelValidation.same(
          sha256.convert(original).bytes,
          originalSha,
        )) {
          throw const FormatException(
            'Native peer receipt original bytes digest mismatch',
          );
        }
        return ChannelSentFrame(sequence: sequence, bytes: original);
      }
    }
    throw const FormatException('Native peer receipt exceeds bounded read');
  }
}

final class _ChannelSentChunk {
  _ChannelSentChunk(this.bytes, this.sha256, this.total);
  final Uint8List bytes, sha256;
  final int total;
}
