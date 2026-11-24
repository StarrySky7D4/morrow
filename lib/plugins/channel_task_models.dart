import 'dart:convert';
import 'dart:typed_data';

enum ChannelSourceKind { byteStream, events }

enum ChannelJobPhase { loading, ready, running, completed, closing, closed }

enum ChannelStatus {
  ready,
  frame,
  acked,
  accepted,
  idle,
  closed,
  closingUnconfirmed,
  revoked,
  expired,
  limit,
  invalid,
  unknown,
  unsupported,
}

enum ChannelCleanupProof { pending, noProducer, joined }

enum ChannelProducerOutcome { pending, eof, unknown }

enum ChannelTaskState { pending, success, guestFailure, runtimeFault, unknown }

/// A declaration or requested ceiling. Only the original native host grants it.
final class ChannelBudget {
  ChannelBudget({
    required this.maxChannels,
    required this.maxFrameBytes,
    required this.maxBytes,
    required this.maxMessages,
    required this.maxRequests,
    required this.maxDurationMs,
  }) {
    if (maxChannels < 1 ||
        maxChannels > 8 ||
        maxFrameBytes < 1 ||
        maxFrameBytes > 65536 ||
        maxBytes < BigInt.from(maxFrameBytes) ||
        maxBytes > BigInt.from(64 * 1024 * 1024) ||
        maxMessages < BigInt.one ||
        maxMessages > BigInt.from(1000000) ||
        maxRequests < BigInt.one ||
        maxRequests > BigInt.from(1000000) ||
        maxDurationMs < BigInt.one ||
        maxDurationMs > BigInt.from(3600000)) {
      throw const FormatException('Invalid finite channel budget');
    }
  }
  final int maxChannels, maxFrameBytes;
  final BigInt maxBytes, maxMessages, maxRequests, maxDurationMs;
}

abstract final class ChannelValidation {
  static Uint8List owned(List<int> bytes) {
    if (bytes.any((value) => value < 0 || value > 255)) {
      throw const FormatException('Invalid original channel byte');
    }
    return Uint8List.fromList(bytes).asUnmodifiableView();
  }

  static Uint8List identity(List<int> bytes) {
    if (bytes.length != 32 || bytes.every((v) => v == 0)) {
      throw const FormatException('Invalid channel identity');
    }
    return owned(bytes);
  }

  static bool same(List<int> a, List<int> b) =>
      a.length == b.length &&
      Iterable<int>.generate(a.length).every((i) => a[i] == b[i]);
  static Uint8List changesCursor(List<int> bytes) {
    if (bytes.length != 73 ||
        bytes.first != 1 ||
        bytes.sublist(1, 33).every((v) => v == 0) ||
        bytes.sublist(33, 41).every((v) => v == 0) ||
        bytes.sublist(41).every((v) => v == 0)) {
      throw const FormatException('Invalid changes acknowledgement');
    }
    return owned(bytes);
  }

  static void u64(BigInt value) {
    if (value < BigInt.zero || value >= (BigInt.one << 64)) {
      throw const FormatException('Channel integer outside UInt64');
    }
  }
}

final class ChangesWindowReview {
  ChangesWindowReview({
    required List<String> cards,
    required this.pageSize,
    List<int>? previousCursor,
  }) : cards = List.unmodifiable(cards),
       previousCursor = previousCursor == null
           ? null
           : ChannelValidation.changesCursor(previousCursor) {
    if (cards.isEmpty ||
        cards.length > 32 ||
        cards.toSet().length != cards.length ||
        cards.any(
          (card) =>
              card.isEmpty ||
              utf8.encode(card).length > 256 ||
              RegExp(r'[\x00-\x1f\x7f-\x9f/\\:]').hasMatch(card),
        ) ||
        pageSize < 1 ||
        pageSize > 64) {
      throw const FormatException('Invalid changes window');
    }
  }
  final List<String> cards;
  final int pageSize;
  final Uint8List? previousCursor;
}

enum ChangesBookmarkState { notSupplied, current, retained, unavailable }

final class ReviewedChangesProgress {
  const ReviewedChangesProgress(this.bookmark, this.latest, this.pending);
  final ChangesBookmarkState bookmark;
  final Uint8List? latest;
  final bool pending;
}

abstract interface class ChangesCheckpointBackend {
  Future<ReviewedChangesProgress> inspectChangesCheckpoint(
    String packageId,
    Uint8List digest,
    BigInt revision,
    ChangesWindowReview selection,
  );
}

abstract interface class ChangesObservationBackend {
  /// No observation is available while the original worker owns the library.
  Future<ReviewedChangesProgress?> observeChangesCheckpoint(
    String packageId,
    Uint8List digest,
    BigInt revision,
    ChangesWindowReview selection,
  );
}

enum ChangesWaitPhase {
  waiting,
  available,
  prepared,
  cancelled,
  expired,
  changed,
  unconfirmed,
}

final class ChangesWaitStatus {
  const ChangesWaitStatus(this.phase, this.preparedKey);
  final ChangesWaitPhase phase;
  final Uint8List? preparedKey;
}

abstract interface class ChangesWaitBackend {
  Future<ChangesWaitStatus> startChangesWait(ChannelPrepareRequest request);
  Future<ChangesWaitStatus> changesWaitStatus(
    Uint8List submission, {
    bool cancel = false,
  });
  Future<ChannelTaskSnapshot> claimChangesWait(Uint8List submission);
}

final class ChannelPrepareRequest {
  ChannelPrepareRequest({
    required List<int> submission,
    required this.packageId,
    required List<int> packageDigest,
    required this.registryRevision,
    required this.handler,
    required this.kind,
    required this.duplex,
    required this.budget,
    required this.lifetimeMs,
    required this.frameCount,
    required this.totalBytes,
    this.compatibilityBudget = false,
    this.changes,
  }) : submission = ChannelValidation.identity(submission),
       packageDigest = ChannelValidation.identity(packageDigest) {
    ChannelValidation.u64(registryRevision);
    if (packageId.isEmpty ||
        packageId.length > 256 ||
        handler.isEmpty ||
        handler.length > 256 ||
        lifetimeMs < 1 ||
        BigInt.from(lifetimeMs) > budget.maxDurationMs ||
        frameCount < 0 ||
        frameCount > 64 ||
        BigInt.from(frameCount) > budget.maxMessages ||
        totalBytes < BigInt.zero ||
        totalBytes > BigInt.from(1024 * 1024) ||
        totalBytes > budget.maxBytes ||
        (frameCount == 0 &&
            (totalBytes != BigInt.zero || (!duplex && changes == null))) ||
        (changes != null &&
            (kind != ChannelSourceKind.events ||
                duplex ||
                frameCount != 0 ||
                handler != 'changes.metadata' ||
                BigInt.from(changes!.pageSize) > budget.maxMessages)) ||
        (duplex && frameCount != 0) ||
        (frameCount > 0 &&
            (totalBytes < BigInt.from(frameCount) ||
                totalBytes > BigInt.from(frameCount * budget.maxFrameBytes)))) {
      throw const FormatException('Invalid bounded local channel source');
    }
  }
  final Uint8List submission, packageDigest;
  final ChangesWindowReview? changes;
  final String packageId, handler;
  final BigInt registryRevision, totalBytes;
  final ChannelSourceKind kind;
  final bool duplex;
  final bool compatibilityBudget;
  final ChannelBudget budget;
  final int lifetimeMs, frameCount;
}

final class ChannelSourceFrame {
  ChannelSourceFrame({
    required this.sequence,
    required List<int> bytes,
    List<int> cursor = const [],
  }) : bytes = ChannelValidation.owned(bytes),
       cursor = ChannelValidation.owned(cursor) {
    ChannelValidation.u64(sequence);
    if (sequence == BigInt.zero ||
        bytes.isEmpty ||
        bytes.length > 65536 ||
        cursor.length > 256) {
      throw const FormatException('Invalid local source frame');
    }
  }
  final BigInt sequence;
  final Uint8List bytes, cursor;
}

final class ChannelEndpoint {
  ChannelEndpoint({
    required List<int> reference,
    required List<int> sourceEpoch,
    required this.kind,
    required this.budget,
  }) : reference = ChannelValidation.identity(reference),
       sourceEpoch = ChannelValidation.identity(sourceEpoch);
  final Uint8List reference, sourceEpoch;
  final ChannelSourceKind kind;
  final ChannelBudget budget;
}

final class ChannelDirectory {
  static const inputType = 'morrow.channel.directory.v1';
  // Canonical one-endpoint Directory, including its 8-byte segment header.
  static const singleEndpointWireBytes = 256;

  ChannelDirectory({
    required List<int> scopeSha256,
    required List<int> wire,
    required List<ChannelEndpoint> channels,
  }) : scopeSha256 = ChannelValidation.identity(scopeSha256),
       wire = ChannelValidation.owned(wire),
       channels = List.unmodifiable(channels);
  final Uint8List scopeSha256, wire;
  final List<ChannelEndpoint> channels;
}

final class ChannelTaskSnapshot {
  ChannelTaskSnapshot({
    required List<int> key,
    required List<int> submission,
    required this.directory,
    required List<int> reference,
    required List<int> sourceEpoch,
    required this.phase,
    required this.status,
    required this.lastAcked,
    List<int> changesCursor = const [],
    required this.acceptedSequence,
    required this.observedSequence,
    required this.cleanupProof,
    required this.producerOutcome,
    required this.taskState,
    required this.taskError,
    required this.outputType,
    required List<int> output,
    required List<int> inputSha256,
    required this.uploadedFrames,
    required this.uploadedBytes,
    required this.sourceFrames,
    required this.sourceBytes,
    required this.resourceReclaimed,
    required this.closeRequested,
    required this.observedBytes,
    required List<int> observedSha256,
    required this.workerJoined,
    required this.snapshotPending,
  }) : key = ChannelValidation.identity(key),
       submission = ChannelValidation.identity(submission),
       reference = ChannelValidation.identity(reference),
       sourceEpoch = ChannelValidation.identity(sourceEpoch),
       changesCursor = changesCursor.isEmpty
           ? ChannelValidation.owned([])
           : ChannelValidation.changesCursor(changesCursor),
       output = ChannelValidation.owned(output),
       inputSha256 = ChannelValidation.owned(inputSha256),
       observedSha256 = ChannelValidation.owned(observedSha256);
  final Uint8List key,
      submission,
      reference,
      sourceEpoch,
      changesCursor,
      output,
      inputSha256,
      observedSha256;
  final ChannelDirectory directory;
  final ChannelJobPhase phase;
  final ChannelStatus status;
  final BigInt lastAcked,
      acceptedSequence,
      observedSequence,
      uploadedBytes,
      sourceBytes,
      observedBytes;
  final ChannelCleanupProof cleanupProof;
  final ChannelProducerOutcome producerOutcome;
  final ChannelTaskState taskState;
  final String taskError, outputType;
  final int uploadedFrames, sourceFrames;
  final bool resourceReclaimed, closeRequested, workerJoined, snapshotPending;
}

final class ChannelSentFrame {
  ChannelSentFrame({required this.sequence, required List<int> bytes})
    : bytes = ChannelValidation.owned(bytes);
  final BigInt sequence;
  final Uint8List bytes;
}

abstract interface class ChannelTaskBackend {
  bool get supportsLocalChannels;
  Future<ChannelTaskSnapshot> prepareChannel(ChannelPrepareRequest request);
  Future<ChannelTaskSnapshot> appendChannel(
    Uint8List key,
    ChannelSourceFrame frame,
  );
  Future<ChannelTaskSnapshot> runChannel(Uint8List key, Uint8List input);
  Future<ChannelTaskSnapshot> statusChannel(Uint8List key);
  Future<ChannelTaskSnapshot> closeChannel(Uint8List key);
  Future<ChannelSentFrame?> readChannelSent(Uint8List key, BigInt sequence);
}
