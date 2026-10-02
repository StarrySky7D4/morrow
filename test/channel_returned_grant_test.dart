import 'dart:async';
import 'dart:typed_data';

import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/channel_task_models.dart';
import 'package:morrow_studio/plugins/channel_task_session.dart';
import 'package:morrow_studio/plugins/generated/channel.capnp.dart' as wire;
import 'package:morrow_studio/plugins/generated/identity.dart' as contract;

import 'channel_task_session_test.dart' show budget, identity, source;

ChannelBudget grant({
  int channels = 1,
  int frameBytes = 256,
  int bytes = 4096,
  int messages = 64,
  int requests = 64,
  int duration = 30000,
}) => ChannelBudget(
  maxChannels: channels,
  maxFrameBytes: frameBytes,
  maxBytes: BigInt.from(bytes),
  maxMessages: BigInt.from(messages),
  maxRequests: BigInt.from(requests),
  maxDurationMs: BigInt.from(duration),
);

ChannelPrepareRequest preparation(
  List<ChannelSourceFrame> frames, {
  ChannelBudget? requestedBudget,
  ChannelSourceKind kind = ChannelSourceKind.byteStream,
  bool duplex = false,
  int lifetimeMs = 1000,
}) => ChannelPrepareRequest(
  submission: identity(7),
  packageId: 'org.example.directory',
  packageDigest: identity(8),
  registryRevision: BigInt.one,
  handler: 'channel.consume',
  kind: kind,
  duplex: duplex,
  budget: requestedBudget ?? budget(),
  lifetimeMs: lifetimeMs,
  frameCount: frames.length,
  totalBytes: frames.fold(
    BigInt.zero,
    (sum, frame) => sum + BigInt.from(frame.bytes.length),
  ),
);

ChannelDirectory directory(
  ChannelBudget ceiling, {
  ChannelSourceKind kind = ChannelSourceKind.byteStream,
  int endpoints = 1,
  bool wrongReference = false,
  bool wrongEpoch = false,
  bool wrongWireSize = false,
}) {
  final channels = <ChannelEndpoint>[
    for (var index = 0; index < endpoints; index++)
      ChannelEndpoint(
        reference: identity(wrongReference ? 25 : 21 + index),
        sourceEpoch: identity(wrongEpoch ? 35 : 31 + index),
        kind: kind,
        budget: ceiling,
      ),
  ];
  final message = MessageBuilder();
  final root = message.initRoot(wire.directoryFactory);
  root.version = 1;
  root.schemaSha256 = Uint8List.fromList(contract.channelDigest);
  root.scopeSha256 = Uint8List.fromList(identity(41));
  final entries = root.initChannels(channels.length);
  for (var index = 0; index < channels.length; index++) {
    final endpoint = channels[index];
    final entry = entries[index];
    entry.reference = endpoint.reference;
    entry.sourceEpoch = endpoint.sourceEpoch;
    entry.kind = wire.Kind.values[kind.index];
    final out = entry.initBudget();
    out.maxChannels = ceiling.maxChannels;
    out.maxFrameBytes = ceiling.maxFrameBytes;
    out.maxBytesBigInt = ceiling.maxBytes;
    out.maxMessagesBigInt = ceiling.maxMessages;
    out.maxRequestsBigInt = ceiling.maxRequests;
    out.maxDurationMsBigInt = ceiling.maxDurationMs;
  }
  final bytes = message.serialize();
  return ChannelDirectory(
    scopeSha256: identity(41),
    wire: wrongWireSize ? bytes.sublist(0, bytes.length - 1) : bytes,
    channels: channels,
  );
}

final class GrantBackend implements ChannelTaskBackend {
  GrantBackend(this.returnedDirectory, {this.foreignSubmission = false});
  final ChannelDirectory returnedDirectory;
  final bool foreignSubmission;
  ChannelPrepareRequest? request;
  int prepares = 0, runs = 0, uploadedFrames = 0;
  BigInt uploadedBytes = BigInt.zero;
  final appended = <ChannelSourceFrame>[];
  final appendedKeys = <Uint8List>[];
  final statusKeys = <Uint8List>[];
  final closeKeys = <Uint8List>[];
  final runKeys = <Uint8List>[];
  Uint8List? runInput;
  ChannelTaskSnapshot? prepared;
  bool closed = false;
  Completer<ChannelTaskSnapshot>? prepareGate;

  @override
  bool get supportsLocalChannels => true;

  ChannelTaskSnapshot state() => ChannelTaskSnapshot(
    key: identity(11),
    submission: foreignSubmission ? identity(70) : request!.submission,
    directory: returnedDirectory,
    reference: identity(21),
    sourceEpoch: identity(31),
    phase: closed ? ChannelJobPhase.closed : ChannelJobPhase.ready,
    status: closed ? ChannelStatus.closed : ChannelStatus.ready,
    lastAcked: BigInt.zero,
    acceptedSequence: BigInt.zero,
    observedSequence: BigInt.zero,
    cleanupProof: closed
        ? ChannelCleanupProof.noProducer
        : ChannelCleanupProof.pending,
    producerOutcome: ChannelProducerOutcome.pending,
    taskState: ChannelTaskState.pending,
    taskError: '',
    outputType: '',
    output: [],
    inputSha256: [],
    uploadedFrames: uploadedFrames,
    uploadedBytes: uploadedBytes,
    sourceFrames: 0,
    sourceBytes: BigInt.zero,
    resourceReclaimed: closed,
    closeRequested: closed,
    observedBytes: BigInt.zero,
    observedSha256: [],
    workerJoined: false,
    snapshotPending: false,
  );

  @override
  Future<ChannelTaskSnapshot> prepareChannel(ChannelPrepareRequest value) {
    prepares++;
    request = value;
    prepared = state();
    return prepareGate?.future ?? Future.value(prepared);
  }

  @override
  Future<ChannelTaskSnapshot> appendChannel(
    Uint8List key,
    ChannelSourceFrame frame,
  ) async {
    appended.add(frame);
    appendedKeys.add(key);
    uploadedFrames++;
    uploadedBytes += BigInt.from(frame.bytes.length);
    return state();
  }

  @override
  Future<ChannelTaskSnapshot> runChannel(Uint8List key, Uint8List input) async {
    runs++;
    runKeys.add(key);
    runInput = input;
    return state();
  }

  @override
  Future<ChannelTaskSnapshot> statusChannel(Uint8List key) async {
    statusKeys.add(key);
    return state();
  }

  @override
  Future<ChannelTaskSnapshot> closeChannel(Uint8List key) async {
    closeKeys.add(key);
    closed = true;
    return state();
  }

  @override
  Future<ChannelSentFrame?> readChannelSent(
    Uint8List key,
    BigInt sequence,
  ) async => null;
}

void main() {
  final widened = <String, ChannelBudget>{
    'maxChannels': grant(channels: 2),
    'maxFrameBytes': grant(frameBytes: 257),
    'maxBytes': grant(bytes: 4097),
    'maxMessages': grant(messages: 65),
    'maxRequests': grant(requests: 65),
    'maxDurationMs': grant(duration: 30001),
  };
  final refused = <String, ChannelDirectory>{
    for (final item in widened.entries)
      'widened ${item.key}': directory(item.value),
    'wrong exact kind': directory(grant(), kind: ChannelSourceKind.events),
    'no endpoint': directory(grant(), endpoints: 0),
    'multiple endpoints': directory(grant(), endpoints: 2),
    'wrong endpoint reference': directory(grant(), wrongReference: true),
    'wrong endpoint epoch': directory(grant(), wrongEpoch: true),
    'wrong total wire size': directory(grant(), wrongWireSize: true),
    'lifetime does not fit': directory(grant(duration: 999)),
    'whole frame count does not fit': directory(grant(messages: 1)),
    'whole byte total does not fit': directory(grant(frameBytes: 3, bytes: 4)),
    'later frozen frame does not fit': directory(
      grant(frameBytes: 2, bytes: 5),
    ),
  };

  for (final item in refused.entries) {
    testWidgets('returned grant refuses before every append/run: ${item.key}', (
      tester,
    ) async {
      final backend = GrantBackend(item.value);
      final session = ChannelTaskSession.forBackend(backend);
      final frames = source();
      final value = preparation(frames);
      await session.prepareAndRun(value, frames);
      expect(session.failure, isFormatException);
      expect(session.uncertain, isTrue);
      expect(session.canPrepare, isFalse);
      expect(session.snapshot, same(backend.prepared));
      expect(session.snapshot!.key, identity(11));
      expect(session.snapshot!.directory, same(item.value));
      expect(session.snapshot!.directory.wire, item.value.wire);
      expect(backend.appended, isEmpty);
      expect(backend.runs, 0);
      expect(backend.closeKeys, isEmpty);
      await tester.pump(const Duration(seconds: 1));
      expect(backend.prepares, 1);
      expect(backend.statusKeys, isEmpty);
      // Recovery commands are explicit and always use the identified original.
      await session.refresh();
      expect(backend.statusKeys.single, identity(11));
      expect(session.snapshot!.directory.wire, item.value.wire);
      await session.close();
      expect(backend.closeKeys.single, identity(11));
      expect(session.snapshot!.key, identity(11));
      expect(session.snapshot!.directory.wire, item.value.wire);
      await session.refresh();
      expect(backend.prepares, 1);
      expect(backend.appended, isEmpty);
      expect(backend.runs, 0);
    });
  }

  testWidgets(
    'local single-channel ceiling survives a wider requested ceiling',
    (tester) async {
      final backend = GrantBackend(directory(grant(channels: 2)));
      final session = ChannelTaskSession.forBackend(backend);
      final frames = source();
      await session.prepareAndRun(
        preparation(frames, requestedBudget: grant(channels: 2)),
        frames,
      );
      expect(session.failure, isFormatException);
      expect(session.snapshot!.key, identity(11));
      expect(backend.appended, isEmpty);
      expect(backend.runs, 0);
      expect(backend.closeKeys, isEmpty);
    },
  );

  for (final kind in ChannelSourceKind.values) {
    testWidgets('legitimate narrowing with one guest request: ${kind.name}', (
      tester,
    ) async {
      final frames = [
        ChannelSourceFrame(
          sequence: BigInt.one,
          bytes: [10, 11],
          cursor: kind == ChannelSourceKind.events ? [100] : [],
        ),
        ChannelSourceFrame(
          sequence: BigInt.two,
          bytes: [12, 13, 14],
          cursor: kind == ChannelSourceKind.events ? [101] : [],
        ),
      ];
      final narrowed = directory(
        grant(
          frameBytes: 3,
          bytes: 5,
          messages: 2,
          requests: 1,
          duration: 1000,
        ),
        kind: kind,
      );
      final backend = GrantBackend(narrowed);
      final session = ChannelTaskSession.forBackend(backend);
      await session.prepareAndRun(
        preparation(frames, requestedBudget: grant(channels: 2), kind: kind),
        frames,
      );
      expect(session.failure, isNull);
      expect(session.uncertain, isFalse);
      expect(backend.appended, frames);
      expect(backend.appendedKeys, everyElement(identity(11)));
      expect(backend.runs, 1);
      expect(backend.runKeys.single, identity(11));
      expect(backend.runInput, narrowed.wire);
      expect(
        session.snapshot!.directory.channels.single.budget.maxRequests,
        BigInt.one,
      );
      expect(backend.prepares, 1);
      expect(backend.closeKeys, isEmpty);
      await session.close();
      await session.refresh();
    });
  }

  testWidgets('empty duplex source fits a legitimate one-request grant', (
    tester,
  ) async {
    final backend = GrantBackend(
      directory(
        grant(
          frameBytes: 1,
          bytes: 1,
          messages: 1,
          requests: 1,
          duration: 1000,
        ),
      ),
    );
    final session = ChannelTaskSession.forBackend(backend);
    await session.prepareAndRun(preparation([], duplex: true), []);
    expect(session.failure, isNull);
    expect(backend.appended, isEmpty);
    expect(backend.runs, 1);
    expect(backend.closeKeys, isEmpty);
    await session.close();
    await session.refresh();
  });

  testWidgets('foreign submission never adopts or guesses a cleanup key', (
    tester,
  ) async {
    final backend = GrantBackend(directory(grant()), foreignSubmission: true);
    final session = ChannelTaskSession.forBackend(backend);
    final frames = source();
    await session.prepareAndRun(preparation(frames), frames);
    expect(session.failure, isFormatException);
    expect(session.uncertain, isTrue);
    expect(session.snapshot, isNull);
    expect(session.observedTaskResult, isNull);
    expect(session.request!.submission, identity(7));
    expect(session.canPrepare, isFalse);
    await session.refresh();
    await session.close();
    await tester.pump(const Duration(seconds: 1));
    expect(backend.prepares, 1);
    expect(backend.appended, isEmpty);
    expect(backend.runs, 0);
    expect(backend.statusKeys, isEmpty);
    expect(backend.closeKeys, isEmpty);
  });

  testWidgets('lost preparation reply never invents a key or another grant', (
    tester,
  ) async {
    final backend = GrantBackend(directory(grant()))..prepareGate = Completer();
    final session = ChannelTaskSession.forBackend(backend);
    final frames = source();
    final running = session.prepareAndRun(preparation(frames), frames);
    backend.prepareGate!.completeError(StateError('reply lost'));
    await running;
    expect(session.uncertain, isTrue);
    expect(session.snapshot, isNull);
    expect(session.canPrepare, isFalse);
    await session.refresh();
    await session.close();
    await tester.pump(const Duration(seconds: 1));
    expect(backend.prepares, 1);
    expect(backend.appended, isEmpty);
    expect(backend.runs, 0);
    expect(backend.statusKeys, isEmpty);
    expect(backend.closeKeys, isEmpty);
  });

  testWidgets('all frozen frames are checked before an early fitting append', (
    tester,
  ) async {
    final backend = GrantBackend(directory(grant(frameBytes: 2, bytes: 5)))
      ..prepareGate = Completer();
    final session = ChannelTaskSession.forBackend(backend);
    final frames = source();
    final running = session.prepareAndRun(preparation(frames), frames);
    // Removing the later oversized frame while prepare waits cannot weaken
    // either the caller's immutable source or the whole-source preflight.
    frames.removeLast();
    backend.prepareGate!.complete(backend.prepared!);
    await running;
    expect(session.failure, isFormatException);
    expect(backend.appended, isEmpty);
    expect(backend.runs, 0);
    expect(backend.prepares, 1);
    expect(backend.closeKeys, isEmpty);
    expect(session.snapshot!.key, identity(11));
  });
}
