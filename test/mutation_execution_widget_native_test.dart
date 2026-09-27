// Real controls and host. Only OS picker choices are injected temporary paths.
import 'dart:io';
import 'dart:typed_data';
import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:flutter/foundation.dart' show debugPrintSynchronously;
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:morrow_studio/plugins/mutation_execution_manager.dart';
import 'package:morrow_studio/plugins/mutation_execution_session.dart';
import 'package:morrow_studio/plugins/mutation_task_models.dart';
import 'package:morrow_studio/plugins/plugin_library.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';
import 'external_plugin_native_test.dart'
    show entireCatalog, removeTestDirectory;

Finder _key(String id) => find.byKey(ValueKey('mutation-execution-$id'));
bool _enabled(WidgetTester t, String id) =>
    _key(id).evaluate().isNotEmpty &&
    t.widget<OutlinedButton>(_key(id)).onPressed != null;
Future<void> _wait(WidgetTester t, bool Function() ready, String step) async {
  final clock = Stopwatch()..start();
  while (!ready() && clock.elapsed < const Duration(seconds: 35)) {
    await t.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 20)),
    );
    await t.pump(const Duration(milliseconds: 100));
    final error = t.takeException();
    if (error != null) throw error;
  }
  expect(ready(), isTrue, reason: step);
  await t.pump();
}

Future<void> _click(WidgetTester t, Finder target) async {
  FocusManager.instance.primaryFocus?.unfocus();
  t.testTextInput.hide();
  await t.pump(const Duration(milliseconds: 150));
  await t.ensureVisible(target);
  await t.pump(const Duration(milliseconds: 150));
  await t.tap(target);
  await t.pump(const Duration(milliseconds: 150));
}

Future<void> _tap(WidgetTester t, String id) async {
  await _wait(t, () => _enabled(t, id), '$id enabled');
  await _click(t, _key(id));
}

void main() {
  LiveTestWidgetsFlutterBinding.ensureInitialized();
  final env = Platform.environment;
  final host = env['MORROW_WORKBENCH_HOST'],
      builtin = env['MORROW_WORKBENCH_PACKAGE'],
      fixture = env['MORROW_MUTATION_TASK_PACKAGE'];
  testWidgets(
    'real Create and Delete require reviewed preparation and separate execution confirmations',
    (t) async {
      Directory? directory;
      RustWorkbench? backend;
      late PluginLibraryEntry plugin;
      late BigInt revision;
      late File source, target;
      final content = Uint8List.fromList(
        List.generate(
          MutationTaskValidation.maxChunkBytes * 2 + 19,
          (i) => i % 251,
        ),
      );
      try {
        await t.runAsync(() async {
          directory = await Directory.systemTemp.createTemp(
            'morrow-external-mutation-execution-widget-',
          );
          backend = await RustWorkbench.open(
            executable: host!,
            package: builtin!,
            directory: directory!,
            managed: true,
          );
          final preview = await backend!.inspectPlugin(fixture!);
          final candidate = preview.entries.single;
          await backend!.importPlugin(
            fixture,
            candidate.digest,
            preview.revision,
          );
          var catalog = await entireCatalog(backend!);
          plugin = catalog.entries.singleWhere((p) => p.id == candidate.id);
          await backend!.configureExternalIo(plugin, catalog.revision, [
            'file-create',
            'file-delete',
          ]);
          catalog = await entireCatalog(backend!);
          plugin = catalog.entries.singleWhere((p) => p.id == candidate.id);
          await backend!.configureExternal(plugin, catalog.revision, [], true);
          catalog = await entireCatalog(backend!);
          plugin = catalog.entries.singleWhere((p) => p.id == candidate.id);
          revision = catalog.revision;
          source = File('${directory!.path}${Platform.pathSeparator}source.bin')
            ..writeAsBytesSync(content);
          target = File(
            '${directory!.path}${Platform.pathSeparator}created.bin',
          );
        });
        final active = backend!;
        final session = MutationExecutionSession.forBackend(
          active.mutationTasks,
          active,
        );
        Widget screen() => MaterialApp(
          locale: const Locale('en'),
          localizationsDelegates: AppLocalizations.localizationsDelegates,
          supportedLocales: AppLocalizations.supportedLocales,
          home: Scaffold(
            body: SingleChildScrollView(
              child: MutationExecutionManager(
                backend: active.mutationTasks,
                ioBackend: active,
                plugins: [plugin],
                registryRevision: revision,
                ink: Colors.black,
                muted: Colors.grey,
                line: Colors.grey,
                radius: BorderRadius.circular(8),
                pickDirectory: () async => directory!.path,
                pickFile: () async => XFile(target.path),
                pickContent: () async => XFile(source.path),
              ),
            ),
          ),
        );
        Future<void> package() async {
          await _wait(
            t,
            () =>
                t.widget<DropdownButton<String>>(_key('package')).onChanged !=
                null,
            'package selector ready',
          );
          await _click(t, _key('package'));
          await t.pump(const Duration(milliseconds: 300));
          await _wait(
            t,
            () => find.text(plugin.name).evaluate().isNotEmpty,
            'package choices visible',
          );
          await _click(t, find.text(plugin.name).last);
          await _wait(t, () => _enabled(t, 'pick-target'), 'picker enabled');
        }

        Future<void> closeTask() async {
          await _tap(t, 'release');
          await _wait(t, () => _enabled(t, 'ack'), 'actual worker exit');
          await _tap(t, 'ack');
          await _wait(t, () => session.canStart, 'ack local');
        }

        await t.pumpWidget(screen());
        await _wait(t, () => session.canStart, 'initial local');
        await package();
        await _tap(t, 'pick-target');
        await _tap(t, 'pick-content');
        await t.enterText(_key('file-name'), 'created.bin');
        await _tap(t, 'review');
        await _wait(
          t,
          () => _key('confirm-prepare').evaluate().isNotEmpty,
          'review dialog',
        );
        expect(session.request, isNull);
        expect(target.existsSync(), isFalse);
        expect(find.textContaining(target.path), findsWidgets);
        await _click(t, _key('confirm-cancel'));
        expect(session.request, isNull);
        await _tap(t, 'review');
        await _wait(
          t,
          () => _key('confirm-prepare').evaluate().isNotEmpty,
          'second review dialog',
        );
        await _click(t, _key('confirm-prepare'));
        await _wait(t, () => _enabled(t, 'execute'), 'prepared create');
        expect(session.error, isNull);
        expect(session.stagedBytes, BigInt.from(content.length));
        expect(target.existsSync(), isFalse);
        final original = session.request;
        await t.pumpWidget(const SizedBox.shrink());
        await t.pumpWidget(screen());
        await _wait(
          t,
          () => _enabled(t, 'execute'),
          'remounted prepared create',
        );
        expect(session.request, same(original));
        expect(target.existsSync(), isFalse);
        await _tap(t, 'execute');
        await _wait(
          t,
          () => _key('confirm-execute').evaluate().isNotEmpty,
          'create execution confirmation',
        );
        expect(target.existsSync(), isFalse);
        await _click(t, _key('confirm-execute'));
        await _wait(t, () => session.outcome != null, 'created result');
        expect(session.outcome!.effect, MutationEffect.osSucceeded);
        expect(target.readAsBytesSync(), content);
        await closeTask();

        await _click(t, _key('disposition'));
        await t.pump(const Duration(milliseconds: 300));
        await _click(
          t,
          find
              .text(L10n.forLocale(const Locale('en')).pluginsIoFileDelete)
              .last,
        );
        await t.pump(const Duration(milliseconds: 500));
        await package();
        await _tap(t, 'pick-target');
        await _tap(t, 'review');
        await _wait(
          t,
          () => _key('confirm-prepare').evaluate().isNotEmpty,
          'delete review',
        );
        await _click(t, _key('confirm-prepare'));
        await _wait(t, () => _enabled(t, 'execute'), 'prepared delete');
        expect(target.existsSync(), isTrue);
        await _tap(t, 'execute');
        await _wait(
          t,
          () => _key('confirm-execute').evaluate().isNotEmpty,
          'delete execution confirmation',
        );
        expect(
          find.text(
            L10n.forLocale(
              const Locale('en'),
            ).pluginsFileMutationExecuteDeleteConfirm,
          ),
          findsWidgets,
        );
        await _click(t, _key('confirm-cancel'));
        expect(target.existsSync(), isTrue);
        await _tap(t, 'execute');
        await _wait(
          t,
          () => _key('confirm-execute').evaluate().isNotEmpty,
          'delete confirmation again',
        );
        await _click(t, _key('confirm-execute'));
        await _wait(
          t,
          () => session.outcome?.kind == MutationResultKind.deleted,
          'deleted result',
        );
        expect(session.outcome!.effect, MutationEffect.osSucceeded);
        expect(target.existsSync(), isFalse);
        await closeTask();
        expect(session.error, isNull);
        expect(session.cleanupError, isNull);
      } catch (error, stack) {
        debugPrintSynchronously(
          'Mutation execution native widget failed: $error\n$stack',
        );
        rethrow;
      } finally {
        await t.pumpWidget(const SizedBox.shrink());
        await t.runAsync(() async {
          try {
            if (backend != null) await backend!.close();
          } finally {
            if (directory != null) await removeTestDirectory(directory!);
          }
        });
      }
    },
    skip:
        !Platform.isWindows ||
        host == null ||
        builtin == null ||
        fixture == null,
    timeout: const Timeout(Duration(minutes: 3)),
  );
}
