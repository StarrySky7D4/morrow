import 'dart:async';
import 'dart:typed_data';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:morrow_studio/plugins/directory_task_manager.dart';
import 'package:morrow_studio/plugins/directory_task_session.dart';
import 'package:morrow_studio/plugins/plugin_library.dart';
import 'directory_task_test_support.dart';

PluginLibraryEntry plugin() => PluginLibraryEntry(
  id: 'test.directory',
  name: 'Folder test',
  version: '1',
  digest: Uint8List.fromList(List.filled(32, 3)),
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
);
Widget page(
  DirectoryFake backend, {
  String locale = 'en',
  Future<String?> Function()? picker,
  bool visible = true,
}) => MaterialApp(
  locale: Locale(locale),
  localizationsDelegates: AppLocalizations.localizationsDelegates,
  supportedLocales: AppLocalizations.supportedLocales,
  home: Scaffold(
    body: TickerMode(
      enabled: visible,
      child: SingleChildScrollView(
        child: DirectoryTaskManager(
          backend: backend,
          ioBackend: backend,
          plugins: [plugin()],
          registryRevision: BigInt.one,
          ink: Colors.black,
          muted: Colors.grey,
          line: Colors.grey,
          radius: BorderRadius.circular(8),
          pickDirectory: picker,
        ),
      ),
    ),
  ),
);
void main() {
  testWidgets(
    'folder picker and labels render in every supported language at narrow width',
    (tester) async {
      tester.view.physicalSize = const Size(360, 800);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      final b = DirectoryFake();
      for (final code in [
        'en',
        'zh',
        'de',
        'es',
        'fr',
        'ja',
        'ko',
        'pt',
        'ru',
      ]) {
        await tester.pumpWidget(
          page(b, locale: code, picker: () async => null),
        );
        await tester.pumpAndSettle();
        final l = L10n.forLocale(Locale(code));
        expect(find.text(l.pluginsDirectoryTitle), findsOneWidget);
        expect(find.text(l.pluginsFileMutationPickDirectory), findsOneWidget);
        expect(tester.takeException(), isNull);
      }
      expect(b.starts, 0);
    },
  );
  testWidgets('late picker reply cannot cross a backend change', (
    tester,
  ) async {
    final first = DirectoryFake(),
        second = DirectoryFake(),
        pending = Completer<String?>();
    await tester.pumpWidget(page(first, picker: () => pending.future));
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const ValueKey('directory-task-pick')));
    await tester.pump();
    await tester.pumpWidget(page(second, picker: () async => null));
    await tester.pump();
    pending.complete(r'C:\late-selection');
    await tester.pumpAndSettle();
    expect(find.text(r'C:\late-selection'), findsNothing);
    expect(first.starts, 0);
    expect(second.starts, 0);
  });
  testWidgets(
    'page removal and locale changes retain the same original directory attempt',
    (tester) async {
      final b = DirectoryFake();
      final session = DirectoryTaskSession.forBackend(b, b);
      await session.refresh();
      await session.start(directoryRequest());
      await tester.pumpWidget(page(b));
      await tester.pump();
      await tester.pumpWidget(page(b, locale: 'zh', visible: false));
      await tester.pump();
      await tester.pumpWidget(const SizedBox.shrink());
      await tester.pump();
      expect(identical(DirectoryTaskSession.forBackend(b, b), session), isTrue);
      expect(session.attempt, isNotNull);
      expect(session.ownsTask, isTrue);
      expect([b.starts, b.runs, b.cancels], [1, 0, 0]);
    },
  );
}
