import 'dart:async';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/changes_page_wait.dart';
import 'package:morrow_studio/plugins/channel_task_models.dart';
import 'package:morrow_studio/plugins/channel_task_session.dart';

import 'channel_task_session_test.dart' show budget, identity, snapshot;

final class WaitBackend extends Fake
    implements ChangesWaitBackend, ChannelTaskBackend {
  final original = Completer<ChangesWaitStatus>();
  final lookups = <Uint8List>[];
  int started = 0;
  int claims = 0;
  ChannelTaskSnapshot? recovered;
  final statusKeys = <Uint8List>[];
  ChangesWaitPhase reply = ChangesWaitPhase.waiting;
  @override
  Future<ChangesWaitStatus> startChangesWait(ChannelPrepareRequest request) {
    started++;
    return original.future;
  }

  @override
  Future<ChangesWaitStatus> changesWaitStatus(
    Uint8List key, {
    bool cancel = false,
  }) async {
    lookups.add(key);
    return ChangesWaitStatus(
      reply,
      reply == ChangesWaitPhase.prepared ? recovered?.key : null,
    );
  }

  @override
  Future<ChannelTaskSnapshot> claimChangesWait(Uint8List key) async {
    claims++;
    throw StateError('lost claim response');
  }

  @override
  Future<ChannelTaskSnapshot> statusChannel(Uint8List key) async {
    statusKeys.add(key);
    return recovered!;
  }
}

ChannelPrepareRequest review() => ChannelPrepareRequest(
  submission: identity(5),
  packageId: 'org.example.changes',
  packageDigest: identity(12),
  registryRevision: BigInt.one,
  handler: 'changes.metadata',
  kind: ChannelSourceKind.events,
  duplex: false,
  budget: budget(),
  lifetimeMs: 30000,
  frameCount: 0,
  totalBytes: BigInt.zero,
  changes: ChangesWindowReview(cards: ['selected'], pageSize: 1),
);

void main() {
  test(
    'a lost claim can inspect its original prepared source without running it',
    () async {
      final backend = WaitBackend();
      final request = review();
      final session = ChannelTaskSession.forBackend(backend);
      await session.prepareAndRun(request, const [], fromApprovedWait: true);
      expect(session.canRecoverPreparedWait, isTrue);
      backend.reply = ChangesWaitPhase.unconfirmed;
      await session.refresh();
      expect(session.canRecoverPreparedWait, isTrue);
      expect(session.canPrepare, isFalse);
      backend.recovered = snapshot(request, 7);
      backend.reply = ChangesWaitPhase.prepared;
      await session.refresh();
      expect(backend.statusKeys.single, orderedEquals(backend.recovered!.key));
      expect(session.snapshot, same(backend.recovered));
      expect(session.canRecoverPreparedWait, isFalse);
      expect(backend.claims, 1);
    },
  );
  test(
    'confirmed pre-admission expiry releases the review for a fresh approval',
    () async {
      final backend = WaitBackend();
      final session = ChannelTaskSession.forBackend(backend);
      await session.prepareAndRun(review(), const [], fromApprovedWait: true);
      backend.reply = ChangesWaitPhase.expired;
      await session.refresh();
      expect(session.uncertain, isFalse);
      expect(session.canPrepare, isTrue);
      expect(session.snapshot, isNull);
      expect(backend.claims, 1);
      expect(backend.statusKeys, isEmpty);
    },
  );
  test('a lost start reply is checked using its original approval and can deliver only once', () async {
    final backend = WaitBackend();
    final deliveries = <ChannelPrepareRequest>[];
    final controller = ChangesPageWait(
      backend,
      (request) async => deliveries.add(request),
    );
    addTearDown(controller.dispose);
    final request = review();
    final started = controller.start(request);
    backend.original.completeError(StateError('start reply lost'));
    await started;
    expect(controller.failure, isA<StateError>());
    expect(controller.original, same(request));
    await expectLater(controller.start(review()), throwsStateError);
    backend.reply = ChangesWaitPhase.available;
    await controller.check();
    await controller.check();
    expect(backend.started, 1);
    expect(backend.lookups.single, orderedEquals(request.submission));
    expect(deliveries, [request]);
    expect(controller.original, isNull);
  });

  test(
    'uncertain cancellation wins over a late start or availability reply',
    () async {
      final backend = WaitBackend();
      var delivered = 0;
      final controller = ChangesPageWait(backend, (_) async {
        delivered++;
      });
      addTearDown(controller.dispose);
      final request = review();
      final started = controller.start(request);
      backend.reply = ChangesWaitPhase.available;
      await controller.check(cancel: true);
      backend.original.complete(
        const ChangesWaitStatus(ChangesWaitPhase.available, null),
      );
      await started;
      await controller.check();
      expect(controller.phase, ChangesWaitPhase.unconfirmed);
      expect(controller.original, same(request));
      expect(delivered, 0);
      backend.reply = ChangesWaitPhase.cancelled;
      await controller.check(cancel: true);
      expect(controller.phase, ChangesWaitPhase.cancelled);
      expect(controller.original, isNull);
      expect(delivered, 0);
      expect(backend.started, 1);
      expect(backend.lookups, everyElement(orderedEquals(request.submission)));
    },
  );
}
