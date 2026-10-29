// Production Flutter widgets with the actual pinned native host and SDK guests.
// Package selection, text entry and deliberate reply discard are qualification adapters.
// This is headless rendering, not a desktop/native-picker or human-review claim.
import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';
import 'dart:ui' as ui;
import 'package:crypto/crypto.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:morrow_studio/plugins/blob_plan_manager.dart';
import 'package:morrow_studio/plugins/blob_plan_models.dart';
import 'package:morrow_studio/plugins/blob_plan_session.dart';
import 'package:morrow_studio/plugins/blob_upload_models.dart';
import 'package:morrow_studio/plugins/blob_upload_history_manager.dart';
import 'package:morrow_studio/plugins/blob_upload_session.dart';
import 'package:morrow_studio/plugins/io_task_models.dart';
import 'package:morrow_studio/plugins/plugin_library.dart';
import 'package:morrow_studio/plugins/workbench_channel_supervised.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';
import 'external_plugin_native_test.dart' show entireCatalog;

String _env(String name) =>
    Platform.environment[name] ?? (throw StateError('Missing $name'));
Future<void> _record(Directory root, String name, Object value) async {
  final file = File('${root.path}/$name.json');
  expect(file.existsSync(), isFalse);
  await file.writeAsString(jsonEncode(value));
}

Future<void> _pin(String path, String expected) async =>
    expect(sha256.convert(await File(path).readAsBytes()).toString(), expected);
Future<RustWorkbench> _open(Directory root, String stage) async {
  final backend = await RustWorkbench.open(
    executable: _env('MORROW_WORKBENCH_HOST'),
    supervisorExecutable: _env('MORROW_WORKBENCH_SUPERVISOR'),
    package: _env('MORROW_WORKBENCH_PACKAGE'),
    directory: root,
    managed: true,
  );
  final channel = backend.channel as SupervisedWorkbenchChannel;
  expect(channel.supervision.value!.phase, 'Running');
  expect(backend.writable, isTrue);
  await _record(root, '$stage-open', channel.supervision.value!.record);
  return backend;
}

// Keep pumping the test zone while the one original close drains pending reads.
// Awaiting close only in runAsync can strand an earlier fake-zone continuation.
Future<void> _closePumping(
  WidgetTester tester,
  RustWorkbench backend,
  Directory root,
  String stage,
) async {
  var done = false;
  Object? failure;
  StackTrace? failureStack;
  await tester.runAsync(() async {
    unawaited(
      _close(backend, root, stage).then<void>(
        (_) => done = true,
        onError: (Object error, StackTrace stack) {
          failure = error;
          failureStack = stack;
          done = true;
        },
      ),
    );
  });
  await _wait(tester, () => done);
  if (failure != null) Error.throwWithStackTrace(failure!, failureStack!);
}

final _closing = Expando<Future<void>>();
Future<void> _close(RustWorkbench backend, Directory root, String stage) =>
    _closing[backend] ??= _closeOnce(backend, root, stage);
Future<void> _closeOnce(
  RustWorkbench backend,
  Directory root,
  String stage,
) async {
  await backend.close();
  expect(await backend.process.exitCode, 0);
  final channel = backend.channel as SupervisedWorkbenchChannel;
  expect(channel.supervision.value!.released, isTrue);
  await _record(root, '$stage-closed', channel.supervision.value!.record);
}

Future<void> _observe(Directory root, int length, String stage) async {
  final process = await Process.start(
    _env('MORROW_TEXT_PLAN_HISTORY_OBSERVER'),
    [root.path, '${root.path}/expected.txt', '1'],
  );
  final out = process.stdout.transform(utf8.decoder).join(),
      err = process.stderr.transform(utf8.decoder).join();
  final code = await process.exitCode;
  final stdout = await out, stderr = await err;
  await _record(root, '$stage-history', {
    'exit_code': code,
    'stdout': stdout,
    'stderr': stderr,
  });
  expect(code, 0);
  expect(stderr, isEmpty);
  expect(
    stdout,
    contains(
      'VERIFIED_ORIGINAL_TEXT_UPLOAD length=$length objects=1 exact_bytes_sha=true pending=0',
    ),
  );
}

/// Forwards every used operation to the actual native host. A real start and
/// approval response are discarded once; large-case result delivery is also lost.
final class _Original
    implements
        ExternalPluginControl,
        BlobPlanBackend,
        BlobUploadBackend,
        WorkbenchIoTaskControl {
  _Original(this.actual, {required this.loseResult});
  final RustWorkbench actual;
  final bool loseResult;
  int starts = 0,
      approvals = 0,
      reads = 0,
      uploadReads = 0,
      categoryApprovals = 0,
      directStarts = 0,
      planAcks = 0,
      uploadAcks = 0,
      unexpected = 0;
  @override
  Future<PluginLibraryPage> pluginPage({
    String cursor = '',
    BigInt? revision,
  }) => actual.pluginPage(cursor: cursor, revision: revision);
  @override
  Future<PluginLibraryPage> inspectPlugin(String path) =>
      actual.inspectPlugin(path);
  @override
  Future<void> importPlugin(String path, Uint8List digest, BigInt revision) =>
      actual.importPlugin(path, digest, revision);
  @override
  Future<void> configureExternal(
    PluginLibraryEntry entry,
    BigInt revision,
    List<String> approved,
    bool enable,
  ) => actual.configureExternal(entry, revision, approved, enable);
  @override
  Future<void> approveBlobUpload(
    String packageId,
    Uint8List digest,
    BigInt revision,
    bool approved,
  ) async {
    categoryApprovals++;
    await actual.approveBlobUpload(packageId, digest, revision, approved);
  }

  @override
  Future<BlobPlanSnapshot> startBlobPlan(BlobPlanRequest request) async {
    starts++;
    await actual.startBlobPlan(request);
    throw StateError('Actual original start reply deliberately discarded');
  }

  @override
  Future<BlobPlanSnapshot> statusBlobPlan(Uint8List key) =>
      actual.statusBlobPlan(key);
  @override
  Future<BlobPlanSnapshot?> discoverBlobPlan(Uint8List submission) =>
      actual.discoverBlobPlan(submission);
  @override
  Future<BlobPlanReply> readBlobPlan(Uint8List key) {
    reads++;
    return actual.readBlobPlan(key);
  }

  @override
  Future<BlobPlanSnapshot> approveBlobPlan(
    Uint8List key,
    BlobPlanView view,
  ) async {
    approvals++;
    await actual.approveBlobPlan(key, view);
    throw StateError('Actual original approval reply deliberately discarded');
  }

  @override
  Future<BlobPlanSnapshot> cancelBlobPlan(Uint8List key) =>
      actual.cancelBlobPlan(key);
  @override
  Future<void> acknowledgeBlobPlan(Uint8List key) {
    planAcks++;
    return actual.acknowledgeBlobPlan(key);
  }

  @override
  Future<BlobUploadSnapshot> startBlobUpload(BlobUploadRequest request) {
    directStarts++;
    throw StateError('This UI must not reconstruct or start raw upload input');
  }

  @override
  Future<BlobUploadSnapshot> statusBlobUpload(Uint8List key) =>
      actual.statusBlobUpload(key);
  @override
  Future<BlobUploadReply> readBlobUpload(Uint8List key) async {
    uploadReads++;
    final reply = await actual.readBlobUpload(key);
    if (loseResult) {
      throw StateError('Actual original result reply deliberately discarded');
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
  @override
  Future<IoTaskSnapshot> ioStatus() => actual.ioStatus();
  @override
  Future<IoTaskSnapshot> cancelIo(Uint8List key) => actual.cancelIo(key);
  @override
  Future<IoTaskSnapshot> repairIo(Uint8List key) => actual.repairIo(key);
  @override
  Future<IoTaskSnapshot> acknowledgeIo(Uint8List key) {
    uploadAcks++;
    return actual.acknowledgeIo(key);
  }

  @override
  dynamic noSuchMethod(Invocation invocation) {
    unexpected++;
    throw UnsupportedError('Unexpected unrelated UI operation');
  }
}

Future<void> _wait(WidgetTester tester, bool Function() complete) async {
  final deadline = DateTime.now().add(const Duration(seconds: 130));
  while (!complete()) {
    if (DateTime.now().isAfter(deadline)) {
      fail('Original UI operation did not finish; retain its evidence');
    }
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 15)),
    );
    await tester.pump();
    final error = tester.takeException();
    if (error != null) throw error;
  }
  await tester.pump();
}

bool _enabled(WidgetTester tester, String key) {
  final found = find.byKey(ValueKey(key));
  return found.evaluate().isNotEmpty &&
      tester.widget<OutlinedButton>(found).onPressed != null;
}

Future<void> _click(WidgetTester tester, String key) async {
  final found = find.byKey(ValueKey(key));
  await tester.ensureVisible(found);
  await tester.tap(found);
  await tester.pump();
  // Let the dialog mount and start its post-frame native read before waiting.
  await tester.pump(const Duration(milliseconds: 250));
  await _wait(
    tester,
    () => find.byType(LinearProgressIndicator).evaluate().isEmpty,
  );
}

Future<void> _ready(WidgetTester tester, String key) async {
  final deadline = DateTime.now().add(const Duration(seconds: 125));
  while (!_enabled(tester, key)) {
    if (DateTime.now().isAfter(deadline)) {
      fail('Original worker not ready; no replay');
    }
    await _click(tester, 'blob-plan-refresh');
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 30)),
    );
    await tester.pump();
  }
}

Future<void> _capture(
  WidgetTester tester,
  GlobalKey key,
  Directory root,
  String name,
) async {
  await tester.ensureVisible(
    find.byKey(
      ValueKey(name == 'proposal' ? 'blob-plan-label' : 'blob-plan-history'),
    ),
  );
  await tester.pump();
  final boundary =
      key.currentContext!.findRenderObject()! as RenderRepaintBoundary;
  await tester.runAsync(() async {
    final image = await boundary.toImage(pixelRatio: 1);
    final bytes = await image.toByteData(format: ui.ImageByteFormat.png);
    image.dispose();
    await File(
      '${root.path}/$name.png',
    ).writeAsBytes(bytes!.buffer.asUint8List());
  });
}

// Text fixtures and stores intentionally use a separate opt-in namespace.
// The older generated-byte qualification environment must not activate this test.
void main() {
  final enabled =
      Platform.isWindows &&
      Platform.environment['MORROW_TEXT_PLAN_UI_ROOT'] != null;
  for (final language in ['rust', 'c', 'cpp']) {
    for (final input in ['  Windows 原生输入 😀\nsecond line  ', 'a' * 32768]) {
      final length = utf8.encode(input).length;
      testWidgets(
        'actual $language text UI saves $length bytes and reads persisted history after known closure',
        (tester) async {
          final root = Directory(
            '${_env('MORROW_TEXT_PLAN_UI_ROOT')}/$language-$length',
          );
          final package =
              '${_env('MORROW_TEXT_PLAN_FIXTURES')}/$language/text.mplugin';
          RustWorkbench? backend;
          tester.view.physicalSize = const Size(1280, 1000);
          tester.view.devicePixelRatio = 1;
          addTearDown(tester.view.resetPhysicalSize);
          addTearDown(tester.view.resetDevicePixelRatio);
          final screenshot = GlobalKey();
          try {
            await tester.runAsync(() async {
              expect(root.existsSync(), isFalse);
              expect(
                root.absolute.path
                    .toLowerCase()
                    .replaceAll('\\', '/')
                    .startsWith('d:/build_cache/'),
                isTrue,
              );
              await root.create(recursive: true);
              await File(
                '${root.path}/expected.txt',
              ).writeAsBytes(utf8.encode(input));
              for (final name in [
                'MORROW_WORKBENCH_HOST',
                'MORROW_WORKBENCH_SUPERVISOR',
                'MORROW_WORKBENCH_PACKAGE',
                'MORROW_TEXT_PLAN_HISTORY_OBSERVER',
              ]) {
                await _pin(_env(name), _env('${name}_SHA256'));
              }
              await _pin(
                package,
                _env(
                  'MORROW_TEXT_PLAN_${language.toUpperCase()}_PACKAGE_SHA256',
                ),
              );
              backend = await _open(root, 'ui');
            });
            final peer = _Original(backend!, loseResult: length == 32768);
            await tester.pumpWidget(
              RepaintBoundary(
                key: screenshot,
                child: MaterialApp(
                  locale: const Locale('zh'),
                  localizationsDelegates:
                      AppLocalizations.localizationsDelegates,
                  supportedLocales: AppLocalizations.supportedLocales,
                  home: Scaffold(
                    body: SingleChildScrollView(
                      child: PluginLibrary(
                        backend: peer,
                        onChanged: () {},
                        ink: Colors.black,
                        muted: Colors.grey,
                        line: Colors.grey,
                        radius: BorderRadius.circular(12),
                        pickPackage: () async => package,
                      ),
                    ),
                  ),
                ),
              ),
            );
            await _wait(
              tester,
              () => find.byType(LinearProgressIndicator).evaluate().isEmpty,
            );
            await _click(tester, 'plugin-pick');
            await _click(tester, 'plugin-import');
            final id = 'org.example.blob-text.$language';
            await _click(tester, 'plugin-entry-$id');
            await _click(tester, 'plugin-approve-$id');
            expect(peer.categoryApprovals, 0);
            expect(_enabled(tester, 'plugin-blob-plan-$id'), isFalse);
            await _click(tester, 'plugin-blob-allow-$id');
            expect(peer.categoryApprovals, 1);
            await _click(tester, 'plugin-blob-plan-$id');
            await tester.enterText(
              find.byKey(const ValueKey('blob-plan-input')),
              input,
            );
            await tester.pump();
            expect(find.byKey(const ValueKey('blob-plan-file')), findsNothing);
            await _click(tester, 'blob-plan-start');
            await _ready(tester, 'blob-plan-read');
            await _click(tester, 'blob-plan-read');
            expect(find.text('Text content'), findsOneWidget);
            expect(find.text('保存大小：$length 字节'), findsOneWidget);
            expect(_enabled(tester, 'blob-plan-approve'), isFalse);
            expect(peer.approvals, 0);
            if (language == 'rust') {
              await _capture(tester, screenshot, root, 'proposal');
            }
            await _click(tester, 'blob-plan-reviewed');
            expect(_enabled(tester, 'blob-plan-approve'), isTrue);
            await _click(tester, 'blob-plan-approve');
            await _ready(tester, 'blob-upload-read');
            await _click(tester, 'blob-upload-read');
            if (peer.loseResult) {
              expect(_enabled(tester, 'blob-plan-finish'), isFalse);
              expect(find.text('内容已保存。'), findsNothing);
            }
            await _click(tester, 'blob-upload-inspect');
            expect(find.text('原记录中已有保存内容'), findsOneWidget);
            if (peer.loseResult) {
              expect(_enabled(tester, 'blob-plan-finish'), isFalse);
              if (language == 'rust') {
                await _capture(
                  tester,
                  screenshot,
                  root,
                  'missing-result-review',
                );
              }
              await _click(tester, 'blob-upload-confirm');
            }
            await _click(tester, 'blob-plan-finish');
            final plans = BlobPlanSession.forBackend(peer, peer),
                uploads = BlobUploadSession.forBackend(peer, peer);
            expect(plans.request, isNull);
            expect(uploads.submission, isNull);
            expect(plans.history.single.snapshot!.originalWorkerJoined, isTrue);
            expect(uploads.history.single.snapshot.task.exit, isNotNull);
            expect(uploads.history.single.result == null, peer.loseResult);
            expect([
              peer.starts,
              peer.approvals,
              peer.reads,
              peer.uploadReads,
              peer.categoryApprovals,
              peer.planAcks,
              peer.uploadAcks,
            ], everyElement(1));
            expect(peer.directStarts, 0);
            expect(peer.unexpected, 0);
            await tester.runAsync(
              () => _record(root, 'business', {
                'original_planning_join': true,
                'original_upload_join': true,
                'category_approvals': peer.categoryApprovals,
                'plan_starts': peer.starts,
                'proposal_approvals': peer.approvals,
                'plan_reads': peer.reads,
                'upload_reads': peer.uploadReads,
                'upload_acknowledgements': peer.uploadAcks,
                'plan_acknowledgements': peer.planAcks,
                'direct_upload_starts': peer.directStarts,
                'unexpected_calls': peer.unexpected,
                'result_reply_discarded': peer.loseResult,
                'missing_result_reconstructed': false,
                'proposal_input_sha256': plans
                    .history
                    .single
                    .result!
                    .view!
                    .planningInputSha256
                    .map((b) => b.toRadixString(16).padLeft(2, '0'))
                    .join(),
              }),
            );
            await _click(tester, 'blob-plan-close');
            expect(find.byType(BlobPlanManager), findsNothing);
            await tester.pumpWidget(const SizedBox.shrink());
            await _closePumping(tester, backend!, root, 'ui');
            backend = null;
            await tester.runAsync(() async {
              await _observe(root, length, 'first');
              backend = await _open(root, 'reopen');
              expect((await backend!.ioStatus()).key, isNull);
              expect(
                (await entireCatalog(
                  backend!,
                )).entries.singleWhere((p) => !p.builtin).blobUploadApproved,
                isTrue,
              );
            });
            final persisted = (await tester.runAsync(
              () => backend!.listBlobUploadHistory(),
            ))!.records.single;
            expect(persisted.revision, 3);
            expect(
              persisted.originalResult!.phase,
              BlobUploadObservation.staged,
            );
            expect(persisted.originalUnknown, isFalse);
            expect(persisted.acknowledgedAt, isNotNull);
            expect(persisted.byteLength, BigInt.from(length));
            expect(
              persisted.wholeSha256,
              sha256.convert(utf8.encode(input)).bytes,
            );
            final submission = persisted.submission
                .map((b) => b.toRadixString(16).padLeft(2, '0'))
                .join();
            await tester.pumpWidget(
              RepaintBoundary(
                key: screenshot,
                child: MaterialApp(
                  locale: const Locale('zh'),
                  localizationsDelegates:
                      AppLocalizations.localizationsDelegates,
                  supportedLocales: AppLocalizations.supportedLocales,
                  home: Scaffold(
                    body: SingleChildScrollView(
                      child: PluginLibrary(
                        backend: backend!,
                        onChanged: () {},
                        ink: Colors.black,
                        muted: Colors.grey,
                        line: Colors.grey,
                        radius: BorderRadius.circular(12),
                      ),
                    ),
                  ),
                ),
              ),
            );
            await _wait(
              tester,
              () => find.byType(LinearProgressIndicator).evaluate().isEmpty,
            );
            await _click(tester, 'plugin-blob-history');
            await _wait(
              tester,
              () => find.text('原任务报告：内容已保存').evaluate().isNotEmpty,
            );
            expect(find.byType(BlobUploadHistoryManager), findsOneWidget);
            expect(find.text('原任务报告：内容已保存'), findsOneWidget);
            expect(find.text('原任务已确认'), findsOneWidget);
            await _click(tester, 'upload-history-inspect-$submission');
            await _wait(
              tester,
              () => find.text('当前内容记录：已保存').evaluate().isNotEmpty,
            );
            expect(find.text('当前内容记录：已保存'), findsOneWidget);
            expect(find.text('原任务报告：内容已保存'), findsOneWidget);
            expect(
              (await tester.runAsync(() => backend!.ioStatus()))!.key,
              isNull,
            );
            if (language == 'rust') {
              final boundary =
                  screenshot.currentContext!.findRenderObject()!
                      as RenderRepaintBoundary;
              await tester.runAsync(() async {
                final image = await boundary.toImage(pixelRatio: 1);
                final bytes = await image.toByteData(
                  format: ui.ImageByteFormat.png,
                );
                image.dispose();
                await File(
                  '${root.path}/persisted-history.png',
                ).writeAsBytes(bytes!.buffer.asUint8List());
              });
            }
            await tester.pumpWidget(const SizedBox.shrink());
            await _closePumping(tester, backend!, root, 'reopen');
            backend = null;
            await tester.runAsync(() async {
              await _observe(root, length, 'second');
              await _record(root, 'persisted-history', {
                'revision': persisted.revision,
                'submission': submission,
                'original_native_result': 'staged',
                'original_unknown': persisted.originalUnknown,
                'original_acknowledgement_recorded':
                    persisted.acknowledgedAt != null,
                'headless_production_history_UI': true,
                'explicit_current_stage_query': 'staged',
                'old_TaskKey_reconstructed': false,
                'separate_original_delivery_loss': peer.loseResult,
                'original_result_delivered_to_prior_UI_claim': false,
              });
              await _record(root, 'accepted', {
                'language': language,
                'bytes': length,
                'actual_native': true,
                'production_Flutter_widgets': true,
                'persisted_history_private_capnp': true,
                'rendered_readonly_history_UI': true,
                'category_and_proposal_approval_via_UI': true,
                'start_and_approval_reply_discarded': true,
                'result_reply_discarded': peer.loseResult,
                'original_planning_joins': 1,
                'original_upload_joins': 1,
                'supervised_owners_closed': 2,
                'independent_closed_library_checks': 2,
                'exact_bytes_SHA': true,
                'headless_rendering': true,
                'desktop_UI': false,
                'native_file_picker': false,
                'cross_restart_Unknown_recovery': false,
              });
            });
          } catch (error, stack) {
            await tester.runAsync(
              () => _record(root, 'test-failure', {
                'error': '$error',
                'stack': '$stack',
              }),
            );
            rethrow;
          } finally {
            await tester.pumpWidget(const SizedBox.shrink());
            if (backend != null) {
              await _closePumping(tester, backend!, root, 'failure');
            }
          }
        },
        skip: !enabled,
        timeout: const Timeout(Duration(minutes: 6)),
      );
    }
  }
}
