import 'dart:async';
import 'dart:typed_data';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/blob_upload_history_models.dart';
import 'package:morrow_studio/plugins/blob_upload_history_manager.dart';
import 'package:morrow_studio/plugins/blob_upload_models.dart';
import 'package:morrow_studio/plugins/io_task_models.dart';

Uint8List id(int value) => Uint8List.fromList(List.filled(32, value));
BlobUploadHistoryRecord record({
  bool unknown = true,
  bool acknowledged = true,
}) => BlobUploadHistoryRecord(
  submission: id(1),
  packageId: 'org.example.saved-upload',
  packageDigest: id(2),
  registryRevision: BigInt.one,
  nominationRef: id(3),
  ownerDomain: id(4),
  operationId: 'original-operation',
  byteLength: BigInt.from(41),
  wholeSha256: id(5),
  handler: 'blob.upload',
  inputSha256: id(6),
  bindingSha256: id(7),
  reservedAt: BigInt.from(1793260800000),
  revision: acknowledged ? 3 : 2,
  originalResult: BlobUploadResult(
    phase: unknown ? null : BlobUploadObservation.staged,
    error: unknown ? BlobUploadError.unknown : null,
    unknown: unknown,
    calls: 5,
    wireBytes: BigInt.from(200),
    nativeWorkBytes: BigInt.from(41),
    hasExecution: true,
    executionFault: IoExecutionFault.none,
    exitCode: 0,
    hostCalls: 5,
  ),
  observedAt: BigInt.from(1793260800001),
  acknowledgedAt: acknowledged ? BigInt.from(1793260800002) : null,
);

final class History implements BlobUploadHistoryBackend {
  int lists = 0, inspections = 0;
  bool fail = false;
  Completer<BlobUploadHistoryPage>? pending;
  BlobUploadHistoryRecord row = record();
  @override
  Future<BlobUploadHistoryPage> listBlobUploadHistory({
    Uint8List? after,
    int limit = 16,
  }) async {
    lists++;
    if (pending != null) return pending!.future;
    if (fail) throw StateError('read unavailable');
    return BlobUploadHistoryPage([row], id(1));
  }

  @override
  Future<BlobUploadHistoryInspection> inspectBlobUploadHistory(
    Uint8List submission,
  ) async {
    inspections++;
    expect(submission, id(1));
    return BlobUploadHistoryInspection(row, BlobUploadObservation.staged);
  }
}

Widget app(History history) => MaterialApp(
  home: Scaffold(body: BlobUploadHistoryManager(backend: history)),
);
void main() {
  testWidgets(
    'history keeps acknowledged Unknown separate from staged content',
    (tester) async {
      await tester.binding.setSurfaceSize(const Size(1000, 900));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      final history = History();
      await tester.pumpWidget(app(history));
      await tester.pumpAndSettle();
      expect(find.text('Original task result is uncertain'), findsOneWidget);
      expect(find.text('Original task was acknowledged'), findsOneWidget);
      await tester.tap(find.text('Check current content record'));
      await tester.pumpAndSettle();
      expect(find.text('Current content record: saved'), findsOneWidget);
      expect(find.text('Original task result is uncertain'), findsOneWidget);
      expect(find.text('Original task reported: content saved'), findsNothing);
      expect(history.lists, 1);
      expect(history.inspections, 1);
      expect(find.text('Retry'), findsNothing);
      expect(find.text('Acknowledge'), findsNothing);
    },
  );
  testWidgets('known result does not invent acknowledgement', (tester) async {
    await tester.binding.setSurfaceSize(const Size(1000, 900));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    final history = History()
      ..row = record(unknown: false, acknowledged: false);
    await tester.pumpWidget(app(history));
    await tester.pumpAndSettle();
    expect(find.text('Original task reported: content saved'), findsOneWidget);
    expect(
      find.text('Original task acknowledgement was not recorded'),
      findsOneWidget,
    );
  });
  testWidgets(
    'failed read stays an error rather than an empty successful history',
    (tester) async {
      final history = History()..fail = true;
      await tester.pumpWidget(app(history));
      await tester.pumpAndSettle();
      expect(find.text('No records could be loaded.'), findsOneWidget);
      expect(history.lists, 1);
      history.fail = false;
      await tester.tap(find.byKey(const ValueKey('upload-history-refresh')));
      await tester.pumpAndSettle();
      expect(history.lists, 2);
      expect(find.text('org.example.saved-upload'), findsOneWidget);
    },
  );
  testWidgets('reply from a replaced library cannot populate the new view', (
    tester,
  ) async {
    final old = History()..pending = Completer<BlobUploadHistoryPage>();
    await tester.pumpWidget(app(old));
    await tester.pump();
    final current = History()..fail = true;
    await tester.pumpWidget(app(current));
    await tester.pumpAndSettle();
    old.pending!.complete(BlobUploadHistoryPage([record()], id(1)));
    await tester.pumpAndSettle();
    expect(find.text('org.example.saved-upload'), findsNothing);
    expect(find.text('No records could be loaded.'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });
}
