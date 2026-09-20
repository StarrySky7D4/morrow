import 'dart:io';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';
import 'external_plugin_native_test.dart'
    show entireCatalog, removeTestDirectory;

void main() {
  final executable = Platform.environment['MORROW_WORKBENCH_HOST'];
  final builtin = Platform.environment['MORROW_WORKBENCH_PACKAGE'];
  final fixture = Platform.environment['MORROW_IO_CONTROL_FIXTURE'];
  final unavailable =
      !Platform.isWindows ||
      executable == null ||
      builtin == null ||
      fixture == null;

  test(
    'actual private transport persists IO decisions across process restart without enabling or touching content grants',
    () async {
      final directory = await Directory.systemTemp.createTemp(
        'morrow-external-io-control-',
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
        final preview = await backend.inspectPlugin(fixture!);
        final candidate = preview.entries.single;
        expect(candidate.declaredIo, ['http-request', 'credential-use']);
        expect(candidate.approvedIo, isEmpty);
        expect(candidate.enabled, isFalse);
        await backend.importPlugin(fixture, candidate.digest, preview.revision);
        var page = await entireCatalog(backend);
        var entry = page.entries.singleWhere((e) => e.id == candidate.id);
        final originalContent = List.of(entry.approved);
        await backend.configureExternalIo(entry, page.revision, [
          'http-request',
        ]);
        await expectLater(
          backend.configureExternalIo(entry, page.revision, ['credential-use']),
          throwsA(isA<StateError>()),
        );
        page = await entireCatalog(backend);
        entry = page.entries.singleWhere((e) => e.id == candidate.id);
        expect(entry.approvedIo, ['http-request']);
        expect(entry.approved, originalContent);
        expect(entry.enabled, isFalse);

        await backend.close();
        backend = await open();
        page = await entireCatalog(backend);
        entry = page.entries.singleWhere((e) => e.id == candidate.id);
        expect(entry.approvedIo, ['http-request']);
        expect(entry.enabled, isFalse);
        await backend.configureExternal(
          entry,
          page.revision,
          entry.declared,
          true,
        );
        page = await entireCatalog(backend);
        entry = page.entries.singleWhere((e) => e.id == candidate.id);
        expect(entry.enabled, isTrue);
        expect(entry.approvedIo, ['http-request']);
        final content = List.of(entry.approved);
        await backend.configureExternalIo(entry, page.revision, []);

        await backend.close();
        backend = await open();
        page = await entireCatalog(backend);
        entry = page.entries.singleWhere((e) => e.id == candidate.id);
        expect(entry.approvedIo, isEmpty);
        expect(entry.approved, content);
        expect(entry.enabled, isTrue);
        // These category decisions do not claim a live IO binding or endpoint.
      } finally {
        await backend?.close();
        await removeTestDirectory(directory);
      }
    },
    skip: unavailable,
    timeout: const Timeout(Duration(minutes: 3)),
  );
}
