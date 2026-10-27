// Actual production Dart client, supervised native host and original SDK Wasm.
// Synthetic producer data; this does not stand in for rendered approval UI.
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';
import 'package:crypto/crypto.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/blob_upload_models.dart';
import 'package:morrow_studio/plugins/io_task_models.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';
import 'package:morrow_studio/plugins/workbench_channel_supervised.dart';
import 'external_plugin_native_test.dart' show entireCatalog;

Uint8List _id(int n) => Uint8List.fromList(List.filled(32, n));
String _environment(String name) =>
    Platform.environment[name] ??
    (throw StateError('Explicit qualification input missing: $name'));
Future<void> _pin(String path, String expected) async {
  expect(sha256.convert(await File(path).readAsBytes()).toString(), expected);
}

Future<void> _record(Directory directory, String name, Object value) async {
  final file = File('${directory.path}/$name.json');
  expect(
    file.existsSync(),
    isFalse,
    reason: 'Never replace previous qualification evidence',
  );
  await file.writeAsString(jsonEncode(value));
}

Future<RustWorkbench> _open(Directory directory, String stage) async {
  final backend = await RustWorkbench.open(
    executable: _environment('MORROW_WORKBENCH_HOST'),
    supervisorExecutable: _environment('MORROW_WORKBENCH_SUPERVISOR'),
    package: _environment('MORROW_WORKBENCH_PACKAGE'),
    directory: directory,
    managed: true,
  );
  final channel = backend.channel as SupervisedWorkbenchChannel;
  await _record(directory, '$stage-open', channel.supervision.value!.record);
  expect(channel.supervision.value!.phase, 'Running');
  expect(backend.writable, isTrue);
  return backend;
}

Future<void> _close(
  RustWorkbench backend,
  Directory directory,
  String stage,
) async {
  await backend.close();
  expect(await backend.process.exitCode, 0);
  final channel = backend.channel as SupervisedWorkbenchChannel;
  expect(channel.supervision.value!.released, isTrue);
  await _record(directory, '$stage-closed', channel.supervision.value!.record);
}

Future<BlobUploadSnapshot> _wait(
  RustWorkbench backend,
  BlobUploadSnapshot first,
) async {
  var snapshot = first;
  final timer = Stopwatch()..start();
  while (!snapshot.ready) {
    if (timer.elapsed > const Duration(seconds: 125)) {
      throw StateError('Retain original owner: upload did not return');
    }
    await Future<void>.delayed(const Duration(milliseconds: 15));
    snapshot = await backend.statusBlobUpload(first.task.key!);
    expect(snapshot.task.key, first.task.key);
    expect(snapshot.task.submission, first.task.submission);
  }
  expect(snapshot.task.exit, isNotNull);
  expect(snapshot.task.exit!.disconnect, IoJobError.none);
  expect(snapshot.task.exit!.maintenance, IoJobError.none);
  return snapshot;
}

Future<void> _observe(Directory directory, int length, String phase) async {
  final process = await Process.start(
    _environment('MORROW_UPLOAD_HISTORY_OBSERVER'),
    [directory.path, '$length', phase],
  );
  final out = process.stdout.transform(utf8.decoder).join();
  final err = process.stderr.transform(utf8.decoder).join();
  final code = await process.exitCode;
  final stdout = await out, stderr = await err;
  await _record(directory, '$phase-history', {
    'exit_code': code,
    'stdout': stdout,
    'stderr': stderr,
  });
  expect(code, 0);
  expect(stderr, isEmpty);
  expect(stdout, contains('VERIFIED_ORIGINAL_PROTECTED_UPLOAD length=$length'));
}

void main() {
  final enabled =
      Platform.isWindows &&
      Platform.environment['MORROW_UPLOAD_PRODUCT_ROOT'] != null;
  for (final language in ['rust', 'c', 'cpp']) {
    for (final length in [150017, 16 * 1024 * 1024]) {
      test(
        'original $language $length bytes crosses Dart and supervised host then reopens',
        () async {
          final root = Directory(_environment('MORROW_UPLOAD_PRODUCT_ROOT'));
          expect(
            root.absolute.path.toLowerCase().replaceAll('/', r'\'),
            startsWith(r'd:\build_cache\'),
          );
          final directory = Directory('${root.path}/$language-$length');
          expect(directory.existsSync(), isFalse);
          await directory.create();
          final inputs = _environment('MORROW_UPLOAD_FIXTURES');
          final package = '$inputs/$language/upload.mplugin';
          await _pin(
            package,
            _environment(
              'MORROW_UPLOAD_${language.toUpperCase()}_PACKAGE_SHA256',
            ),
          );
          for (final name in [
            'MORROW_WORKBENCH_HOST',
            'MORROW_WORKBENCH_SUPERVISOR',
            'MORROW_WORKBENCH_PACKAGE',
            'MORROW_UPLOAD_HISTORY_OBSERVER',
          ]) {
            await _pin(_environment(name), _environment('${name}_SHA256'));
          }
          final input = await File(
            '$inputs/$language/descriptor-$length.bin',
          ).readAsBytes();
          final whole = Uint8List.fromList(
            sha256.convert(Uint8List(length)..fillRange(0, length, 42)).bytes,
          );
          RustWorkbench? backend;
          BlobUploadRequest request(
            BigInt revision,
            Uint8List digest,
            int submission, {
            bool release = false,
          }) => BlobUploadRequest(
            submission: _id(submission),
            packageId: 'org.example.blob-upload.$language',
            packageDigest: digest,
            registryRevision: revision,
            nominationRef: _id(71),
            ownerDomain: _id(72),
            transferEpoch: _id(71),
            objectRef: _id(2),
            operationId: _id(3),
            byteLength: BigInt.from(length),
            wholeSha256: whole,
            timeoutMs: 120000,
            handler: release ? 'blob.upload.release' : 'blob.upload',
            inputType: 'upload.descriptor.v1',
            outputType: 'upload.result.v1',
            input: input,
          );
          try {
            backend = await _open(directory, 'stage');
            final preview = await backend.inspectPlugin(package);
            expect(preview.entries.single.available, isTrue);
            await backend.importPlugin(
              package,
              preview.entries.single.digest,
              preview.revision,
            );
            var catalog = await entireCatalog(backend);
            var entry = catalog.entries.singleWhere((p) => !p.builtin);
            expect(entry.blobUploadSupported, isTrue);
            expect(entry.blobUploadApproved, isFalse);
            await backend.configureExternal(entry, catalog.revision, [], true);
            catalog = await entireCatalog(backend);
            entry = catalog.entries.singleWhere((p) => !p.builtin);
            await expectLater(
              backend.startBlobUpload(
                request(catalog.revision, entry.digest, 1),
              ),
              throwsStateError,
            );
            expect((await backend.ioStatus()).key, isNull);
            await backend.approveBlobUpload(
              entry.id,
              entry.digest,
              catalog.revision,
              true,
            );
            catalog = await entireCatalog(backend);
            entry = catalog.entries.singleWhere((p) => !p.builtin);
            expect(entry.blobUploadApproved, isTrue);
            final exact = request(catalog.revision, entry.digest, 1);
            final started = await backend.startBlobUpload(exact);
            expect(started.task.submission, exact.submission);
            expect((await backend.ioStatus()).submission, exact.submission);
            await _wait(backend, started);
            final read = await backend.readBlobUpload(started.task.key!);
            expect(read.result!.phase, BlobUploadObservation.staged);
            expect(read.result!.error, isNull);
            expect(read.result!.unknown, isFalse);
            expect(read.result!.calls, (length + 61439) ~/ 61440 + 4);
            expect(read.snapshot.delivered, isTrue);
            await expectLater(
              backend.readBlobUpload(started.task.key!),
              throwsStateError,
            );
            final historical = await backend.inspectBlobUpload(
              started.task.key!,
            );
            expect(historical.observed, BlobUploadObservation.staged);
            await backend.acknowledgeIo(started.task.key!);
            await expectLater(backend.startBlobUpload(exact), throwsStateError);
            await _close(backend, directory, 'stage');
            backend = null;
            await _observe(directory, length, 'staged');
            // Only known-success closure is reopened. No historical Unknown owner
            // is restarted or assigned a new operation identity.
            backend = await _open(directory, 'release');
            expect((await backend.ioStatus()).key, isNull);
            catalog = await entireCatalog(backend);
            entry = catalog.entries.singleWhere((p) => !p.builtin);
            final release = await backend.startBlobUpload(
              request(catalog.revision, entry.digest, 2, release: true),
            );
            await _wait(backend, release);
            final released = await backend.readBlobUpload(release.task.key!);
            expect(released.result!.phase, BlobUploadObservation.released);
            expect(released.result!.calls, 2);
            expect(released.result!.unknown, isFalse);
            await backend.acknowledgeIo(release.task.key!);
            await _close(backend, directory, 'release');
            backend = null;
            await _observe(directory, length, 'released');
            await _record(directory, 'accepted', {
              'language': language,
              'bytes': length,
              'sha256': sha256
                  .convert(Uint8List(length)..fillRange(0, length, 42))
                  .toString(),
              'original_workers_joined': 2,
              'supervised_owners_closed': 2,
              'actual_Dart_native_exchange': true,
              'rendered_approval_UI': false,
            });
          } finally {
            // Retain all evidence. Only this held original process is closed.
            if (backend != null) await _close(backend, directory, 'failure');
          }
        },
        skip: !enabled,
        timeout: const Timeout(Duration(minutes: 6)),
      );
    }
  }
}
