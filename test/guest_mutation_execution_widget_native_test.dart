// Real Rust SDK guest and native Widget clicks through both separate approvals.
import 'dart:io';
import 'dart:typed_data';

import 'package:crypto/crypto.dart';
import 'package:file_selector/file_selector.dart';
import 'package:flutter/foundation.dart' show debugPrintSynchronously;
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:morrow_studio/plugins/guest_mutation_execution_manager.dart';
import 'package:morrow_studio/plugins/guest_mutation_models.dart';
import 'package:morrow_studio/plugins/plugin_library.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';

import 'external_plugin_native_test.dart'
    show entireCatalog, removeTestDirectory;

Finder _key(String name) => find.byKey(ValueKey('guest-mutation-$name'));

bool _enabled(WidgetTester tester, String name) =>
    _key(name).evaluate().isNotEmpty &&
    tester.widget<OutlinedButton>(_key(name)).onPressed != null;

Future<void> _wait(
  WidgetTester tester,
  bool Function() ready,
  String step,
) async {
  final clock = Stopwatch()..start();
  while (!ready() && clock.elapsed < const Duration(seconds: 40)) {
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 20)),
    );
    await tester.pump(const Duration(milliseconds: 100));
    final error = tester.takeException();
    if (error != null) throw error;
  }
  expect(ready(), isTrue, reason: step);
  await tester.pump();
}

Future<void> _click(WidgetTester tester, Finder target) async {
  FocusManager.instance.primaryFocus?.unfocus();
  tester.testTextInput.hide();
  await tester.pump(const Duration(milliseconds: 150));
  await tester.ensureVisible(target);
  await tester.pump(const Duration(milliseconds: 150));
  await tester.tap(target);
  await tester.pump(const Duration(milliseconds: 150));
}

Future<void> _tap(WidgetTester tester, String name) async {
  await _wait(tester, () => _enabled(tester, name), '$name enabled');
  await _click(tester, _key(name));
}

Uint8List _content() {
  final target = 3 * GuestMutationValidation.maxChunkBytes + 37;
  final bytes = BytesBuilder(copy: false);
  for (var counter = 0; bytes.length < target; counter++) {
    bytes.add(
      sha256.convert(<int>[
        ...'morrow.guest.widget.four-chunks.v1'.codeUnits,
        ...List<int>.generate(8, (i) => (counter >> (8 * i)) & 255),
      ]).bytes,
    );
  }
  return Uint8List.sublistView(bytes.toBytes(), 0, target);
}

void main() {
  LiveTestWidgetsFlutterBinding.ensureInitialized();
  final env = Platform.environment;
  final host = env['MORROW_WORKBENCH_HOST'];
  final builtin = env['MORROW_WORKBENCH_PACKAGE'];
  final fixture = env['MORROW_GUEST_MUTATION_PACKAGE'];
  final language = env['MORROW_GUEST_MUTATION_LANGUAGE'];
  final moduleHash = env['MORROW_GUEST_MUTATION_WASM_SHA256'];
  testWidgets(
    'real Rust guest Widget requires draft, Prepare and Execute confirmations',
    (tester) async {
      expect(language, 'rust');
      expect(moduleHash, matches(RegExp(r'^[0-9a-f]{64}$')));
      for (final path in [host!, builtin!, fixture!]) {
        expect(File(path).existsSync(), isTrue, reason: path);
      }
      Directory? directory;
      RustWorkbench? backend;
      late PluginLibraryEntry plugin;
      late BigInt revision;
      late File source, target;
      final content = _content();
      try {
        await tester.runAsync(() async {
          directory = await Directory.systemTemp.createTemp(
            'morrow-external-real-guest-widget-',
          );
          backend = await RustWorkbench.open(
            executable: host,
            package: builtin,
            directory: directory!,
            managed: true,
          );
          final preview = await backend!.inspectPlugin(fixture);
          final candidate = preview.entries.single;
          expect(candidate.mutationSupported, isTrue);
          expect(
            candidate.mutationBudget?.maxJobBytes,
            BigInt.from(32 * 1024 * 1024),
          );
          expect(
            candidate.mutationBudget?.maxBytes,
            BigInt.from(256 * 1024 * 1024),
          );
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
        Widget screen() => MaterialApp(
          locale: const Locale('en'),
          localizationsDelegates: AppLocalizations.localizationsDelegates,
          supportedLocales: AppLocalizations.supportedLocales,
          home: Scaffold(
            body: SingleChildScrollView(
              child: GuestMutationExecutionManager(
                backend: active.guestMutationTasks,
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
        await tester.pumpWidget(screen());
        await _wait(
          tester,
          () =>
              tester
                  .widget<DropdownButton<String>>(_key('package'))
                  .onChanged !=
              null,
          'package selector',
        );
        await _click(tester, _key('package'));
        await tester.pump(const Duration(milliseconds: 300));
        await _click(tester, find.text(plugin.name).last);
        await _wait(tester, () => _enabled(tester, 'pick-target'), 'picker');
        await tester.enterText(
          _key('job-budget'),
          plugin.mutationBudget!.maxJobBytes.toString(),
        );
        await tester.enterText(
          _key('total-budget'),
          plugin.mutationBudget!.maxBytes.toString(),
        );
        await _tap(tester, 'pick-target');
        await _tap(tester, 'pick-content');
        await tester.enterText(_key('file-name'), 'created.bin');
        await _tap(tester, 'draft');
        await _wait(
          tester,
          () => _key('confirm-draft').evaluate().isNotEmpty,
          'draft dialog',
        );
        expect(target.existsSync(), isFalse);
        await _click(tester, _key('confirm-cancel'));
        expect(target.existsSync(), isFalse);
        await _tap(tester, 'draft');
        await _wait(
          tester,
          () => _key('confirm-draft').evaluate().isNotEmpty,
          'review dialog',
        );
        await _click(tester, _key('confirm-draft'));
        await _wait(
          tester,
          () => _enabled(tester, 'prepare'),
          'canonical plan reviewed',
        );
        expect(target.existsSync(), isFalse);
        await _tap(tester, 'prepare');
        await _wait(
          tester,
          () => _key('confirm-prepare').evaluate().isNotEmpty,
          'Prepare dialog',
        );
        expect(target.existsSync(), isFalse);
        await _click(tester, _key('confirm-cancel'));
        await _wait(
          tester,
          () => _enabled(tester, 'prepare'),
          'Prepare restored after cancellation',
        );
        await _tap(tester, 'prepare');
        await _wait(
          tester,
          () => _key('confirm-prepare').evaluate().isNotEmpty,
          'Prepare again',
        );
        await _click(tester, _key('confirm-prepare'));
        await _wait(
          tester,
          () => _enabled(tester, 'execute'),
          'durable four chunks',
        );
        expect(target.existsSync(), isFalse, reason: 'Prepare cannot create');
        await _tap(tester, 'execute');
        await _wait(
          tester,
          () => _key('confirm-execute').evaluate().isNotEmpty,
          'Execute dialog',
        );
        expect(target.existsSync(), isFalse);
        await _click(tester, _key('confirm-cancel'));
        expect(target.existsSync(), isFalse);
        await _tap(tester, 'execute');
        await _wait(
          tester,
          () => _key('confirm-execute').evaluate().isNotEmpty,
          'Execute again',
        );
        await _click(tester, _key('confirm-execute'));
        await _wait(tester, () => target.existsSync(), 'observed Create');
        expect(target.readAsBytesSync(), content);
        await _tap(tester, 'query');
        await _wait(
          tester,
          () => _enabled(tester, 'release'),
          'observed Query',
        );
        await _tap(tester, 'release');
        await _wait(
          tester,
          () => _enabled(tester, 'ack'),
          'worker actually exited',
        );
        await _tap(tester, 'ack');
        await _wait(
          tester,
          () => _enabled(tester, 'draft'),
          'ACK restored local state',
        );
        expect(_key('operation-error'), findsNothing);
        expect(_key('cleanup-error'), findsNothing);
        // ignore: avoid_print
        print('GUEST_WIDGET_PASS=rust');
      } catch (error, stack) {
        debugPrintSynchronously('Real guest Widget failed: $error\n$stack');
        rethrow;
      } finally {
        await tester.pumpWidget(const SizedBox.shrink());
        await tester.runAsync(() async {
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
        fixture == null ||
        language == null ||
        moduleHash == null,
    timeout: const Timeout(Duration(minutes: 4)),
  );
}
