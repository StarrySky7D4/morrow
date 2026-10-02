import 'dart:async';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/channel_task_models.dart';
import 'package:morrow_studio/plugins/channel_task_session.dart';

List<int> identity(int value) => List.filled(32, value);

ChannelBudget budget() => ChannelBudget(
  maxChannels: 1,
  maxFrameBytes: 256,
  maxBytes: BigInt.from(4096),
  maxMessages: BigInt.from(64),
  maxRequests: BigInt.from(64),
  maxDurationMs: BigInt.from(30000),
);

ChannelPrepareRequest request(int value, List<ChannelSourceFrame> frames) =>
    ChannelPrepareRequest(
      submission: identity(value),
      packageId: 'org.example.channel',
      packageDigest: identity(99),
      registryRevision: BigInt.one,
      handler: 'channel.consume',
      kind: ChannelSourceKind.byteStream,
      duplex: false,
      budget: budget(),
      lifetimeMs: 30000,
      frameCount: frames.length,
      totalBytes: frames.fold(
        BigInt.zero,
        (sum, frame) => sum + BigInt.from(frame.bytes.length),
      ),
    );

List<ChannelSourceFrame> source() => [
  ChannelSourceFrame(sequence: BigInt.one, bytes: [10, 11]),
  ChannelSourceFrame(sequence: BigInt.two, bytes: [12, 13, 14]),
];

ChannelTaskSnapshot snapshot(
  ChannelPrepareRequest request,
  int key, {
  int uploadedFrames = 0,
  BigInt? uploadedBytes,
  bool closed = false,
  ChannelTaskState taskState = ChannelTaskState.pending,
  BigInt? observedSequence,
}) {
  final endpoint = ChannelEndpoint(
    reference: identity(key + 10),
    sourceEpoch: identity(key + 20),
    kind: request.kind,
    budget: request.budget,
  );
  return ChannelTaskSnapshot(
    key: identity(key),
    submission: request.submission,
    directory: ChannelDirectory(
      scopeSha256: identity(key + 30),
      wire: List.filled(256, key),
      channels: [endpoint],
    ),
    reference: endpoint.reference,
    sourceEpoch: endpoint.sourceEpoch,
    phase: closed ? ChannelJobPhase.closed : ChannelJobPhase.ready,
    status: closed ? ChannelStatus.closed : ChannelStatus.ready,
    lastAcked: BigInt.zero,
    acceptedSequence: observedSequence ?? BigInt.zero,
    observedSequence: observedSequence ?? BigInt.zero,
    cleanupProof: closed
        ? ChannelCleanupProof.noProducer
        : ChannelCleanupProof.pending,
    producerOutcome: ChannelProducerOutcome.pending,
    taskState: taskState,
    taskError: '',
    outputType: taskState == ChannelTaskState.success ? 'summary' : '',
    output: taskState == ChannelTaskState.success ? [7] : [],
    inputSha256: [],
    uploadedFrames: uploadedFrames,
    uploadedBytes: uploadedBytes ?? BigInt.zero,
    sourceFrames: 0,
    sourceBytes: BigInt.zero,
    resourceReclaimed: closed,
    closeRequested: closed,
    observedBytes: BigInt.zero,
    observedSha256: [],
    workerJoined: false,
    snapshotPending: false,
  );
}

final class Backend implements ChannelTaskBackend {
  int prepares = 0, runs = 0;
  final appended = <ChannelSourceFrame>[];
  final statusKeys = <Uint8List>[];
  final closeKeys = <Uint8List>[];
  final requests = <int, ChannelPrepareRequest>{};
  final uploadedFrames = <int, int>{};
  final uploadedBytes = <int, BigInt>{};
  Completer<ChannelTaskSnapshot>? preparedReply;
  Future<ChannelTaskSnapshot> Function(int key)? onStatus, onClose;
  Future<ChannelSentFrame?> Function(int key, BigInt sequence)? onSent;

  @override
  bool get supportsLocalChannels => true;

  ChannelTaskSnapshot current(int key, {bool closed = false}) => snapshot(
    requests[key]!,
    key,
    uploadedFrames: uploadedFrames[key] ?? 0,
    uploadedBytes: uploadedBytes[key] ?? BigInt.zero,
    closed: closed,
  );

  @override
  Future<ChannelTaskSnapshot> prepareChannel(ChannelPrepareRequest value) {
    final key = ++prepares;
    requests[key] = value;
    return preparedReply?.future ?? Future.value(current(key));
  }

  @override
  Future<ChannelTaskSnapshot> appendChannel(
    Uint8List key,
    ChannelSourceFrame frame,
  ) async {
    appended.add(frame);
    uploadedFrames[key.first] = (uploadedFrames[key.first] ?? 0) + 1;
    uploadedBytes[key.first] =
        (uploadedBytes[key.first] ?? BigInt.zero) +
        BigInt.from(frame.bytes.length);
    return current(key.first);
  }

  @override
  Future<ChannelTaskSnapshot> runChannel(Uint8List key, Uint8List input) async {
    runs++;
    expect(input, current(key.first).directory.wire);
    return current(key.first);
  }

  @override
  Future<ChannelTaskSnapshot> statusChannel(Uint8List key) {
    statusKeys.add(key);
    return onStatus?.call(key.first) ?? Future.value(current(key.first));
  }

  @override
  Future<ChannelTaskSnapshot> closeChannel(Uint8List key) {
    closeKeys.add(key);
    return onClose?.call(key.first) ??
        Future.value(current(key.first, closed: true));
  }

  @override
  Future<ChannelSentFrame?> readChannelSent(
    Uint8List key,
    BigInt sequence,
  ) async => onSent?.call(key.first, sequence);
}

Future<void> stopPolling(ChannelTaskSession session, Backend backend) async {
  backend.onStatus = (key) async => backend.current(key, closed: true);
  await session.refresh();
}

void main() {
  testWidgets('caller list is frozen before listeners and prepare await', (
    tester,
  ) async {
    final backend = Backend()..preparedReply = Completer();
    final session = ChannelTaskSession.forBackend(backend);
    final frames = source();
    final originals = List.of(frames);
    final preparation = request(1, frames);
    var notified = false;
    session.addListener(() {
      if (!notified) {
        notified = true;
        frames.clear();
      }
    });
    final running = session.prepareAndRun(preparation, frames);
    expect(frames, isEmpty);
    frames.add(ChannelSourceFrame(sequence: BigInt.one, bytes: [200]));
    backend.preparedReply!.complete(backend.current(1));
    await running;
    expect(backend.appended, originals);
    expect(backend.appended.map((frame) => frame.bytes), [
      [10, 11],
      [12, 13, 14],
    ]);
    expect(backend.prepares, 1);
    expect(backend.runs, 1);
    expect(session.failure, isNull);
    await stopPolling(session, backend);
  });

  for (final fails in [false, true]) {
    testWidgets(
      'stale status ${fails ? "error" : "reply"} cannot alter a new preparation',
      (tester) async {
        final backend = Backend();
        final session = ChannelTaskSession.forBackend(backend);
        final frames = source();
        await session.prepareAndRun(request(1, frames), frames);
        final oldReply = Completer<ChannelTaskSnapshot>();
        backend.onStatus = (_) => oldReply.future;
        final oldRefresh = session.refresh();
        await session.close();
        expect(session.canPrepare, isTrue);
        backend.preparedReply = Completer();
        final running = session.prepareAndRun(request(2, frames), frames);
        var notifications = 0;
        session.addListener(() => notifications++);
        if (fails) {
          oldReply.completeError(StateError('old status failed'));
        } else {
          oldReply.complete(backend.current(1, closed: true));
        }
        await oldRefresh;
        expect(session.request!.submission, identity(2));
        expect(session.snapshot, isNull);
        expect(session.busy, isTrue);
        expect(session.failure, isNull);
        expect(session.uncertain, isFalse);
        expect(notifications, 0);
        backend.preparedReply!.complete(backend.current(2));
        await running;
        expect(session.snapshot!.key, identity(2));
        expect(backend.prepares, 2);
        expect(backend.runs, 2);
        await stopPolling(session, backend);
      },
    );
  }

  testWidgets('old refresh finally cannot unlock a new generation refresh', (
    tester,
  ) async {
    final backend = Backend();
    final session = ChannelTaskSession.forBackend(backend);
    final frames = source();
    await session.prepareAndRun(request(1, frames), frames);
    final oldReply = Completer<ChannelTaskSnapshot>();
    backend.onStatus = (_) => oldReply.future;
    final oldRefresh = session.refresh();
    await session.close();
    await session.prepareAndRun(request(2, frames), frames);
    final newReply = Completer<ChannelTaskSnapshot>();
    backend.onStatus = (_) => newReply.future;
    final newRefresh = session.refresh();
    expect(backend.statusKeys.map((key) => key.first), [1, 2]);
    oldReply.complete(backend.current(1, closed: true));
    await oldRefresh;
    await session.refresh();
    expect(backend.statusKeys.map((key) => key.first), [1, 2]);
    newReply.complete(backend.current(2));
    await newRefresh;
    expect(session.snapshot!.key, identity(2));
    expect(session.failure, isNull);
    await stopPolling(session, backend);
  });

  for (final fails in [false, true]) {
    testWidgets(
      'stale Close ${fails ? "error" : "reply"} cannot replace a new key or timer',
      (tester) async {
        final backend = Backend();
        final session = ChannelTaskSession.forBackend(backend);
        final frames = source();
        await session.prepareAndRun(request(1, frames), frames);
        final oldReply = Completer<ChannelTaskSnapshot>();
        backend.onClose = (_) => oldReply.future;
        final oldClose = session.close();
        await stopPolling(session, backend);
        expect(session.canPrepare, isTrue);
        await session.prepareAndRun(request(2, frames), frames);
        backend.onStatus = (key) async => backend.current(key);
        var notifications = 0;
        session.addListener(() => notifications++);
        if (fails) {
          oldReply.completeError(StateError('old Close failed'));
        } else {
          oldReply.complete(backend.current(1, closed: true));
        }
        await oldClose;
        expect(session.snapshot!.key, identity(2));
        expect(session.request!.submission, identity(2));
        expect(session.failure, isNull);
        expect(session.uncertain, isFalse);
        expect(notifications, 0);
        expect(backend.closeKeys.map((key) => key.first), [1]);
        await tester.pump(const Duration(milliseconds: 250));
        expect(backend.statusKeys.last.first, 2);
        expect(session.snapshot!.key, identity(2));
        expect(backend.prepares, 2);
        expect(backend.runs, 2);
        await stopPolling(session, backend);
      },
    );
  }

  testWidgets(
    'uncertain reply keeps the original key without replay or Close',
    (tester) async {
      final backend = Backend();
      final session = ChannelTaskSession.forBackend(backend);
      final frames = source();
      await session.prepareAndRun(request(1, frames), frames);
      backend.onStatus = (_) async => throw StateError('lost status');
      await session.refresh();
      expect(session.snapshot!.key, identity(1));
      expect(session.uncertain, isTrue);
      expect(session.canPrepare, isFalse);
      await tester.pump(const Duration(seconds: 1));
      expect(backend.prepares, 1);
      expect(backend.runs, 1);
      expect(backend.closeKeys, isEmpty);
      backend.onClose = (key) async => backend.current(key, closed: true);
      await session.close();
      expect(backend.closeKeys.single, identity(1));
      await stopPolling(session, backend);
    },
  );
  testWidgets(
    'first business result survives same-generation Unknown cleanup',
    (tester) async {
      final backend = Backend();
      final session = ChannelTaskSession.forBackend(backend);
      final frames = source();
      final first = request(1, frames);
      await session.prepareAndRun(first, frames);
      final success = snapshot(
        first,
        1,
        uploadedFrames: frames.length,
        uploadedBytes: first.totalBytes,
        taskState: ChannelTaskState.success,
      );
      backend.onStatus = (_) async => success;
      await session.refresh();
      expect(session.observedTaskResult, same(success));
      final generation = session.generation;
      backend.onStatus = (_) async => snapshot(
        first,
        1,
        uploadedFrames: frames.length,
        uploadedBytes: first.totalBytes,
        closed: true,
        taskState: ChannelTaskState.unknown,
      );
      await session.refresh();
      expect(session.generation, generation);
      expect(session.snapshot!.taskState, ChannelTaskState.unknown);
      expect(session.observedTaskResult, same(success));
      expect(session.observedTaskResult!.output, [7]);
      expect(session.canPrepare, isTrue);
      expect(backend.prepares, 1);
      expect(backend.closeKeys, isEmpty);
    },
  );
}
