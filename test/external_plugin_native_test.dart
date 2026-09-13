import 'dart:io';
import 'dart:typed_data';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_plugin_ui/online.dart';
import 'package:morrow_studio/main.dart' show Idea;
import 'package:morrow_studio/plugins/plugin_library.dart';
import 'package:morrow_studio/plugins/workbench_backend.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';
import 'plugin_tools_native_test.dart' show pumpHost;

Future<PluginLibraryPage> entireCatalog(RustWorkbench backend) async {
  final first = await backend.pluginPage();
  final entries = [...first.entries];
  var cursor = first.cursor;
  final seen = <String>{};
  while (cursor.isNotEmpty) {
    expect(seen.add(cursor), isTrue);
    final page = await backend.pluginPage(
      cursor: cursor,
      revision: first.revision,
    );
    expect(page.revision, first.revision);
    entries.addAll(page.entries);
    cursor = page.cursor;
  }
  expect(entries.map((entry) => entry.id).toSet().length, entries.length);
  return PluginLibraryPage(
    revision: first.revision,
    entries: entries,
    cursor: '',
  );
}

Future<void> removeTestDirectory(Directory directory) async {
  final actual = await directory.resolveSymbolicLinks();
  final temporary = await Directory.systemTemp.resolveSymbolicLinks();
  if (!actual.toLowerCase().startsWith(
    '${temporary.toLowerCase()}${Platform.pathSeparator}morrow-external-',
  )) {
    throw StateError('refusing cleanup outside the generated test directory');
  }
  await Directory(actual).delete(recursive: true);
}

void main() {
  final executable = Platform.environment['MORROW_WORKBENCH_HOST'];
  final builtin = Platform.environment['MORROW_WORKBENCH_PACKAGE'];
  final unavailable =
      !Platform.isWindows || executable == null || builtin == null;
  String fixture(String name) =>
      File('sdk/compat/guest-v1-rc1/$name.mplugin').absolute.path;

  test(
    'actual catalog transport imports pinned packages, binds approval, executes binary transforms and preserves cards',
    () async {
      final directory = await Directory.systemTemp.createTemp(
        'morrow-external-catalog-',
      );
      RustWorkbench? backend;
      Future<RustWorkbench> open() => RustWorkbench.open(
        executable: executable!,
        package: builtin!,
        directory: directory,
        managed: true,
      );
      try {
        backend = await open();
        await backend.apply(
          PluginAction.create,
          Idea(
            '保留的卡片',
            '原始正文',
            '灵感',
            Idea.icons[0],
            const Color(0xff8866aa),
            id: 'keep-card',
          ),
        );
        final initial = await entireCatalog(backend);
        expect(initial.entries.where((entry) => entry.builtin), hasLength(1));
        final changed = File('${directory.path}/selected.mplugin');
        await changed.writeAsBytes(
          await File(fixture('rust-transform')).readAsBytes(),
        );
        final preview = await backend.inspectPlugin(changed.path);
        expect((await entireCatalog(backend)).revision, initial.revision);
        await changed.writeAsBytes(
          await File(fixture('c-transform')).readAsBytes(),
        );
        await expectLater(
          backend.importPlugin(
            changed.path,
            preview.entries.single.digest,
            preview.revision,
          ),
          throwsStateError,
        );
        expect((await entireCatalog(backend)).revision, initial.revision);
        for (final language in ['rust', 'c', 'cpp']) {
          final proposal = await backend.inspectPlugin(
            fixture('$language-transform'),
          );
          await backend.importPlugin(
            fixture('$language-transform'),
            proposal.entries.single.digest,
            proposal.revision,
          );
          var catalog = await entireCatalog(backend);
          var entry = catalog.entries.singleWhere(
            (entry) => entry.id == proposal.entries.single.id,
          );
          expect(entry.enabled, isFalse);
          expect(entry.approved, isEmpty);
          await expectLater(
            backend.configureExternal(entry, initial.revision, [], true),
            throwsStateError,
          );
          await backend.configureExternal(entry, catalog.revision, [], true);
          catalog = await entireCatalog(backend);
          entry = catalog.entries.singleWhere((value) => value.id == entry.id);
          final reverse = entry.handlers.singleWhere(
            (handler) => handler.name == 'bytes.reverse',
          );
          expect(
            await backend.transformExternal(
              entry,
              catalog.revision,
              reverse,
              Uint8List.fromList([0, 255, 65, 90]),
            ),
            [90, 65, 255, 0],
          );
          final reject = entry.handlers.singleWhere(
            (handler) => handler.name == 'bytes.require-ascii',
          );
          await expectLater(
            backend.transformExternal(
              entry,
              catalog.revision,
              reject,
              Uint8List.fromList([255]),
            ),
            throwsStateError,
          );
          expect((await backend.pluginState()).writable, isTrue);
        }
        var catalog = await entireCatalog(backend);
        expect(catalog.entries, hasLength(4));
        final entry = catalog.entries.firstWhere((entry) => !entry.builtin);
        await backend.removeExternal(entry, catalog.revision);
        expect((await backend.load()).single.id, 'keep-card');
        await backend.close();
        backend = await open();
        catalog = await entireCatalog(backend);
        expect(catalog.entries, hasLength(3));
        expect(catalog.entries.any((value) => value.id == entry.id), isFalse);
        expect((await backend.load()).single.description, '原始正文');
        expect((await backend.pluginState()).writable, isTrue);
      } finally {
        await backend?.close();
        await removeTestDirectory(directory);
      }
    },
    skip: unavailable,
    timeout: const Timeout(Duration(minutes: 3)),
  );

  testWidgets(
    'PluginLibrary user flow imports, enables and edits an actual external Wasm form',
    (tester) async {
      Directory? directory;
      RustWorkbench? backend;
      try {
        await tester.runAsync(() async {
          directory = await Directory.systemTemp.createTemp(
            'morrow-external-ui-',
          );
          backend = await RustWorkbench.open(
            executable: executable!,
            package: builtin!,
            directory: directory!,
            managed: true,
          );
        });
        await tester.pumpWidget(
          MaterialApp(
            home: Scaffold(
              body: SingleChildScrollView(
                child: PluginLibrary(
                  backend: backend!,
                  onChanged: () {},
                  ink: Colors.black,
                  muted: Colors.grey,
                  line: Colors.grey,
                  radius: BorderRadius.circular(11),
                  pickPackage: () async => fixture('rust-ui'),
                ),
              ),
            ),
          ),
        );
        Future<void> settled() async {
          await pumpHost(
            tester,
            () => find.byType(LinearProgressIndicator).evaluate().isEmpty,
          );
          await tester.pumpAndSettle();
        }

        Future<void> tap(String key) async {
          final target = find.byKey(ValueKey(key));
          await tester.ensureVisible(target);
          await tester.tap(target);
          await tester.pump();
          await settled();
        }

        await settled();
        await tap('plugin-pick');
        expect(find.byKey(const ValueKey('plugin-preview')), findsOneWidget);
        await tester.runAsync(() async {
          expect((await entireCatalog(backend!)).entries, hasLength(1));
        });
        await tap('plugin-import');
        const id = 'org.morrow.compat.rust.ui';
        await tap('plugin-entry-$id');
        await tap('plugin-approve-$id');
        await tap('plugin-ui-$id');
        expect(find.text('插件表单'), findsOneWidget);
        final field = find.byType(TextField);
        await tester.ensureVisible(field);
        await tester.enterText(field, '真实外部表单');
        await tester.pump();
        await settled();
        expect(tester.widget<TextField>(field).controller!.text, '真实外部表单');
        final controller = tester
            .widget<ManagedPluginForm>(find.byType(ManagedPluginForm))
            .controller;
        await pumpHost(tester, () => controller.phase != PluginUiPhase.busy);
        expect(
          controller.phase,
          PluginUiPhase.ready,
          reason: controller.failure?.message,
        );
        expect(controller.serial, BigInt.one);
        expect(controller.revision, BigInt.two);

        await tester.runAsync(() async {
          expect(await backend!.load(), isEmpty);
        });
        await tap('plugin-ui-$id');
        expect(find.text('插件表单'), findsNothing);
        await tap('plugin-disable-$id');
        await tap('plugin-remove-$id');
        await tester.runAsync(() async {
          expect(
            (await entireCatalog(
              backend!,
            )).entries.any((entry) => entry.id == id),
            isFalse,
          );
          expect((await backend!.pluginState()).writable, isTrue);
        });
        expect(tester.takeException(), isNull);
      } finally {
        await tester.pumpWidget(const SizedBox.shrink());
        await tester.runAsync(() async {
          await backend?.close();
          if (directory != null) await removeTestDirectory(directory!);
        });
      }
    },
    skip: unavailable,
    timeout: const Timeout(Duration(minutes: 3)),
  );
}
