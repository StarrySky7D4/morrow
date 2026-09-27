import 'dart:io';
import 'dart:typed_data';

import 'package:crypto/crypto.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/file_task_models.dart';
import 'package:morrow_studio/plugins/file_task_session.dart';
import 'package:morrow_studio/plugins/io_task_models.dart';
import 'package:morrow_studio/plugins/plugin_library.dart';
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

Future<(RustWorkbench, PluginLibraryEntry, BigInt)> _approvedBackend(
  Directory directory,
  String executable,
  String builtin,
  String fixture,
) async {
  final backend = await RustWorkbench.open(
    executable: executable,
    package: builtin,
    directory: directory,
    managed: true,
  );
  try {
    final preview = await backend.inspectPlugin(fixture);
    final candidate = preview.entries.single;
    expect(candidate.declaredIo, contains('file-read'));
    expect(candidate.ioHandlers, contains('file.read-selected'));
    await backend.importPlugin(fixture, candidate.digest, preview.revision);
    var catalog = await entireCatalog(backend);
    var plugin = catalog.entries.singleWhere((e) => e.id == candidate.id);
    await backend.configureExternalIo(plugin, catalog.revision, ['file-read']);
    catalog = await entireCatalog(backend);
    plugin = catalog.entries.singleWhere((e) => e.id == candidate.id);
    await backend.configureExternal(plugin, catalog.revision, [], true);
    catalog = await entireCatalog(backend);
    plugin = catalog.entries.singleWhere((e) => e.id == candidate.id);
    expect(plugin.approvedIo, contains('file-read'));
    expect(plugin.enabled, isTrue);
    return (backend, plugin, catalog.revision);
  } catch (_) {
    await backend.close();
    rethrow;
  }
}

FileTaskRequest _request(
  PluginLibraryEntry plugin,
  BigInt revision,
  int attempt,
  File selected,
  int maxBytes,
) => FileTaskRequest(
  submission: Uint8List(32)..[0] = attempt,
  packageId: plugin.id,
  packageDigest: plugin.digest,
  registryRevision: revision,
  handler: 'file.read-selected',
  selectedPath: selected.absolute.path,
  maxBytes: BigInt.from(maxBytes),
  timeoutMs: 30000,
);

// The real host accepts this start. Only its Dart acknowledgement is lost.
// Subsequent calls are forwarded unchanged, so status-based recovery must not
// submit another task to the host.
class _LoseOneStartReply implements FileTaskBackend, WorkbenchIoTaskControl {
  _LoseOneStartReply(this.backend);
  final RustWorkbench backend;
  int starts = 0;
  int statusReads = 0;
  Uint8List? acceptedKey;

  @override
  Future<IoTaskSnapshot> startFile(FileTaskRequest request) async {
    starts++;
    final accepted = await backend.startFile(request);
    acceptedKey = accepted.key;
    throw StateError('simulated lost Dart start reply');
  }

  @override
  Future<IoTaskSnapshot> ioStatus() {
    statusReads++;
    return backend.ioStatus();
  }

  @override
  Future<IoTaskSnapshot> requestFileChunk(
    Uint8List key,
    BigInt offset,
    int limit,
  ) => backend.requestFileChunk(key, offset, limit);
  @override
  Future<IoTaskSnapshot> finishFile(Uint8List key) => backend.finishFile(key);
  @override
  Future<FileTaskRead> readFile(Uint8List key) => backend.readFile(key);
  @override
  Future<IoTaskSnapshot> pollIo(Uint8List key) => backend.pollIo(key);
  @override
  Future<IoTaskSnapshot> cancelIo(Uint8List key) => backend.cancelIo(key);
  @override
  Future<IoTaskSnapshot> repairIo(Uint8List key) => backend.repairIo(key);
  @override
  Future<IoTaskSnapshot> acknowledgeIo(Uint8List key) =>
      backend.acknowledgeIo(key);
  @override
  Future<IoTaskRead> readIo(Uint8List key) => backend.readIo(key);
  @override
  Future<IoTaskSnapshot> startHttp(HttpTaskRequest request) =>
      backend.startHttp(request);
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

  test(
    'real ready file chunk is suppressed by cancellation and owner joins before acknowledgement',
    () async {
      final directory = await Directory.systemTemp.createTemp(
        'morrow-external-file-cancel-',
      );
      RustWorkbench? backend;
      try {
        final source = File('${directory.path}/cancel.bin');
        final bytes = Uint8List.fromList(
          List<int>.generate(90007, (i) => i % 251),
        );
        await source.writeAsBytes(bytes, flush: true);
        final prepared = await _approvedBackend(
          directory,
          executable!,
          builtin!,
          fixture!,
        );
        backend = prepared.$1;
        final key = (await backend.startFile(
          _request(prepared.$2, prepared.$3, 3, source, bytes.length),
        )).key!;
        await _waitFor(
          backend,
          key,
          (s) => s.delivery == IoDeliveryPhase.ready,
        );
        expect((await backend.readFile(key)).result, isA<FileTaskCaptured>());
        await backend.requestFileChunk(key, BigInt.zero, 65536);
        await _waitFor(
          backend,
          key,
          (s) => s.delivery == IoDeliveryPhase.ready,
        );
        await backend.cancelIo(key);
        await expectLater(backend.readFile(key), throwsStateError);
        final joined = await _waitFor(backend, key, (s) => s.exit != null);
        expect(joined.exit!.disconnect, IoJobError.none);
        expect(
          (await backend.acknowledgeIo(key)).storage,
          IoStoragePhase.local,
        );
        expect((await backend.ioStatus()).key, isNull);
        expect((await backend.pluginPage()).revision, isNotNull);
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

  test(
    'real accepted start with lost Dart reply recovers by status once and FileTaskSession verifies to completion',
    () async {
      final directory = await Directory.systemTemp.createTemp(
        'morrow-external-file-lost-start-',
      );
      RustWorkbench? backend;
      try {
        final source = File('${directory.path}/recover.bin');
        final bytes = Uint8List.fromList(
          List<int>.generate(90007, (i) => i % 251),
        );
        await source.writeAsBytes(bytes, flush: true);
        final prepared = await _approvedBackend(
          directory,
          executable!,
          builtin!,
          fixture!,
        );
        backend = prepared.$1;
        final wrapper = _LoseOneStartReply(backend);
        final session = FileTaskSession(
          wrapper,
          wrapper,
          pollInterval: const Duration(milliseconds: 10),
        );
        await session.refresh();
        expect(session.canStart, isTrue);
        await session.start(
          _request(prepared.$2, prepared.$3, 4, source, bytes.length),
        );
        expect(wrapper.starts, 1);
        expect(wrapper.acceptedKey, isNotNull);
        expect(session.phase, FileTaskPhase.unknown);
        expect(session.notice, FileTaskNotice.startUnknown);
        final statusBeforeRecovery = wrapper.statusReads;
        await session.refresh();
        expect(wrapper.statusReads, statusBeforeRecovery + 1);
        expect(wrapper.starts, 1);
        expect(session.ownsTask, isTrue);
        expect(session.phase, FileTaskPhase.capturing);
        await session.verify();
        expect(session.phase, FileTaskPhase.verified);
        expect(session.notice, isNull);
        expect(session.received, BigInt.from(bytes.length));
        expect(session.digest, sha256.convert(bytes).toString());
        expect(session.preview, bytes.sublist(0, 4096));
        expect(wrapper.starts, 1);

        final deadline = DateTime.now().add(const Duration(seconds: 10));
        while (session.snapshot?.exit == null) {
          expect(DateTime.now().isBefore(deadline), isTrue);
          await session.refresh();
          await Future<void>.delayed(const Duration(milliseconds: 10));
        }
        expect(session.canAcknowledge, isTrue);
        await session.acknowledge();
        expect(session.phase, FileTaskPhase.idle);
        expect(session.history.single.phase, FileTaskPhase.verified);
        expect(session.history.single.notice, isNull);
        expect((await backend.ioStatus()).storage, IoStoragePhase.local);
        expect(wrapper.starts, 1);
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
