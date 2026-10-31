// The shared part needs the same name in both Dart and Flutter test libraries.
// ignore: unnecessary_library_name
library history_delivery_test_support;

import 'dart:io';
import 'package:crypto/crypto.dart';
import 'package:morrow_studio/plugins/io_task_models.dart';
import 'dart:async';
import 'dart:typed_data';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/blob_history_delivery_manager.dart';
import 'package:morrow_studio/plugins/blob_history_delivery_models.dart';
import 'package:morrow_studio/plugins/blob_upload_history_manager.dart';
import 'package:morrow_studio/plugins/blob_upload_history_models.dart';
import 'package:morrow_studio/plugins/plugin_library.dart';

import 'blob_upload_history_manager_test.dart' as history_fixture;

part '../packages/morrow_core_client/test/support/history_delivery_fixture.dart';

PluginLibraryEntry plugin({int digest = 3, bool enabled = true}) =>
    PluginLibraryEntry(
      id: 'org.example.history',
      name: 'History inspector',
      version: '1.0',
      digest: id(digest),
      enabled: enabled,
      builtin: false,
      available: true,
      declared: const [],
      approved: const [],
      dependencies: const [],
      handlers: const [
        PluginTransformHandler(
          name: 'history.inspect',
          inputType: 'morrow.blob.upload.history.v1',
          outputType: 'text.utf8',
          maxInputBytes: 8192,
          maxOutputBytes: 8192,
        ),
      ],
      issue: '',
    );
Widget app(
  Backend backend, {
  List<PluginLibraryEntry>? plugins,
  VoidCallback? onBack,
}) => MaterialApp(
  home: Scaffold(
    body: BlobHistoryDeliveryManager(
      backend: backend,
      plugins: plugins ?? [plugin()],
      registryRevision: BigInt.one,
      submission: id(1),
      onBack: onBack ?? () {},
    ),
  ),
);
Finder button(String name) => find.byKey(ValueKey('history-delivery-$name'));
Future<void> click(WidgetTester tester, String name) async {
  await tester.ensureVisible(button(name));
  await tester.tap(button(name));
  await tester.pumpAndSettle();
}

Future<void> choose(WidgetTester tester) async {
  await tester.tap(button('recipient'));
  await tester.pumpAndSettle();
  await tester.tap(find.text('History inspector · history.inspect').last);
  await tester.pumpAndSettle();
}

Future<void> prepare(WidgetTester tester) async {
  await choose(tester);
  await click(tester, 'prepare');
}

final class BothBackend extends Backend implements BlobUploadHistoryBackend {
  @override
  Future<BlobUploadHistoryPage> listBlobUploadHistory({
    Uint8List? after,
    int limit = 16,
  }) async => BlobUploadHistoryPage([history_fixture.record()], id(1));
  @override
  Future<BlobUploadHistoryInspection> inspectBlobUploadHistory(
    Uint8List submission,
  ) async => throw StateError('not used');
}

void main() {
  testWidgets(
    'preview is readable and does not authorize delivery before checkbox and button',
    (tester) async {
      await tester.binding.setSurfaceSize(const Size(1100, 1000));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      final backend = Backend();
      await tester.pumpWidget(app(backend));
      await tester.pumpAndSettle();
      await prepare(tester);
      expect(backend.prepares, 1);
      expect(backend.approvals, 0);
      expect(find.text('Original upload outcome is uncertain'), findsOneWidget);
      expect(tester.widget<FilledButton>(button('approve')).onPressed, isNull);
      await click(tester, 'reviewed');
      expect(backend.approvals, 0);
      await click(tester, 'approve');
      expect(backend.approvals, 1);
      expect(backend.acks, 0);
      expect(tester.widget<FilledButton>(button('approve')).onPressed, isNull);
      await click(tester, 'ack');
      expect(backend.acks, 1);
      expect(find.text('Original upload outcome is uncertain'), findsOneWidget);
      await click(tester, 'finish');
      expect(button('prepare'), findsOneWidget);
    },
  );
  testWidgets(
    'lost approval survives dialog disposal without resubmission or automatic reads',
    (tester) async {
      await tester.binding.setSurfaceSize(const Size(1100, 1000));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      final backend = Backend()..loseApproval = true;
      await tester.pumpWidget(app(backend));
      await prepare(tester);
      await click(tester, 'reviewed');
      await click(tester, 'approve');
      expect(
        find.byKey(const ValueKey('history-delivery-unresolved')),
        findsOneWidget,
      );
      await tester.pumpWidget(const SizedBox.shrink());
      await tester.pumpWidget(app(backend));
      await tester.pumpAndSettle();
      expect(backend.approvals, 1);
      expect(backend.reads, 0);
      expect(backend.cancels, 0);
      expect(button('prepare'), findsNothing);
      expect(tester.widget<FilledButton>(button('approve')).onPressed, isNull);
      await click(tester, 'read');
      expect(backend.reads, 1);
      expect(backend.approvals, 1);
      await click(tester, 'ack');
      expect(backend.acks, 1);
    },
  );
  testWidgets('late preparation cannot populate replacement backend view', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1100, 1000));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    final old = Backend()..pending = Completer<BlobHistoryDeliveryPreview>();
    final next = Backend();
    await tester.pumpWidget(app(old));
    await choose(tester);
    await tester.tap(button('prepare'));
    await tester.pump();
    await tester.pumpWidget(app(next));
    await tester.pumpAndSettle();
    old.pending!.complete(preview(old.original!));
    await tester.pumpAndSettle();
    expect(button('prepare'), findsOneWidget);
    expect(button('approve'), findsNothing);
    expect(next.prepares + next.reads + next.discoveries, 0);
    expect(tester.takeException(), isNull);
  });
  testWidgets('changed recipient identity invalidates the visible consent', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1100, 1000));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    final backend = Backend();
    await tester.pumpWidget(app(backend));
    await prepare(tester);
    await click(tester, 'reviewed');
    await tester.pumpWidget(app(backend, plugins: [plugin(digest: 4)]));
    await tester.pumpAndSettle();
    expect(tester.widget<CheckboxListTile>(button('reviewed')).value, isFalse);
    expect(tester.widget<FilledButton>(button('approve')).onPressed, isNull);
    expect(backend.approvals, 0);
  });
  testWidgets(
    'Unknown closure cannot offer acknowledgement or a fresh request',
    (tester) async {
      await tester.binding.setSurfaceSize(const Size(1100, 1000));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      final backend = Backend()..close = BlobHistoryDeliveryClose.unknown;
      await tester.pumpWidget(app(backend));
      await prepare(tester);
      await click(tester, 'reviewed');
      await click(tester, 'approve');
      expect(
        find.text(
          'This delivery has no confirmed closure. Keep its original record.',
        ),
        findsOneWidget,
      );
      expect(tester.widget<OutlinedButton>(button('ack')).onPressed, isNull);
      expect(tester.widget<TextButton>(button('finish')).onPressed, isNull);
      expect(button('prepare'), findsNothing);
    },
  );
  testWidgets(
    'history entry returns to the same route and retains an unfinished delivery',
    (tester) async {
      await tester.binding.setSurfaceSize(const Size(1100, 1000));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      final backend = BothBackend();
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: BlobUploadHistoryManager(
              backend: backend,
              delivery: backend,
              plugins: [plugin()],
              registryRevision: BigInt.one,
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();
      final share = find.byKey(ValueKey('upload-history-share-${hex(id(1))}'));
      await tester.ensureVisible(share);
      await tester.tap(share);
      await tester.pumpAndSettle();
      await prepare(tester);
      await click(tester, 'back');
      expect(find.text('Upload history'), findsOneWidget);
      expect(backend.cancels, 0);
      await tester.tap(
        find.byKey(const ValueKey('upload-history-resume-delivery')),
      );
      await tester.pumpAndSettle();
      expect(button('prepare'), findsNothing);
      expect(backend.prepares, 1);
      expect(backend.reads, 0);
      await click(tester, 'cancel');
      expect(backend.cancels, 1);
      expect(backend.approvals, 0);
      expect(
        find.text('This delivery was cancelled before starting the plugin.'),
        findsOneWidget,
      );
    },
  );
}
