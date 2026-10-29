import 'dart:typed_data';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/blob_plan_manager.dart';
import 'package:morrow_studio/plugins/blob_plan_models.dart';
import 'package:morrow_studio/plugins/blob_plan_session.dart';
import 'package:morrow_studio/plugins/blob_upload_models.dart';
import 'package:morrow_studio/plugins/blob_upload_session.dart';
import 'support/blob_plan_peers.dart';
import 'support/blob_upload_metadata_peer.dart';

Future<void> click(WidgetTester tester, String key) async {
  final target = find.byKey(ValueKey(key));
  await tester.ensureVisible(target);
  await tester.tap(target);
  await tester.pumpAndSettle();
}

bool enabled(WidgetTester tester, String key) =>
    tester.widget<OutlinedButton>(find.byKey(ValueKey(key))).onPressed != null;

Future<(Original, Plans, MetadataPeer, BlobUploadSession)> prepare({
  bool unknown = false,
}) async {
  final io = Original();
  final plans = Plans(io), peer = MetadataPeer(io)..unknown = unknown;
  final plan = BlobPlanSession.forBackend(plans, io),
      planView = plan.attachView();
  await plan.refresh(planView);
  await plan.start(
    planView,
    BlobPlanRequest(
      submission: id(7),
      packageId: 'org.example.planner',
      packageDigest: id(42),
      registryRevision: BigInt.one,
      handler: 'blob.plan',
      inputType: 'text.utf8',
      input: Uint8List.fromList([97]),
    ),
  );
  plans.joined = true;
  await plan.refresh(planView);
  await plan.read(planView);
  await plan.approve(planView);
  await plan.detachView(planView, cancelOriginal: false);
  io.ready = true;
  io.recovery = unknown;
  final session = BlobUploadSession.forBackend(peer, io),
      view = session.attachView();
  await session.adoptOriginal(view, id(3), id(1));
  await session.read(view);
  io.recovery = true;
  await session.refresh(view);
  await session.detachView(view, cancelOriginal: false);
  peer.onComplete = (_) async {
    io.recovery = unknown;
    return BlobUploadMetadataCompletion(io.io, io.snapshot);
  };
  return (io, plans, peer, session);
}

void main() {
  for (final mode in ['known', 'unknown', 'lost-reply']) {
    testWidgets('explicit original metadata review and completion: $mode', (
      tester,
    ) async {
      tester.view.physicalSize = const Size(1500, 1800);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      final (io, plans, peer, session) = await prepare(
        unknown: mode == 'unknown',
      );
      peer.loseCompletion = mode == 'lost-reply';
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: BlobPlanManager(plans: plans, uploads: peer, io: io),
          ),
        ),
      );
      await tester.pumpAndSettle();
      expect(
        peer.calls,
        isEmpty,
        reason: 'opening a window never completes metadata',
      );
      expect(enabled(tester, 'blob-upload-metadata-complete'), isFalse);
      expect(enabled(tester, 'blob-plan-finish'), isFalse);
      await click(tester, 'blob-upload-metadata-review');
      expect(
        find.text('Pending: the original task result record.'),
        findsOneWidget,
      );
      expect(
        find.text(
          'This record is missing from the library. It will use the result retained by the original task.',
        ),
        findsOneWidget,
      );
      expect(peer.calls, ['review']);
      expect(enabled(tester, 'blob-upload-metadata-complete'), isTrue);
      await click(tester, 'blob-upload-metadata-complete');
      expect(peer.calls, ['review', 'complete']);
      expect(io.calls.where((c) => c == 'start'), isEmpty);
      expect(plans.calls.where((c) => c == 'approve').length, 1);
      if (mode == 'known') {
        expect(enabled(tester, 'blob-plan-finish'), isTrue);
        await click(tester, 'blob-plan-finish');
        expect(session.history.length, 1);
        expect(io.calls.where((c) => c == 'acknowledge').length, 1);
      } else {
        expect(enabled(tester, 'blob-plan-finish'), isFalse);
        expect(enabled(tester, 'blob-upload-metadata-complete'), isFalse);
        expect(session.history, isEmpty);
        if (mode == 'unknown') expect(session.result!.unknown, isTrue);
      }
      await tester.pumpWidget(const SizedBox());
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);
    });
  }
}
