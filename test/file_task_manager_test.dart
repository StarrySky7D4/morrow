import 'dart:typed_data';

import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:morrow_studio/plugins/file_task_manager.dart';
import 'package:morrow_studio/plugins/file_task_models.dart';
import 'package:morrow_studio/plugins/io_task_models.dart';
import 'package:morrow_studio/plugins/plugin_library.dart';

class _Files implements FileTaskBackend {
  final starts = <FileTaskRequest>[];

  @override
  Future<IoTaskSnapshot> startFile(FileTaskRequest request) async {
    starts.add(request);
    throw StateError('Unexpected file task submission');
  }

  @override
  dynamic noSuchMethod(Invocation invocation) =>
      throw UnimplementedError(invocation.memberName.toString());
}

class _Io implements WorkbenchIoTaskControl {
  int statuses = 0;

  @override
  Future<IoTaskSnapshot> ioStatus() async {
    statuses++;
    return IoTaskSnapshot(
      storage: IoStoragePhase.local,
      delivery: IoDeliveryPhase.absent,
      exit: null,
    );
  }

  @override
  dynamic noSuchMethod(Invocation invocation) =>
      throw UnimplementedError(invocation.memberName.toString());
}

PluginLibraryEntry _plugin() => PluginLibraryEntry(
  id: 'test.file-ui',
  name: 'File test package',
  version: '1',
  digest: Uint8List.fromList(List.filled(32, 1)),
  enabled: true,
  builtin: false,
  available: true,
  declared: [],
  approved: [],
  dependencies: [],
  handlers: [],
  issue: '',
  declaredIo: const ['file-read'],
  approvedIo: const ['file-read'],
  ioHandlers: const ['test.file-read.v1'],
);

Widget _page(
  String locale,
  _Files files,
  _Io io, {
  Future<XFile?> Function()? pickFile,
}) => MaterialApp(
  locale: Locale(locale),
  localizationsDelegates: AppLocalizations.localizationsDelegates,
  supportedLocales: AppLocalizations.supportedLocales,
  home: Scaffold(
    body: SingleChildScrollView(
      child: FileTaskManager(
        backend: files,
        ioBackend: io,
        plugins: [_plugin()],
        registryRevision: BigInt.one,
        ink: Colors.black,
        muted: Colors.grey,
        line: Colors.grey,
        radius: BorderRadius.circular(8),
        pickFile: pickFile,
      ),
    ),
  ),
);

void main() {
  testWidgets('file task title and picker use each supported language', (
    tester,
  ) async {
    final files = _Files();
    final io = _Io();
    for (final locale in [
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
      await tester.pumpWidget(
        _page(locale, files, io, pickFile: () async => null),
      );
      await tester.pumpAndSettle();
      final l = L10n.forLocale(Locale(locale));
      expect(find.text(l.pluginsFileTaskTitle), findsOneWidget);
      expect(find.text(l.pluginsFileTaskSelectFile), findsOneWidget);
      expect(find.text(l.pluginsFileTaskIdle), findsOneWidget);
    }
    expect(io.statuses, greaterThan(0));
    expect(files.starts, isEmpty);
  });

  testWidgets('cancelled picker keeps selection and does not submit', (
    tester,
  ) async {
    final files = _Files();
    final io = _Io();
    var picks = 0;
    Future<XFile?> pick() async =>
        ++picks == 1 ? XFile(r'C:\test\sample.txt', name: 'sample.txt') : null;

    await tester.pumpWidget(_page('en', files, io, pickFile: pick));
    await tester.pumpAndSettle();
    final pickButton = find.byKey(const ValueKey('file-task-pick'));
    await tester.ensureVisible(pickButton);
    await tester.tap(pickButton);
    await tester.pumpAndSettle();
    expect(
      find.text(
        L10n.forLocale(const Locale('en')).pluginsSelectedFile('sample.txt'),
      ),
      findsOneWidget,
    );

    await tester.ensureVisible(pickButton);
    await tester.tap(pickButton);
    await tester.pumpAndSettle();
    expect(picks, 2);
    expect(
      find.text(
        L10n.forLocale(const Locale('en')).pluginsSelectedFile('sample.txt'),
      ),
      findsOneWidget,
    );
    expect(files.starts, isEmpty);

    await tester.pumpWidget(_page('de', files, io, pickFile: pick));
    await tester.pumpAndSettle();
    expect(
      find.text(
        L10n.forLocale(const Locale('de')).pluginsSelectedFile('sample.txt'),
      ),
      findsOneWidget,
    );
    expect(files.starts, isEmpty);
  });
}
