import 'dart:async';
import 'dart:convert';
import 'dart:typed_data';
import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/blob_plan_manager.dart';
import 'package:morrow_studio/plugins/blob_plan_models.dart';
import 'package:morrow_studio/plugins/blob_plan_session.dart';
import 'package:morrow_studio/plugins/blob_upload_models.dart';
import 'package:morrow_studio/plugins/blob_upload_session.dart';
import 'package:morrow_studio/plugins/io_task_models.dart';
import 'package:morrow_studio/plugins/plugin_library.dart';
import 'support/blob_plan_peers.dart' show Plans, Original;
import 'plugin_library_test.dart' as catalog_fixture;

Uint8List _id(int value) => Uint8List.fromList(List.filled(32, value));
PluginLibraryEntry _entry({
  bool approved = true,
  String type = 'text.utf8',
  int limit = 32768,
}) => PluginLibraryEntry(
  id: 'org.example.planner',
  name: 'My content plugin',
  version: '1',
  digest: _id(42),
  enabled: true,
  builtin: false,
  available: true,
  declared: const [],
  approved: const [],
  dependencies: const [],
  issue: '',
  blobUploadSupported: true,
  blobUploadApproved: approved,
  blobUploadHandlers: const ['blob.upload'],
  blobUploadMaxBytes: BigInt.from(16777216),
  blobPlanners: [BlobPlannerMapping('blob.plan', 'blob.upload')],
  handlers: [
    PluginTransformHandler(
      name: 'blob.plan',
      inputType: type,
      outputType: BlobPlanRequest.outputType,
      maxInputBytes: limit,
      maxOutputBytes: 34816,
    ),
  ],
);

Widget _page(
  Plans plans,
  Original io, {
  PluginLibraryEntry? entry,
  bool recovery = false,
  Future<XFile?> Function()? picker,
  bool direct = false,
  bool chinese = false,
}) {
  Widget manager() => BlobPlanManager(
    plans: plans,
    uploads: io,
    io: io,
    entry: recovery ? null : entry ?? _entry(),
    registryRevision: BigInt.one,
    pickInput: picker,
  );
  return MaterialApp(
    locale: chinese ? const Locale('zh') : const Locale('en'),
    supportedLocales: const [Locale('zh'), Locale('en')],
    localizationsDelegates: const [
      GlobalMaterialLocalizations.delegate,
      GlobalWidgetsLocalizations.delegate,
      GlobalCupertinoLocalizations.delegate,
    ],
    home: direct
        ? Scaffold(body: manager())
        : Scaffold(
            body: Builder(
              builder: (context) => TextButton(
                key: const ValueKey('open'),
                onPressed: () => showDialog<void>(
                  context: context,
                  builder: (_) => manager(),
                ),
                child: const Text('Open'),
              ),
            ),
          ),
  );
}

Future<void> _click(WidgetTester tester, String key) async {
  final finder = find.byKey(ValueKey(key));
  await tester.ensureVisible(finder);
  await tester.tap(finder);
  await tester.pumpAndSettle();
}

bool _enabled(WidgetTester tester, String key) =>
    tester.widget<OutlinedButton>(find.byKey(ValueKey(key))).onPressed != null;
Future<void> _chooseRealInput(WidgetTester tester) async {
  final button = find.byKey(const ValueKey('blob-plan-file'));
  await tester.ensureVisible(button);
  await tester.runAsync(() async {
    await tester.tap(button);
    // Observe one original selection on the real event loop, including stream
    // cancellation. No second picker invocation or simulated elapsed IO.
    for (var i = 0; i < 200; i++) {
      await Future<void>.delayed(const Duration(milliseconds: 10));
      await tester.pump();
      if (find.byType(LinearProgressIndicator).evaluate().isEmpty) return;
    }
    fail('Original file selection did not finish');
  });
  await tester.pumpAndSettle();
}

Future<void> _mount(
  WidgetTester tester,
  Plans p,
  Original io, {
  PluginLibraryEntry? entry,
  Future<XFile?> Function()? picker,
  bool recovery = false,
  bool chinese = false,
}) async {
  tester.view.resetPhysicalSize();
  tester.view.physicalSize = const Size(1200, 1000);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.resetPhysicalSize);
  addTearDown(tester.view.resetDevicePixelRatio);
  await tester.pumpWidget(
    _page(
      p,
      io,
      entry: entry,
      picker: picker,
      recovery: recovery,
      chinese: chinese,
    ),
  );
  await _click(tester, 'open');
}

Future<void> _proposal(
  WidgetTester tester,
  Plans p, {
  String input = '  Original 😀\nsecond line  ',
}) async {
  await tester.enterText(find.byKey(const ValueKey('blob-plan-input')), input);
  await _click(tester, 'blob-plan-start');
  expect(p.calls.where((c) => c == 'start').length, 1);
  expect(p.calls.contains('approve'), isFalse);
  p.joined = true;
  await _click(tester, 'blob-plan-refresh');
  await _click(tester, 'blob-plan-read');
}

Future<void> _approve(WidgetTester tester) async {
  expect(_enabled(tester, 'blob-plan-approve'), isFalse);
  await _click(tester, 'blob-plan-reviewed');
  expect(_enabled(tester, 'blob-plan-approve'), isTrue);
  await _click(tester, 'blob-plan-approve');
}

Future<void> _result(WidgetTester tester, Original io) async {
  io.ready = true;
  await _click(tester, 'blob-plan-refresh');
  await _click(tester, 'blob-upload-read');
}

final class _GrowingFile extends XFile {
  _GrowingFile() : super('unused');
  @override
  String get name => 'input.bin';
  @override
  Future<int> length() async => 1;
  @override
  Stream<Uint8List> openRead([int? start, int? end]) => Stream.fromIterable([
    Uint8List.fromList([1, 2, 3, 4]),
    Uint8List.fromList([5, 6, 7, 8, 9]),
  ]);
}

class _LibraryBackend extends catalog_fixture.FakeBackend
    implements BlobPlanBackend, BlobUploadBackend, WorkbenchIoTaskControl {
  _LibraryBackend() : super([_entry(approved: false)]);
  int approvals = 0, unexpected = 0;
  bool lost = false;
  @override
  Future<void> approveBlobUpload(
    String packageId,
    Uint8List digest,
    BigInt revision,
    bool approved,
  ) async {
    expect(packageId, entries.single.id);
    expect(digest, entries.single.digest);
    expect(revision, this.revision);
    approvals++;
    entries = [_entry(approved: approved)];
    this.revision += BigInt.one;
    if (lost) throw StateError('private-details-must-not-be-rendered');
  }

  @override
  Future<IoTaskSnapshot> ioStatus() async => IoTaskSnapshot(
    storage: IoStoragePhase.local,
    delivery: IoDeliveryPhase.absent,
    exit: null,
  );
  @override
  dynamic noSuchMethod(Invocation invocation) {
    unexpected++;
    throw StateError('No operation is authorized merely by opening a view');
  }
}

void main() {
  testWidgets(
    'review and explicit approval preserve exact original input and end both records',
    (tester) async {
      final io = Original(), p = Plans(io);
      await _mount(tester, p, io);
      await _proposal(tester, p);
      expect(p.original!.input, utf8.encode('  Original 😀\nsecond line  '));
      expect(find.text('Description supplied by the plugin'), findsOneWidget);
      expect(find.text('Sample'), findsOneWidget);
      expect(find.text('Destination: current library'), findsOneWidget);
      expect(_enabled(tester, 'blob-plan-approve'), isFalse);
      expect(io.calls.contains('start'), isFalse);
      await _approve(tester);
      expect(p.calls.where((c) => c == 'approve').length, 1);
      expect(find.byKey(const ValueKey('blob-plan-approve')), findsNothing);
      expect(_enabled(tester, 'blob-plan-finish'), isFalse);
      await _result(tester, io);
      expect(find.text('Content saved.'), findsOneWidget);
      await _click(tester, 'blob-plan-finish');
      expect(io.calls.where((c) => c == 'acknowledge').length, 1);
      expect(p.calls.where((c) => c == 'ack').length, 1);
      expect(
        BlobUploadSession.forBackend(io, io).history.single.result!.unknown,
        isFalse,
      );
      expect(
        find.text('1 record(s) completed in this session.'),
        findsOneWidget,
      );
      await _click(tester, 'blob-plan-close');
      expect(io.calls.contains('cancel'), isFalse);
    },
  );
  testWidgets(
    'lost approval response discovers original upload without a second approval or start',
    (tester) async {
      final io = Original(), p = Plans(io)..loseApprove = true;
      await _mount(tester, p, io);
      await _proposal(tester, p);
      await _approve(tester);
      expect(find.text('Original upload task'), findsOneWidget);
      await _click(tester, 'blob-plan-refresh');
      expect(p.calls.where((c) => c == 'approve').length, 1);
      expect(io.calls.contains('start'), isFalse);
      await _click(tester, 'blob-plan-close');
      expect(io.calls.where((c) => c == 'cancel').length, 1);
      await _click(tester, 'open');
      expect(find.byKey(const ValueKey('blob-plan-approve')), findsNothing);
      expect(p.calls.where((c) => c == 'start').length, 1);
      await _click(tester, 'blob-plan-close');
      expect(io.calls.where((c) => c == 'cancel').length, 1);
    },
  );
  testWidgets('lost proposal cannot be approved or read twice', (tester) async {
    final io = Original(), p = Plans(io)..loseRead = true;
    await _mount(tester, p, io);
    await _proposal(tester, p);
    expect(
      find.text(
        'The original proposal was not received. This save cannot be approved.',
      ),
      findsOneWidget,
    );
    expect(find.byKey(const ValueKey('blob-plan-approve')), findsNothing);
    await _click(tester, 'blob-plan-refresh');
    expect(_enabled(tester, 'blob-plan-read'), isFalse);
    await _click(tester, 'blob-plan-close');
    await _click(tester, 'open');
    expect(p.calls.where((c) => c == 'read').length, 1);
    expect(p.calls.contains('approve'), isFalse);
    expect(io.active, isFalse);
    await _click(tester, 'blob-plan-finish');
    await _click(tester, 'blob-plan-close');
  });
  testWidgets(
    'lost result needs explicit history review without inventing a successful result',
    (tester) async {
      final io = Original()..loseRead = true, p = Plans(io);
      await _mount(tester, p, io);
      await _proposal(tester, p);
      await _approve(tester);
      await _result(tester, io);
      expect(find.text('Content saved.'), findsNothing);
      expect(_enabled(tester, 'blob-plan-finish'), isFalse);
      expect(_enabled(tester, 'blob-upload-confirm'), isFalse);
      await _click(tester, 'blob-upload-inspect');
      expect(
        find.text('Original history contains saved content'),
        findsOneWidget,
      );
      expect(_enabled(tester, 'blob-plan-finish'), isFalse);
      await _click(tester, 'blob-upload-confirm');
      expect(BlobUploadSession.forBackend(io, io).result, isNull);
      await _click(tester, 'blob-plan-finish');
      expect(
        BlobUploadSession.forBackend(io, io).history.single.result,
        isNull,
      );
      expect(io.calls.where((c) => c == 'read').length, 1);
      await _click(tester, 'blob-plan-close');
    },
  );
  testWidgets(
    'missing finish response stays unresolved across window replacement',
    (tester) async {
      final io = Original()..loseAck = true, p = Plans(io);
      await _mount(tester, p, io);
      await _proposal(tester, p);
      await _approve(tester);
      await _result(tester, io);
      await _click(tester, 'blob-plan-finish');
      expect(_enabled(tester, 'blob-plan-finish'), isFalse);
      expect(
        find.textContaining('The finish reply is missing.'),
        findsOneWidget,
      );
      await _click(tester, 'blob-plan-close');
      await _click(tester, 'open');
      expect(_enabled(tester, 'blob-plan-finish'), isFalse);
      expect(find.byKey(const ValueKey('blob-plan-start')), findsNothing);
      expect(io.calls.where((c) => c == 'acknowledge').length, 1);
      expect(p.calls.contains('ack'), isFalse);
      expect(io.calls.contains('cancel'), isFalse);
      await _click(tester, 'blob-plan-close');
    },
  );
  testWidgets('closing during approval cancels only its original upload once', (
    tester,
  ) async {
    final io = Original(), p = Plans(io)..approvalWait = Completer<void>();
    await _mount(tester, p, io);
    await _proposal(tester, p);
    await _click(tester, 'blob-plan-reviewed');
    await tester.ensureVisible(find.byKey(const ValueKey('blob-plan-approve')));
    await tester.tap(find.byKey(const ValueKey('blob-plan-approve')));
    await tester.pump();
    expect(p.calls.where((c) => c == 'approve').length, 1);
    await tester.tap(find.byKey(const ValueKey('blob-plan-close')));
    await tester.pumpAndSettle();
    p.approvalWait!.complete();
    await tester.pumpAndSettle();
    expect(io.calls.where((c) => c == 'cancel').length, 1);
    expect(io.calls.contains('start'), isFalse);
    expect(tester.takeException(), isNull);
  });
  testWidgets('UTF8 byte bound is enforced before starting a plan', (
    tester,
  ) async {
    final io = Original(), p = Plans(io);
    await _mount(tester, p, io, entry: _entry(limit: 8));
    await tester.enterText(
      find.byKey(const ValueKey('blob-plan-input')),
      '😀😀😀',
    );
    await _click(tester, 'blob-plan-start');
    expect(p.calls, isEmpty);
    expect(find.text('Input exceeds the plugin limit.'), findsOneWidget);
    await _click(tester, 'blob-plan-close');
  });
  testWidgets(
    'binary input is bounded while reading and frozen before planning',
    (tester) async {
      final io = Original(),
          p = Plans(io),
          data = Uint8List.fromList([0, 255, 9, 8]);
      int pick = 0;
      await _mount(
        tester,
        p,
        io,
        entry: _entry(type: 'example.binary', limit: 8),
        picker: () async => pick++ == 0
            ? _GrowingFile()
            : XFile.fromData(data, path: 'business.data'),
      );
      await _chooseRealInput(tester);
      expect(find.text('Input exceeds the plugin limit.'), findsOneWidget);
      expect(p.calls, isEmpty);
      await _chooseRealInput(tester);
      expect(find.text('business.data'), findsOneWidget);
      data[0] = 100;
      await _click(tester, 'blob-plan-start');
      expect(p.original!.input, [0, 255, 9, 8]);
      expect(p.original!.inputType, 'example.binary');
      await _click(tester, 'blob-plan-close');
      expect(p.calls.where((c) => c == 'cancel').length, 1);
    },
  );
  testWidgets(
    'category denial and global recovery view never start a new task',
    (tester) async {
      final io = Original(), p = Plans(io);
      await _mount(tester, p, io, entry: _entry(approved: false));
      expect(_enabled(tester, 'blob-plan-start'), isFalse);
      await _click(tester, 'blob-plan-close');
      await tester.pumpWidget(const SizedBox.shrink());
      await tester.pumpWidget(_page(p, io, recovery: true, chinese: true));
      await _click(tester, 'open');
      expect(find.text('保存插件内容'), findsOneWidget);
      expect(find.byKey(const ValueKey('blob-plan-start')), findsNothing);
      expect(p.calls, isEmpty);
      expect(io.calls.contains('start'), isFalse);
      await _click(tester, 'blob-plan-close');
    },
  );
  testWidgets(
    'backend replacement cannot display a delayed proposal from its predecessor',
    (tester) async {
      final io = Original(), p = Plans(io)..readWait = Completer<void>();
      final nextIo = Original(), next = Plans(nextIo);
      await tester.pumpWidget(_page(p, io, direct: true));
      await tester.pumpAndSettle();
      await tester.enterText(
        find.byKey(const ValueKey('blob-plan-input')),
        'old content',
      );
      await _click(tester, 'blob-plan-start');
      p.joined = true;
      await _click(tester, 'blob-plan-refresh');
      await tester.ensureVisible(find.byKey(const ValueKey('blob-plan-read')));
      await tester.tap(find.byKey(const ValueKey('blob-plan-read')));
      await tester.pump();
      await tester.pumpWidget(_page(next, nextIo, direct: true));
      await tester.pumpAndSettle();
      p.readWait!.complete();
      await tester.pumpAndSettle();
      expect(find.text('Sample'), findsNothing);
      expect(next.calls, isEmpty);
      expect(p.calls.where((c) => c == 'cancel').length, 1);
      expect(BlobPlanSession.forBackend(p, io).result, isNull);
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox.shrink());
    },
  );
  for (final loseReply in [false, true]) {
    testWidgets(
      'library category approval is explicit and rereads its revision; lost=$loseReply',
      (tester) async {
        final backend = _LibraryBackend()..lost = loseReply;
        await catalog_fixture.mount(tester, backend);
        await catalog_fixture.click(tester, 'plugin-entry-org.example.planner');
        expect(backend.approvals, 0);
        final start = find.byKey(
          const ValueKey('plugin-blob-plan-org.example.planner'),
        );
        expect(tester.widget<OutlinedButton>(start).onPressed, isNull);
        await catalog_fixture.click(
          tester,
          'plugin-blob-allow-org.example.planner',
        );
        expect(backend.approvals, 1);
        if (loseReply) {
          expect(find.textContaining('private-details'), findsNothing);
          await catalog_fixture.click(tester, 'plugin-refresh');
        }
        expect(
          tester
              .widget<CheckboxListTile>(
                find.byKey(
                  const ValueKey('plugin-blob-allow-org.example.planner'),
                ),
              )
              .value,
          isTrue,
        );
        await catalog_fixture.click(
          tester,
          'plugin-blob-plan-org.example.planner',
        );
        expect(find.byType(BlobPlanManager), findsOneWidget);
        expect(
          tester
              .widget<BlobPlanManager>(find.byType(BlobPlanManager))
              .registryRevision,
          backend.revision,
        );
        expect(backend.unexpected, 0);
        await _click(tester, 'blob-plan-close');
        expect(backend.approvals, 1);
        await catalog_fixture.click(tester, 'plugin-blob-original');
        expect(find.byKey(const ValueKey('blob-plan-start')), findsNothing);
        await _click(tester, 'blob-plan-close');
        expect(backend.unexpected, 0);
      },
    );
  }
  testWidgets(
    'library owner replacement removes only its original upload dialog',
    (tester) async {
      final first = _LibraryBackend(), next = _LibraryBackend();
      await catalog_fixture.mount(tester, first);
      await catalog_fixture.click(tester, 'plugin-blob-original');
      expect(find.byType(BlobPlanManager), findsOneWidget);
      await tester.pumpWidget(catalog_fixture.page(next));
      await tester.pumpAndSettle();
      expect(find.byType(BlobPlanManager), findsNothing);
      expect(first.unexpected, 0);
      expect(next.unexpected, 0);
      await catalog_fixture.click(tester, 'plugin-blob-original');
      expect(find.byType(BlobPlanManager), findsOneWidget);
      await _click(tester, 'blob-plan-close');
      expect(tester.takeException(), isNull);
    },
  );
}
