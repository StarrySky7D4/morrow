// The shared part needs the same name in both Dart and Flutter test libraries.
// ignore: unnecessary_library_name
library history_delivery_test_support;

import 'dart:io';
import 'dart:typed_data';
import 'package:crypto/crypto.dart';
import '../../../lib/plugins/io_task_models.dart';
import 'dart:async';
import 'package:test/test.dart';
import '../../../lib/plugins/blob_history_delivery_models.dart';
import '../../../lib/plugins/blob_history_delivery_session.dart';
part 'support/history_delivery_fixture.dart';

void main() {
  test('explicit preview and consent precede one approval', () async {
    final backend = Backend(),
        s = BlobHistoryDeliverySession.forBackend(Backend());
    final original = BlobHistoryDeliverySession.forBackend(backend),
        view = original.attachView(() {});
    expect(await original.prepare(view, request()), isTrue);
    expect(original.canApprove, isFalse);
    expect(await original.approve(view), isFalse);
    expect(backend.approvals, 0);
    original.review(view, true);
    expect(await original.approve(view), isTrue);
    expect(await original.approve(view), isFalse);
    expect(backend.approvals, 1);
    expect(original.preview!.selectedHistory.originalUnknown, isTrue);
    expect(await original.acknowledge(view), isTrue);
    expect(original.finish(view), isTrue);
    expect(original.canPrepare, isTrue);
    expect(s.request, isNull);
  });
  test(
    'lost preparation retains identity and only explicit discovery recovers it',
    () async {
      final backend = Backend()..losePrepare = true;
      final s = BlobHistoryDeliverySession.forBackend(backend),
          view = s.attachView(() {});
      expect(await s.prepare(view, request()), isFalse);
      expect(await s.prepare(view, request()), isFalse);
      expect(backend.prepares, 1);
      expect(s.canApprove, isFalse);
      expect(backend.discoveries, 0);
      backend.absent = true;
      expect(await s.read(view), isFalse);
      expect(s.request, isNotNull);
      backend.absent = false;
      expect(await s.read(view), isTrue);
      expect(backend.previews, 1);
      expect(s.canApprove, isFalse);
      s.review(view, true);
      expect(s.canApprove, isTrue);
    },
  );
  test(
    'lost approval is never submitted twice, including across reopened views',
    () async {
      final backend = Backend()..loseApproval = true;
      final s = BlobHistoryDeliverySession.forBackend(backend);
      var view = s.attachView(() {});
      await s.prepare(view, request());
      s.review(view, true);
      expect(await s.approve(view), isFalse);
      s.detachView(view);
      view = s.attachView(() {});
      expect(
        identical(BlobHistoryDeliverySession.forBackend(backend), s),
        isTrue,
      );
      expect(await s.approve(view), isFalse);
      expect(await s.cancel(view), isFalse);
      expect(await s.read(view), isTrue);
      expect(s.canAcknowledge, isTrue);
      expect(backend.approvals, 1);
      expect(backend.cancels, 0);
    },
  );
  test(
    'detach during preparation preserves the late reply for the original backend',
    () async {
      final backend = Backend()
        ..pending = Completer<BlobHistoryDeliveryPreview>();
      final s = BlobHistoryDeliverySession.forBackend(backend);
      var oldChanges = 0, newChanges = 0;
      final oldView = s.attachView(() => oldChanges++);
      final start = s.prepare(oldView, request());
      s.detachView(oldView);
      final oldCount = oldChanges;
      final newView = s.attachView(() => newChanges++);
      backend.pending!.complete(preview(backend.original!));
      expect(await start, isTrue);
      expect(oldChanges, oldCount);
      expect(newChanges, 1);
      expect(await s.approve(oldView), isFalse);
      expect(s.canApprove, isFalse);
      s.review(newView, true);
      expect(s.canApprove, isTrue);
      expect(backend.prepares, 1);
    },
  );
  test('reopening a preview removes the previous view consent', () async {
    final backend = Backend();
    final s = BlobHistoryDeliverySession.forBackend(backend);
    var view = s.attachView(() {});
    await s.prepare(view, request());
    s.review(view, true);
    s.detachView(view);
    view = s.attachView(() {});
    expect(await s.approve(view), isFalse);
    expect(backend.approvals, 0);
    expect(backend.cancels, 0);
    expect(backend.reads, 0);
  });
  for (final operation in ['cancel', 'ack']) {
    test(
      'lost $operation is observed without repeating the mutation',
      () async {
        final backend = Backend();
        final s = BlobHistoryDeliverySession.forBackend(backend),
            view = s.attachView(() {});
        await s.prepare(view, request());
        if (operation == 'cancel') {
          backend.loseCancel = true;
          expect(await s.cancel(view), isFalse);
          expect(await s.cancel(view), isFalse);
          expect(await s.read(view), isTrue);
          expect(s.receipt!.dispatched, isFalse);
          expect(backend.cancels, 1);
        } else {
          s.review(view, true);
          await s.approve(view);
          backend.loseAck = true;
          expect(await s.acknowledge(view), isFalse);
          expect(await s.acknowledge(view), isFalse);
          expect(s.finish(view), isFalse);
          expect(await s.read(view), isTrue);
          expect(s.finish(view), isTrue);
          expect(backend.acks, 1);
        }
      },
    );
  }
  test(
    'Unknown close blocks acknowledgement and cannot become clean by reread',
    () async {
      final backend = Backend()..close = BlobHistoryDeliveryClose.unknown;
      final s = BlobHistoryDeliverySession.forBackend(backend),
          view = s.attachView(() {});
      await s.prepare(view, request());
      s.review(view, true);
      await s.approve(view);
      expect(s.canAcknowledge, isFalse);
      expect(s.finish(view), isFalse);
      backend.current = receipt(
        backend.original!,
        error: BlobHistoryDeliveryError.close,
      );
      expect(await s.read(view), isFalse);
      expect(s.receipt!.close, BlobHistoryDeliveryClose.unknown);
      expect(await s.acknowledge(view), isFalse);
      expect(backend.acks, 0);
    },
  );
  test(
    'mismatched receipt cannot replace the retained selection or enable approval',
    () async {
      final backend = Backend();
      final s = BlobHistoryDeliverySession.forBackend(backend),
          view = s.attachView(() {});
      await s.prepare(view, request());
      backend.current = receipt(
        backend.original!,
        consumed: false,
        requestId: id(7),
      );
      expect(await s.read(view), isFalse);
      expect(s.canReview, isFalse);
      s.review(view, true);
      expect(await s.approve(view), isFalse);
      expect(backend.approvals, 0);
    },
  );
  test(
    'backend replacement starts no work and cannot adopt the original request',
    () async {
      final old = Backend(), fresh = Backend();
      final a = BlobHistoryDeliverySession.forBackend(old),
          b = BlobHistoryDeliverySession.forBackend(fresh);
      final view = a.attachView(() {});
      await a.prepare(view, request());
      a.detachView(view);
      final other = b.attachView(() {});
      expect(b.request, isNull);
      expect(await b.read(other), isFalse);
      expect(old.prepares, 1);
      expect(fresh.prepares + fresh.reads + fresh.discoveries, 0);
    },
  );
}
