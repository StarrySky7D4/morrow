import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:morrow_studio/plugins/directory_task_manager.dart';
import 'package:morrow_studio/plugins/directory_task_session.dart';
import 'package:morrow_studio/plugins/plugin_library.dart';
import 'directory_native_picker_test_support.dart';
import 'directory_task_test_support.dart';

Widget page(
  PendingNativeDirectory b, {
  String locale = 'en',
  bool visible = true,
  bool confirmed = true,
  int revision = 1,
  Future<String?> Function()? pathPicker,
}) => MaterialApp(
  locale: Locale(locale),
  localizationsDelegates: AppLocalizations.localizationsDelegates,
  supportedLocales: AppLocalizations.supportedLocales,
  home: Scaffold(
    body: TickerMode(
      enabled: visible,
      child: SingleChildScrollView(
        child: DirectoryTaskManager(
          backend: b,
          ioBackend: b,
          plugins: confirmed
              ? [
                  PluginLibraryEntry(
                    id: 'test.directory',
                    name: 'Folder test',
                    version: '1',
                    digest: directoryRequest().packageDigest,
                    enabled: true,
                    builtin: false,
                    available: true,
                    declared: [],
                    approved: [],
                    dependencies: [],
                    handlers: [],
                    issue: '',
                    directorySupported: true,
                    declaredIo: ['file-list'],
                    approvedIo: ['file-list'],
                    ioHandlers: ['directory.immediate-selected'],
                  ),
                ]
              : [],
          registryRevision: confirmed ? BigInt.from(revision) : null,
          ink: Colors.black,
          muted: Colors.grey,
          line: Colors.grey,
          radius: BorderRadius.circular(8),
          pickDirectory: pathPicker,
        ),
      ),
    ),
  ),
);

Future<void> clickStart(WidgetTester t) async {
  await t.tap(find.byKey(const ValueKey('directory-task-handler')));
  await t.pumpAndSettle();
  await t.tap(find.text('Folder test · directory.immediate-selected').last);
  await t.pumpAndSettle();
  final button = find.byKey(const ValueKey('directory-task-start'));
  await t.ensureVisible(button);
  await t.tap(button);
  await t.pump();
}

void main() {
  testWidgets(
    'native UI starts metadata-only admission without a caller picker',
    (t) async {
      final b = PendingNativeDirectory();
      var externalSelections = 0;
      await t.pumpWidget(
        page(
          b,
          pathPicker: () async {
            externalSelections++;
            return r'C:\untrusted';
          },
        ),
      );
      await t.pumpAndSettle();
      expect(find.byKey(const ValueKey('directory-task-pick')), findsNothing);
      await clickStart(t);
      expect(b.starts, 1);
      expect(b.request.nativePicker, isTrue);
      expect(b.request.selectedPath, isEmpty);
      expect(externalSelections, 0);
      await t.pumpWidget(const SizedBox.shrink());
      b.startReply.complete(b.state);
      await t.pump();
      expect([b.cancels, b.runs], [1, 0]);
    },
  );

  testWidgets(
    'late admission remains with old backend after backend replacement',
    (t) async {
      final old = PendingNativeDirectory(), next = PendingNativeDirectory();
      await t.pumpWidget(page(old));
      await t.pumpAndSettle();
      await clickStart(t);
      await t.pumpWidget(page(next));
      await t.pump();
      old.startReply.complete(old.state);
      await t.pump();
      expect(
        [old.starts, old.cancels, old.runs, next.starts, next.cancels],
        [1, 1, 0, 0, 0],
      );
      expect(DirectoryTaskSession.forBackend(old, old).ownsTask, isTrue);
      await t.pumpWidget(const SizedBox.shrink());
    },
  );

  for (final reason in [
    'catalog unavailable',
    'catalog revision changed',
    'view hidden',
  ]) {
    testWidgets('$reason revokes the same pending native picker once', (
      t,
    ) async {
      final b = PendingNativeDirectory();
      await t.pumpWidget(page(b));
      await t.pumpAndSettle();
      await clickStart(t);
      await t.pumpWidget(
        page(
          b,
          confirmed: reason != 'catalog unavailable',
          revision: reason == 'catalog revision changed' ? 2 : 1,
          visible: reason != 'view hidden',
        ),
      );
      await t.pump();
      b.startReply.complete(b.state);
      await t.pump();
      expect([b.starts, b.cancels, b.runs], [1, 1, 0]);
      await t.pumpWidget(const SizedBox.shrink());
      await t.pump();
      expect(b.cancels, 1);
    });
  }

  testWidgets(
    'locale change retains admission with the same confirmed catalog',
    (t) async {
      final b = PendingNativeDirectory();
      await t.pumpWidget(page(b));
      await t.pumpAndSettle();
      await clickStart(t);
      await t.pumpWidget(page(b, locale: 'zh'));
      await t.pump();
      expect(
        DirectoryTaskSession.forBackend(b, b).phase,
        DirectoryTaskPhase.capturing,
      );
      expect([b.starts, b.cancels], [1, 0]);
      await t.pumpWidget(const SizedBox.shrink());
      b.startReply.complete(b.state);
      await t.pump();
      expect(b.cancels, 1);
    },
  );

  testWidgets('native picker labels fit all nine locales at narrow width', (
    t,
  ) async {
    t.view.physicalSize = const Size(360, 800);
    t.view.devicePixelRatio = 1;
    addTearDown(t.view.resetPhysicalSize);
    addTearDown(t.view.resetDevicePixelRatio);
    final b = PendingNativeDirectory();
    for (final code in ['en', 'zh', 'de', 'es', 'fr', 'ja', 'ko', 'pt', 'ru']) {
      await t.pumpWidget(page(b, locale: code));
      await t.pumpAndSettle();
      final l = L10n.forLocale(Locale(code));
      expect(find.text(l.pluginsDirectoryNativePickerStart), findsOneWidget);
      expect(find.text(l.pluginsDirectoryNativePickerIntro), findsOneWidget);
      expect(t.takeException(), isNull);
    }
    expect(b.starts, 0);
    await t.pumpWidget(const SizedBox.shrink());
  });
}
