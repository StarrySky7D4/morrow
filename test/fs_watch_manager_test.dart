import 'dart:async';
import 'dart:typed_data';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart' hide Fake;
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:morrow_studio/plugins/fs_watch_models.dart';
import 'package:morrow_studio/plugins/fs_watch_session.dart';
import 'package:morrow_studio/plugins/fs_watch_manager.dart';
import 'package:morrow_studio/plugins/http_task_manager.dart';
import 'package:morrow_studio/plugins/endpoint_control.dart';
import 'package:morrow_studio/plugins/plugin_library.dart';
import 'fs_watch_session_checks.dart' show Fake;
import 'fs_watch_wire_checks.dart' show id, request;

PluginLibraryEntry plugin({bool allowed = true}) => PluginLibraryEntry(
  id: 'org.example.watch',
  name: 'Directory observation example',
  version: '1',
  digest: id(13),
  enabled: true,
  builtin: false,
  available: true,
  declared: [],
  approved: [],
  dependencies: [],
  handlers: [],
  issue: '',
  declaredIo: ['file-list'],
  approvedIo: ['file-list'],
  ioHandlers: ['observe'],
  fsWatchSupported: true,
  fsWatchApproved: allowed,
  fsWatchHandlers: ['observe'],
  fsWatchLimits: FsWatchLimits(
    frames: 2,
    wireBytes: BigInt.from(4096),
    windowMs: 29000,
  ),
);

class NoEndpoints implements WorkbenchEndpointControl {
  @override
  Future<EndpointPage> endpointPage({
    Uint8List? after,
    Uint8List? snapshot,
  }) async => EndpointPage(entries: [], snapshot: id(90));
  @override
  Future<StoredEndpoint> saveEndpoint({
    required Uint8List reference,
    required BigInt expectedRevision,
    required BigInt registryRevision,
    required int lifetimeDays,
    required EndpointPolicy policy,
  }) => throw StateError('Endpoint writes forbidden');
  @override
  Future<StoredEndpoint> disableEndpoint(StoredEndpoint expected) =>
      throw StateError('Endpoint writes forbidden');
}

Widget page(
  Fake b, {
  String language = 'en',
  bool visible = true,
  bool approved = true,
  bool http = false,
}) => MaterialApp(
  locale: Locale(language),
  localizationsDelegates: AppLocalizations.localizationsDelegates,
  supportedLocales: AppLocalizations.supportedLocales,
  home: Scaffold(
    body: TickerMode(
      enabled: visible,
      child: SingleChildScrollView(
        child: http
            ? HttpTaskManager(
                backend: b,
                endpointBackend: NoEndpoints(),
                plugins: [plugin()],
                registryRevision: BigInt.one,
                ink: Colors.black,
                muted: Colors.grey,
                line: Colors.grey,
                radius: BorderRadius.circular(8),
                watchSession: FsWatchSession.forBackend(b, b),
              )
            : FsWatchManager(
                backend: b,
                ioBackend: b,
                plugins: [plugin(allowed: approved)],
                registryRevision: BigInt.one,
                ink: Colors.black,
                muted: Colors.grey,
                line: Colors.grey,
                radius: BorderRadius.circular(8),
              ),
      ),
    ),
  ),
);

Finder key(String s) => find.byKey(ValueKey(s));
bool enabled(WidgetTester tester, String s) =>
    tester.widget<OutlinedButton>(key(s)).onPressed != null;
Future<void> press(WidgetTester tester, String s) async {
  await tester.ensureVisible(key(s));
  await tester.tap(key(s));
  await tester.pump();
}

Future<void> fill(WidgetTester tester) async {
  await tester.ensureVisible(key('fs-watch-handler'));
  await tester.tap(key('fs-watch-handler'));
  await tester.pumpAndSettle();
  await tester.tap(find.text('Directory observation example · observe').last);
  await tester.pumpAndSettle();
  await tester.enterText(key('fs-watch-path'), 'D:\\build_cache\\ 原始 😀 ');
  expect(
    tester.widget<DropdownButton<String>>(key('fs-watch-handler')).value,
    isNotNull,
    reason: 'The dropdown must preserve the selected draft.',
  );
  expect(
    tester.widget<TextField>(key('fs-watch-frames')).controller!.text,
    '2',
  );
  expect(
    tester.widget<TextField>(key('fs-watch-bytes')).controller!.text,
    '4096',
  );
  expect(
    tester.widget<TextField>(key('fs-watch-window')).controller!.text,
    '29000',
  );
  expect(enabled(tester, 'fs-watch-start'), isTrue);
}

void main() {
  for (final language in ['en', 'zh']) {
    testWidgets(
      'watch receipt count and byte labels remain distinct in $language',
      (t) async {
        final b = Fake();
        await t.pumpWidget(page(b, language: language));
        await t.pumpAndSettle();
        await fill(t);
        await press(t, 'fs-watch-start');
        await press(t, 'fs-watch-refresh');
        await press(t, 'fs-watch-read');
        final result = FsWatchSession.forBackend(b, b).result!;
        expect(result.deliveries.length, 2);
        expect(result.chargedBytes.toString(), '9007199254740993');
        expect(
          find.text(
            language == 'zh'
                ? '插件接收记录：2 条；已计入数据：9007199254740993 字节。'
                : 'Records received by the plugin: 2; data charged: 9007199254740993 bytes.',
          ),
          findsOneWidget,
        );
        expect(b.calls['start'], 1);
        expect(b.calls['read'], 1);
        expect(b.calls['ack'], isNull);
        expect(t.takeException(), isNull);
      },
    );
  }

  testWidgets('all nine languages fit a narrow view without dispatch', (
    t,
  ) async {
    t.view.physicalSize = const Size(360, 800);
    t.view.devicePixelRatio = 1;
    addTearDown(t.view.resetPhysicalSize);
    addTearDown(t.view.resetDevicePixelRatio);
    final b = Fake();
    for (final language in [
      'zh',
      'en',
      'ru',
      'fr',
      'de',
      'es',
      'ja',
      'ko',
      'pt',
    ]) {
      await t.pumpWidget(page(b, language: language));
      await t.pumpAndSettle();
      expect(
        find.text(L10n.forLocale(Locale(language)).pluginsWatchTitle),
        findsOneWidget,
      );
      expect(t.takeException(), isNull);
    }
    expect(b.calls['start'], isNull);
  });
  for (final entry in {
    'en': 'Plugin limits: 2 records, 4096 bytes, 29000 milliseconds.',
    'zh': '插件上限：2 条记录、4096 字节、29000 毫秒。',
  }.entries) {
    testWidgets(
      '${entry.key} shows plugin frame and byte limits with their correct units',
      (t) async {
        final b = Fake();
        await t.pumpWidget(page(b, language: entry.key));
        await t.pumpAndSettle();
        await fill(t);
        expect(find.text(entry.value), findsOneWidget);
        expect(b.calls['start'], isNull);
      },
    );
  }
  testWidgets('FileList alone cannot select or start the watch declaration', (
    t,
  ) async {
    final b = Fake();
    await t.pumpWidget(page(b, approved: false));
    await t.pumpAndSettle();
    expect(
      t.widget<DropdownButton<String>>(key('fs-watch-handler')).items,
      isEmpty,
    );
    expect(enabled(t, 'fs-watch-start'), isFalse);
    expect(b.calls['start'], isNull);
  });
  testWidgets(
    'invalid limits cannot dispatch and exact path is retained through one read',
    (t) async {
      final b = Fake();
      await t.pumpWidget(page(b));
      await t.pumpAndSettle();
      await fill(t);
      await t.enterText(key('fs-watch-frames'), '3');
      await press(t, 'fs-watch-start');
      expect(b.calls['start'], isNull);
      await t.enterText(key('fs-watch-frames'), '2');
      await press(t, 'fs-watch-start');
      expect(b.calls['start'], 1);
      expect(b.lastRequest!.path, 'D:\\build_cache\\ 原始 😀 ');
      final s = FsWatchSession.forBackend(b, b);
      expect(s.phase, FsWatchSessionPhase.running);
      expect(enabled(t, 'fs-watch-start'), isFalse);
      expect(enabled(t, 'fs-watch-ack'), isFalse);
      await press(t, 'fs-watch-refresh');
      expect(enabled(t, 'fs-watch-read'), isTrue);
      expect(enabled(t, 'fs-watch-ack'), isFalse);
      await press(t, 'fs-watch-read');
      expect(b.calls['read'], 1);
      expect(enabled(t, 'fs-watch-read'), isFalse);
      expect(enabled(t, 'fs-watch-ack'), isTrue);
      await press(t, 'fs-watch-ack');
      expect(b.calls['ack'], 1);
      expect(enabled(t, 'fs-watch-start'), isFalse);
      expect(enabled(t, 'fs-watch-new'), isTrue);
      await press(t, 'fs-watch-new');
      expect(s.history.length, 1);
      expect(b.calls['start'], 1);
      expect(b.calls['read'], 1);
      expect(b.calls['ack'], 1);
    },
  );
  testWidgets(
    'transport Unknown stays sealed across page and language changes',
    (t) async {
      final b = Fake()..fail = 'start';
      await t.pumpWidget(page(b));
      await t.pumpAndSettle();
      await fill(t);
      await press(t, 'fs-watch-start');
      final before = Map.of(b.calls);
      expect(
        FsWatchSession.forBackend(b, b).phase,
        FsWatchSessionPhase.unknown,
      );
      await t.pumpWidget(const SizedBox.shrink());
      await t.pump();
      await t.pumpWidget(page(b, language: 'zh'));
      await t.pumpAndSettle();
      for (final action in [
        'start',
        'refresh',
        'read',
        'cancel',
        'repair',
        'ack',
        'new',
      ]) {
        expect(enabled(t, 'fs-watch-$action'), isFalse);
      }
      expect(b.calls, before);
      expect(t.takeException(), isNull);
    },
  );
  testWidgets(
    'view removal during pending start cancels only the returned original identity',
    (t) async {
      final barrier = Completer<void>();
      final b = Fake()..startBarrier = barrier;
      await t.pumpWidget(page(b));
      await t.pumpAndSettle();
      await fill(t);
      await press(t, 'fs-watch-start');
      expect(b.calls['start'], 1);
      expect(b.calls['cancel'], isNull);
      await t.pumpWidget(const SizedBox.shrink());
      await t.pump();
      barrier.complete();
      await t.pump();
      await t.pump();
      expect(b.calls['cancel'], 1);
      await t.pumpWidget(page(b));
      await t.pump();
      expect(b.calls['start'], 1);
      expect(b.calls['cancel'], 1);
      expect(enabled(t, 'fs-watch-start'), isFalse);
    },
  );
  testWidgets('catalog revocation withdraws the original watch exactly once', (
    t,
  ) async {
    final b = Fake();
    final s = FsWatchSession.forBackend(b, b);
    await s.initialize();
    await s.start(request());
    await t.pumpWidget(page(b));
    await t.pump();
    await t.pumpWidget(page(b, approved: false));
    await t.pump();
    expect(b.calls['cancel'], 1);
    expect(b.calls['start'], 1);
    await t.pumpWidget(page(b, approved: false, visible: false));
    await t.pump();
    expect(b.calls['cancel'], 1);
    expect(enabled(t, 'fs-watch-start'), isFalse);
  });
  testWidgets('generic HTTP controls cannot adopt a running or Unknown watch', (
    t,
  ) async {
    for (final lost in [false, true]) {
      final b = Fake();
      final s = FsWatchSession.forBackend(b, b);
      await s.initialize();
      if (lost) b.fail = 'start';
      await s.start(request());
      final before = Map.of(b.calls);
      await t.pumpWidget(page(b, http: true));
      await t.pump();
      await t.pump(const Duration(seconds: 2));
      for (final action in ['read', 'cancel', 'repair', 'ack', 'start']) {
        expect(key('http-task-$action'), findsNothing);
      }
      expect(b.calls, before);
      await t.pumpWidget(const SizedBox.shrink());
      await t.pump();
    }
  });
}
