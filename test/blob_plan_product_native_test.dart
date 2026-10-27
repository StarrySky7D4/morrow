// Actual retained Dart sessions, supervised native host and pinned SDK planners.
// Reply loss below occurs after a real native response at the application edge;
// it is not a claim of OS transport failure or a rendered approval by a person.
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';
import 'package:crypto/crypto.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/blob_plan_models.dart';
import 'package:morrow_studio/plugins/blob_plan_session.dart';
import 'package:morrow_studio/plugins/blob_upload_models.dart';
import 'package:morrow_studio/plugins/blob_upload_session.dart';
import 'package:morrow_studio/plugins/io_task_models.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';
import 'package:morrow_studio/plugins/workbench_channel_supervised.dart';
import 'external_plugin_native_test.dart' show entireCatalog;

Uint8List _id(int n) => Uint8List.fromList(List.filled(32, n));
String _environment(String name) =>
    Platform.environment[name] ??
    (throw StateError('Missing explicit qualification input: $name'));
Future<void> _pin(String path, String expected) async =>
    expect(sha256.convert(await File(path).readAsBytes()).toString(), expected);
Future<void> _record(Directory directory, String name, Object value) async {
  final file = File('${directory.path}/$name.json');
  expect(file.existsSync(), isFalse);
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

final _closing = Expando<Future<void>>();
Future<void> _close(RustWorkbench backend, Directory directory, String stage) =>
    _closing[backend] ??= _closeOnce(backend, directory, stage);
Future<void> _closeOnce(
  RustWorkbench backend,
  Directory directory,
  String stage,
) async {
  await backend.close();
  expect(await backend.process.exitCode, 0);
  final channel = backend.channel as SupervisedWorkbenchChannel;
  expect(channel.supervision.value!.released, isTrue);
  // Raw supervision facts are retained separately from observed business data.
  await _record(directory, '$stage-closed', channel.supervision.value!.record);
}

Future<void> _observe(
  Directory directory,
  int length,
  int count,
  String stage,
) async {
  final process = await Process.start(
    _environment('MORROW_PLAN_HISTORY_OBSERVER'),
    [directory.path, '$length', '$count'],
  );
  final out = process.stdout.transform(utf8.decoder).join(),
      err = process.stderr.transform(utf8.decoder).join();
  final code = await process.exitCode;
  final stdout = await out, stderr = await err;
  await _record(directory, '$stage-history', {
    'exit_code': code,
    'stdout': stdout,
    'stderr': stderr,
  });
  expect(code, 0);
  expect(stderr, isEmpty);
  expect(
    stdout,
    contains('VERIFIED_ORIGINAL_PLANNED_UPLOAD length=$length objects=$count'),
  );
}

final class _PlanReplies implements BlobPlanBackend {
  _PlanReplies(this.actual, {this.loseRead = false});
  final RustWorkbench actual;
  final bool loseRead;
  int starts = 0, reads = 0, approvals = 0;
  @override
  Future<BlobPlanSnapshot> startBlobPlan(BlobPlanRequest request) async {
    starts++;
    await actual.startBlobPlan(request);
    throw StateError('Original native start reply deliberately discarded');
  }

  @override
  Future<BlobPlanSnapshot> statusBlobPlan(Uint8List key) =>
      actual.statusBlobPlan(key);
  @override
  Future<BlobPlanSnapshot?> discoverBlobPlan(Uint8List submission) =>
      actual.discoverBlobPlan(submission);
  @override
  Future<BlobPlanReply> readBlobPlan(Uint8List key) async {
    reads++;
    final reply = await actual.readBlobPlan(key);
    if (loseRead) {
      throw StateError('Original native proposal reply deliberately discarded');
    }
    return reply;
  }

  @override
  Future<BlobPlanSnapshot> cancelBlobPlan(Uint8List key) =>
      actual.cancelBlobPlan(key);
  @override
  Future<void> acknowledgeBlobPlan(Uint8List key) =>
      actual.acknowledgeBlobPlan(key);
  @override
  Future<BlobPlanSnapshot> approveBlobPlan(
    Uint8List key,
    BlobPlanView reviewed,
  ) async {
    approvals++;
    await actual.approveBlobPlan(key, reviewed);
    throw StateError('Original native approval reply deliberately discarded');
  }
}

final class _UploadReplies implements BlobUploadBackend {
  _UploadReplies(this.actual, {required this.loseRead});
  final RustWorkbench actual;
  final bool loseRead;
  int starts = 0, reads = 0;
  BlobUploadReply? discarded;
  @override
  Future<void> approveBlobUpload(
    String id,
    Uint8List digest,
    BigInt revision,
    bool approved,
  ) => actual.approveBlobUpload(id, digest, revision, approved);
  @override
  Future<BlobUploadSnapshot> startBlobUpload(BlobUploadRequest request) async {
    starts++;
    throw StateError(
      'Retained planning flow must never reconstruct upload input',
    );
  }

  @override
  Future<BlobUploadSnapshot> statusBlobUpload(Uint8List key) =>
      actual.statusBlobUpload(key);
  @override
  Future<BlobUploadReply> readBlobUpload(Uint8List key) async {
    reads++;
    final reply = await actual.readBlobUpload(key);
    if (loseRead) {
      discarded = reply;
      throw StateError('Original native result reply deliberately discarded');
    }
    return reply;
  }

  @override
  Future<BlobUploadSnapshot> inspectBlobUpload(Uint8List key) =>
      actual.inspectBlobUpload(key);
  @override
  Future<BlobUploadSnapshot> confirmBlobUpload(
    Uint8List key,
    BlobUploadObservation expected,
  ) => actual.confirmBlobUpload(key, expected);
}

Future<Directory> _fresh(String name) async {
  final root = Directory(_environment('MORROW_PLAN_PRODUCT_ROOT'));
  expect(
    root.absolute.path.toLowerCase().replaceAll('/', r'\'),
    startsWith(r'd:\build_cache\'),
  );
  final directory = Directory('${root.path}/$name');
  expect(directory.existsSync(), isFalse);
  await directory.create();
  for (final name in [
    'MORROW_WORKBENCH_HOST',
    'MORROW_WORKBENCH_SUPERVISOR',
    'MORROW_WORKBENCH_PACKAGE',
    'MORROW_PLAN_HISTORY_OBSERVER',
  ]) {
    await _pin(_environment(name), _environment('${name}_SHA256'));
  }
  return directory;
}

Future<BlobPlanRequest> _install(
  RustWorkbench backend,
  String language,
  int length,
) async {
  final package =
      '${_environment('MORROW_PLAN_FIXTURES')}/$language/planner.mplugin';
  await _pin(
    package,
    _environment('MORROW_PLAN_${language.toUpperCase()}_PACKAGE_SHA256'),
  );
  final preview = await backend.inspectPlugin(package);
  expect(preview.entries.single.available, isTrue);
  await backend.importPlugin(
    package,
    preview.entries.single.digest,
    preview.revision,
  );
  var page = await entireCatalog(backend);
  var entry = page.entries.singleWhere((p) => !p.builtin);
  expect(entry.blobPlanners.single.handler, 'blob.plan');
  expect(entry.blobPlanners.single.uploadHandler, 'blob.upload');
  expect(entry.blobUploadApproved, isFalse);
  await backend.configureExternal(entry, page.revision, [], true);
  page = await entireCatalog(backend);
  entry = page.entries.singleWhere((p) => !p.builtin);
  await backend.approveBlobUpload(entry.id, entry.digest, page.revision, true);
  page = await entireCatalog(backend);
  entry = page.entries.singleWhere((p) => !p.builtin);
  expect(entry.blobUploadApproved, isTrue);
  final input = await File(
    '${_environment('MORROW_PLAN_FIXTURES')}/$language/options-$length.bin',
  ).readAsBytes();
  return BlobPlanRequest(
    submission: _id(61),
    packageId: entry.id,
    packageDigest: entry.digest,
    registryRevision: page.revision,
    handler: entry.blobPlanners.single.handler,
    inputType: entry.handlers
        .singleWhere((h) => h.name == 'blob.plan')
        .inputType,
    input: input,
  );
}

Future<void> _waitPlan(BlobPlanSession session, Object view) async {
  final timer = Stopwatch()..start();
  do {
    expect(await session.refresh(view), isTrue);
    if (session.snapshot!.ready) break;
    if (timer.elapsed > const Duration(seconds: 125)) {
      throw StateError('Retain original planner: no replay');
    }
    await Future<void>.delayed(const Duration(milliseconds: 15));
  } while (true);
  expect(session.snapshot!.originalWorkerJoined, isTrue);
  expect(session.snapshot!.error, isNull);
}

Future<void> _waitUpload(BlobUploadSession session, Object view) async {
  final timer = Stopwatch()..start();
  while (session.snapshot?.ready != true) {
    expect(await session.refresh(view), isTrue);
    if (timer.elapsed > const Duration(seconds: 125)) {
      throw StateError('Retain original upload: no replay');
    }
    await Future<void>.delayed(const Duration(milliseconds: 15));
  }
  final task = session.snapshot!.task;
  expect(task.exit, isNotNull);
  expect(task.exit!.disconnect, IoJobError.none);
  expect(task.exit!.maintenance, IoJobError.none);
}

void main() {
  final enabled =
      Platform.isWindows &&
      Platform.environment['MORROW_PLAN_PRODUCT_ROOT'] != null;
  for (final language in ['rust', 'c', 'cpp']) {
    for (final length in [150017, 16777216]) {
      test(
        'original $language plan for $length bytes crosses retained sessions and protected reopen',
        () async {
          final directory = await _fresh('$language-$length');
          RustWorkbench? backend;
          try {
            backend = await _open(directory, 'plan');
            final request = await _install(backend, language, length);
            final peer = _PlanReplies(backend),
                plans = BlobPlanSession.forBackend(peer, backend),
                view = plans.attachView();
            expect(await plans.refresh(view), isTrue);
            expect(await plans.start(view, request), isFalse);
            expect(plans.canStart, isFalse);
            expect(await plans.start(view, request), isFalse);
            await _waitPlan(plans, view);
            expect((await backend.ioStatus()).key, isNull);
            expect(await plans.read(view), isTrue);
            final proposal = plans.result!.view!;
            final expected = Uint8List.fromList(
              sha256.convert(Uint8List(length)..fillRange(0, length, 73)).bytes,
            );
            expect(proposal.wholeSha256, expected);
            expect(proposal.byteLength, BigInt.from(length));
            expect(
              proposal.planningInputSha256,
              sha256.convert(request.input).bytes,
            );
            expect(await plans.approve(view), isFalse);
            expect(await plans.approve(view), isFalse);
            expect(await plans.refresh(view), isTrue);
            final original = plans.snapshot!;
            expect(original.consumed, isTrue);
            expect(original.uploadKey, isNotNull);
            expect(original.uploadSubmission, isNot(request.submission));
            final lostResult = length == 16777216;
            final uploadsPeer = _UploadReplies(backend, loseRead: lostResult),
                uploads = BlobUploadSession.forBackend(uploadsPeer, backend),
                uploadView = uploads.attachView();
            expect(
              await uploads.adoptOriginal(
                uploadView,
                original.uploadKey!,
                original.uploadSubmission!,
              ),
              isTrue,
            );
            await _waitUpload(uploads, uploadView);
            expect(await uploads.read(uploadView), !lostResult);
            final observedResult = lostResult
                ? uploadsPeer.discarded!.result
                : uploads.result;
            expect(observedResult!.phase, BlobUploadObservation.staged);
            expect(observedResult.error, isNull);
            expect(observedResult.unknown, isFalse);
            expect(observedResult.calls, (length + 61439) ~/ 61440 + 4);
            if (lostResult) {
              expect(uploads.result, isNull);
              expect(uploads.deliveryLost, isTrue);
              expect(uploads.canAcknowledge, isFalse);
              expect(await uploads.refresh(uploadView), isTrue);
              expect(await uploads.read(uploadView), isFalse);
            }
            expect(await uploads.inspect(uploadView), isTrue);
            expect(uploads.snapshot!.observed, BlobUploadObservation.staged);
            if (lostResult) {
              expect(
                await uploads.confirm(uploadView, BlobUploadObservation.staged),
                isTrue,
              );
              expect(uploads.result, isNull);
            }
            expect(await uploads.acknowledge(uploadView), isTrue);
            expect(uploads.history.single.result == null, lostResult);
            expect(await plans.refresh(view), isTrue);
            expect(plans.snapshot!.uploadKey, original.uploadKey);
            expect(await plans.acknowledge(view), isTrue);
            expect(
              plans.history.single.snapshot!.uploadSubmission,
              original.uploadSubmission,
            );
            expect(peer.starts, 1);
            expect(peer.approvals, 1);
            expect(peer.reads, 1);
            expect(uploadsPeer.starts, 0);
            expect(uploadsPeer.reads, 1);
            await plans.detachView(view);
            await uploads.detachView(uploadView);
            await _record(directory, 'original-business', {
              'planning_submission': base64Encode(request.submission),
              'planning_key': base64Encode(original.key),
              'upload_key': base64Encode(original.uploadKey!),
              'upload_submission': base64Encode(original.uploadSubmission!),
              'input_sha256': sha256.convert(request.input).toString(),
              'object_sha256': sha256
                  .convert(Uint8List(length)..fillRange(0, length, 73))
                  .toString(),
              'plan_starts': peer.starts,
              'approvals': peer.approvals,
              'direct_upload_starts': uploadsPeer.starts,
              'lost_result': lostResult,
              'client_result_reconstructed': false,
              'original_history': 'staged',
              'original_workers_joined': 2,
            });
            await _close(backend, directory, 'plan');
            backend = null;
            await _observe(directory, length, 1, 'first');
            // Reopen only after known application outcome, original worker joins,
            // actual native exit, released supervision and an independent byte check.
            backend = await _open(directory, 'reopen');
            expect((await backend.ioStatus()).key, isNull);
            expect(await backend.discoverBlobPlan(request.submission), isNull);
            final page = await entireCatalog(backend);
            expect(
              page.entries.singleWhere((p) => !p.builtin).blobUploadApproved,
              isTrue,
            );
            await _close(backend, directory, 'reopen');
            backend = null;
            await _observe(directory, length, 1, 'second');
            await _record(directory, 'accepted', {
              'language': language,
              'bytes': length,
              'original_planning_workers_joined': 1,
              'original_upload_workers_joined': 1,
              'supervised_owners_closed': 2,
              'independent_closed_library_checks': 2,
              'start_reply_discarded': true,
              'approval_reply_discarded': true,
              'result_reply_discarded': lostResult,
              'actual_Dart_native_planning_exchange': true,
              'retained_sessions': true,
              'exact_bytes_SHA': true,
              'rendered_approval_UI': false,
              'cross_restart_Unknown_recovery': false,
            });
          } finally {
            if (backend != null) await _close(backend, directory, 'failure');
          }
        },
        skip: !enabled,
        timeout: const Timeout(Duration(minutes: 6)),
      );
    }
  }
  test(
    'lost original proposal cannot acquire approval or create a stored object',
    () async {
      final directory = await _fresh('rust-lost-proposal');
      RustWorkbench? backend;
      try {
        backend = await _open(directory, 'plan');
        final request = await _install(backend, 'rust', 150017);
        final peer = _PlanReplies(backend, loseRead: true),
            plans = BlobPlanSession.forBackend(peer, backend),
            view = plans.attachView();
        await plans.refresh(view);
        expect(await plans.start(view, request), isFalse);
        await _waitPlan(plans, view);
        expect(await plans.read(view), isFalse);
        expect(await plans.refresh(view), isTrue);
        expect(plans.deliveryLost, isTrue);
        expect(plans.result, isNull);
        expect(await plans.read(view), isFalse);
        expect(await plans.approve(view), isFalse);
        expect(peer.starts, 1);
        expect(peer.reads, 1);
        expect(peer.approvals, 0);
        expect((await backend.ioStatus()).key, isNull);
        expect(await plans.cancel(view), isTrue);
        expect(await plans.acknowledge(view), isTrue);
        await plans.detachView(view);
        await _close(backend, directory, 'plan');
        backend = null;
        await _observe(directory, 150017, 0, 'empty');
        await _record(directory, 'accepted', {
          'language': 'rust',
          'bytes': 0,
          'lost_proposal': true,
          'approvals': 0,
          'stored_objects': 0,
          'original_planning_workers_joined': 1,
          'original_upload_workers_joined': 0,
          'supervised_owners_closed': 1,
          'actual_Dart_native_planning_exchange': true,
          'retained_sessions': true,
          'rendered_approval_UI': false,
        });
      } finally {
        if (backend != null) await _close(backend, directory, 'failure');
      }
    },
    skip: !enabled,
    timeout: const Timeout(Duration(minutes: 4)),
  );
}
