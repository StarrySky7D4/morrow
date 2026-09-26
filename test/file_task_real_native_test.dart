import 'dart:io';
import 'dart:typed_data';

import 'package:crypto/crypto.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/file_task_models.dart';
import 'package:morrow_studio/plugins/io_task_models.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';

import 'external_plugin_native_test.dart'
    show entireCatalog, removeTestDirectory;

Future<IoTaskSnapshot> _waitFor(
  RustWorkbench backend,
  Uint8List key,
  bool Function(IoTaskSnapshot) ready,
) async {
  final deadline = DateTime.now().add(const Duration(seconds: 10));
  while (true) {
    final state = await backend.pollIo(key);
    if (ready(state)) return state;
    expect(
      DateTime.now().isBefore(deadline),
      isTrue,
      reason: 'native file task did not reach its expected state',
    );
    await Future<void>.delayed(const Duration(milliseconds: 10));
  }
}

void main() {
  final executable = Platform.environment['MORROW_WORKBENCH_HOST'];
  final builtin = Platform.environment['MORROW_WORKBENCH_PACKAGE'];
  final fixture = Platform.environment['MORROW_FILE_TASK_PACKAGE'];
  test(
    'real Windows host captures selected temp file, serves exact chunks and digest, reclaims owner, and denies revoked file-read',
    () async {
      final directory = await Directory.systemTemp.createTemp(
        'morrow-external-file-task-',
      );
      RustWorkbench? backend;
      try {
        final source = File('${directory.path}/selected.bin');
        final expected = Uint8List.fromList(
          List<int>.generate(90007, (i) => i % 251),
        );
        await source.writeAsBytes(expected, flush: true);
        backend = await RustWorkbench.open(
          executable: executable!,
          package: builtin!,
          directory: directory,
          managed: true,
        );
        final preview = await backend.inspectPlugin(fixture!);
        final candidate = preview.entries.single;
        expect(candidate.declaredIo, contains('file-read'));
        expect(candidate.ioHandlers, contains('file.read-selected'));
        await backend.importPlugin(fixture, candidate.digest, preview.revision);
        var catalog = await entireCatalog(backend);
        var plugin = catalog.entries.singleWhere((e) => e.id == candidate.id);
        await backend.configureExternalIo(plugin, catalog.revision, [
          'file-read',
        ]);
        catalog = await entireCatalog(backend);
        plugin = catalog.entries.singleWhere((e) => e.id == candidate.id);
        await backend.configureExternal(plugin, catalog.revision, [], true);
        catalog = await entireCatalog(backend);
        plugin = catalog.entries.singleWhere((e) => e.id == candidate.id);
        expect(plugin.approvedIo, contains('file-read'));
        expect(plugin.enabled, isTrue);

        FileTaskRequest request(int attempt, String selectedPath) =>
            FileTaskRequest(
              submission: Uint8List(32)..[0] = attempt,
              packageId: plugin.id,
              packageDigest: plugin.digest,
              registryRevision: catalog.revision,
              handler: 'file.read-selected',
              selectedPath: selectedPath,
              maxBytes: BigInt.from(expected.length),
              timeoutMs: 10000,
            );

        final started = await backend.startFile(
          request(1, source.absolute.path),
        );
        final key = started.key!;
        expect(started.submission, request(1, source.absolute.path).submission);
        await _waitFor(
          backend,
          key,
          (s) => s.delivery == IoDeliveryPhase.ready,
        );
        final captured = (await backend.readFile(key)).result;
        expect(captured, isA<FileTaskCaptured>());
        final metadata = captured! as FileTaskCaptured;
        expect(metadata.length, BigInt.from(expected.length));
        expect(metadata.sha256, sha256.convert(expected).bytes);

        await source.delete();
        expect(await source.exists(), isFalse);
        final received = BytesBuilder(copy: false);
        for (final offset in [0, 65536]) {
          await backend.requestFileChunk(key, BigInt.from(offset), 65536);
          await _waitFor(
            backend,
            key,
            (s) => s.delivery == IoDeliveryPhase.ready,
          );
          final chunk = (await backend.readFile(key)).result;
          expect(chunk, isA<FileTaskChunk>());
          final part = chunk! as FileTaskChunk;
          expect(part.offset, BigInt.from(offset));
          expect(
            part.bytes.length,
            offset == 0 ? 65536 : expected.length - offset,
          );
          expect(part.eof, offset != 0);
          received.add(part.bytes);
        }
        final bytes = received.takeBytes();
        expect(bytes, expected);
        expect(sha256.convert(bytes).bytes, metadata.sha256);

        await backend.finishFile(key);
        await _waitFor(
          backend,
          key,
          (s) => s.delivery == IoDeliveryPhase.ready,
        );
        expect((await backend.readFile(key)).result, isA<FileTaskFinished>());
        final joined = await _waitFor(backend, key, (s) => s.exit != null);
        expect(joined.exit!.disconnect, IoJobError.none);
        final reclaimed = await backend.acknowledgeIo(key);
        expect(reclaimed.storage, IoStoragePhase.local);
        expect((await backend.ioStatus()).key, isNull);

        final denied = File('${directory.path}/denied.bin');
        await denied.writeAsBytes(expected, flush: true);
        catalog = await entireCatalog(backend);
        plugin = catalog.entries.singleWhere((e) => e.id == candidate.id);
        await backend.configureExternalIo(plugin, catalog.revision, []);
        catalog = await entireCatalog(backend);
        plugin = catalog.entries.singleWhere((e) => e.id == candidate.id);
        expect(plugin.approvedIo, isEmpty);
        await expectLater(
          backend.startFile(request(2, denied.absolute.path)),
          throwsStateError,
        );
        expect((await backend.ioStatus()).key, isNull);
        expect(await denied.readAsBytes(), expected);
      } finally {
        await backend?.close();
        await removeTestDirectory(directory);
      }
    },
    skip:
        !Platform.isWindows ||
        executable == null ||
        builtin == null ||
        fixture == null,
    timeout: const Timeout(Duration(minutes: 2)),
  );
}
