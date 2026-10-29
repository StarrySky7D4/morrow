// Real, owned fault-fixture process exits at a commit boundary. A new default
// production host then renders history without resuming the original task.
// Headless Flutter evidence only; not a desktop or historical-owner replay.
import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:ui' as ui;
import 'package:crypto/crypto.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:morrow_studio/plugins/blob_upload_history_manager.dart';
import 'package:morrow_studio/plugins/blob_upload_history_models.dart';
import 'package:morrow_studio/plugins/blob_upload_models.dart';
import 'package:morrow_studio/plugins/plugin_library.dart';
import 'package:morrow_studio/plugins/workbench_channel_supervised.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';

String _env(String name) =>
    Platform.environment[name] ?? (throw StateError('Missing $name'));
String _hex(List<int> bytes) =>
    bytes.map((b) => b.toRadixString(16).padLeft(2, '0')).join();
Future<void> _pin(String key) async => expect(
  sha256.convert(await File(_env(key)).readAsBytes()).toString(),
  _env('${key}_SHA256'),
);
Future<void> _record(Directory root, String name, Object value) async {
  final file = File('${root.path}/$name.json');
  expect(file.existsSync(), isFalse);
  await file.writeAsString(jsonEncode(value));
}

Map<String, Object?> _identity(BlobUploadHistoryRecord row) => {
  'submission': _hex(row.submission),
  'package': row.packageId,
  'package_digest': _hex(row.packageDigest),
  'registry_revision': row.registryRevision.toString(),
  'nomination': _hex(row.nominationRef),
  'domain': _hex(row.ownerDomain),
  'operation': row.operationId,
  'bytes': row.byteLength.toString(),
  'sha256': _hex(row.wholeSha256),
  'handler': row.handler,
  'input_sha256': _hex(row.inputSha256),
  'binding_sha256': _hex(row.bindingSha256),
  'reserved_at': row.reservedAt.toString(),
  'revision': row.revision,
  'original_result_missing': row.originalResult == null,
  'original_unknown': row.originalUnknown,
  'observed_at': row.observedAt?.toString(),
  'acknowledged_at': row.acknowledgedAt?.toString(),
};

Future<void> _wait(WidgetTester tester, bool Function() ready) async {
  final deadline = DateTime.now().add(const Duration(seconds: 30));
  while (!ready()) {
    if (DateTime.now().isAfter(deadline)) {
      fail('Read-only history did not settle');
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

// Keep pumping the test zone while the one original close drains pending reads.
// Awaiting close only in runAsync can strand an earlier fake-zone continuation.
Future<void> _closePumping(
  WidgetTester tester,
  RustWorkbench backend,
  Directory root,
) async {
  var done = false;
  Object? failure;
  StackTrace? failureStack;
  await tester.runAsync(() async {
    unawaited(
      _close(backend, root).then<void>(
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
Future<void> _close(RustWorkbench backend, Directory root) =>
    _closing[backend] ??= _closeOnce(backend, root);
Future<void> _closeOnce(RustWorkbench backend, Directory root) async {
  await backend.close();
  expect(await backend.process.exitCode, 0);
  final channel = backend.channel as SupervisedWorkbenchChannel;
  expect(channel.supervision.value!.released, isTrue);
  await _record(root, 'reader-closed', channel.supervision.value!.record);
}

void main() {
  final enabled =
      Platform.isWindows &&
      Platform.environment['MORROW_HISTORY_CRASH_UI_ROOT'] != null;
  for (final staged in [false, true]) {
    final point = staged
        ? 'blob-stage-after-commit'
        : 'blob-stage-before-commit';
    testWidgets(
      'actual $point leaves missing original result uncertain in history UI',
      (tester) async {
        final root = Directory(
          '${_env('MORROW_HISTORY_CRASH_UI_ROOT')}/$point',
        );
        RustWorkbench? backend;
        final screenshot = GlobalKey();
        tester.view.physicalSize = const Size(1280, 1000);
        tester.view.devicePixelRatio = 1;
        addTearDown(tester.view.resetPhysicalSize);
        addTearDown(tester.view.resetDevicePixelRatio);
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
            for (final key in [
              'MORROW_WORKBENCH_HOST',
              'MORROW_WORKBENCH_SUPERVISOR',
              'MORROW_WORKBENCH_PACKAGE',
              'MORROW_HISTORY_CRASH_PROGRAM',
              'MORROW_RUST_UPLOAD_WASM',
              'MORROW_TEXT_PLAN_HISTORY_OBSERVER',
            ]) {
              await _pin(key);
            }
            await root.create();
            await File(
              '${root.path}/expected.txt',
            ).writeAsBytes(List.filled(41, 42));
            final env = Map<String, String>.from(Platform.environment)
              ..removeWhere((name, _) => name.startsWith('MORROW_'))
              ..addAll({
                'MORROW_UPLOAD_CRASH_ROOT': root.path,
                'MORROW_TEST_CRASH_AT': point,
                'MORROW_RUST_UPLOAD_WASM': _env('MORROW_RUST_UPLOAD_WASM'),
                'MORROW_RUST_UPLOAD_WASM_SHA256': _env(
                  'MORROW_RUST_UPLOAD_WASM_SHA256',
                ),
              });
            final child = await Process.start(
              _env('MORROW_HISTORY_CRASH_PROGRAM'),
              [
                '--exact',
                'protected_upload_history_commit_crash_child',
                '--ignored',
                '--nocapture',
              ],
              environment: env,
              includeParentEnvironment: false,
            );
            final out = child.stdout.transform(utf8.decoder).join();
            final err = child.stderr.transform(utf8.decoder).join();
            final code = await child.exitCode;
            await _record(root, 'original-process-exit', {
              'point': point,
              'exit_code': code,
              'stdout': await out,
              'stderr': await err,
              'program_sha256': _env('MORROW_HISTORY_CRASH_PROGRAM_SHA256'),
              'original_handle_held_until_exit': true,
              'replayed': false,
            });
            expect(
              code,
              86,
              reason: 'Original owned child must actually exit first',
            );
            backend = await RustWorkbench.open(
              executable: _env('MORROW_WORKBENCH_HOST'),
              supervisorExecutable: _env('MORROW_WORKBENCH_SUPERVISOR'),
              package: _env('MORROW_WORKBENCH_PACKAGE'),
              directory: root,
              managed: true,
            );
            final channel = backend!.channel as SupervisedWorkbenchChannel;
            expect(channel.supervision.value!.phase, 'Running');
            await _record(
              root,
              'reader-open',
              channel.supervision.value!.record,
            );
          });
          final page = (await tester.runAsync(
            () => backend!.listBlobUploadHistory(),
          ))!;
          expect(page.records, hasLength(1));
          final row = page.records.single;
          final before = _identity(row);
          expect(row.revision, 1);
          expect(row.submission, everyElement(93));
          expect(row.originalUnknown, isTrue);
          expect(row.originalResult, isNull);
          expect(row.observedAt, isNull);
          expect(row.acknowledgedAt, isNull);
          expect(row.byteLength, BigInt.from(41));
          expect(
            _hex(row.wholeSha256),
            sha256.convert(List.filled(41, 42)).toString(),
          );
          await tester.pumpWidget(
            RepaintBoundary(
              key: screenshot,
              child: MaterialApp(
                locale: const Locale('zh'),
                localizationsDelegates: AppLocalizations.localizationsDelegates,
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
            () => find.text('原任务结果未记录，执行结果不确定').evaluate().isNotEmpty,
          );
          expect(find.byType(BlobUploadHistoryManager), findsOneWidget);
          expect(find.text('原任务结果未记录，执行结果不确定'), findsOneWidget);
          expect(find.text('未记录原任务确认'), findsOneWidget);
          await _click(
            tester,
            'upload-history-inspect-${_hex(row.submission)}',
          );
          await _wait(
            tester,
            () => find
                .text(staged ? '当前内容记录：已保存' : '当前内容记录：未找到')
                .evaluate()
                .isNotEmpty,
          );
          expect(
            find.text(staged ? '当前内容记录：已保存' : '当前内容记录：未找到'),
            findsOneWidget,
          );
          expect(find.text('原任务结果未记录，执行结果不确定'), findsOneWidget);
          expect(find.text('原任务报告：内容已保存'), findsNothing);
          expect(find.text('原任务已确认'), findsNothing);
          final inspect = (await tester.runAsync(
            () => backend!.inspectBlobUploadHistory(row.submission),
          ))!;
          expect(
            inspect.currentStage,
            staged
                ? BlobUploadObservation.staged
                : BlobUploadObservation.absent,
          );
          expect(_identity(inspect.record), before);
          expect(
            (await tester.runAsync(() => backend!.ioStatus()))!.key,
            isNull,
          );
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
              '${root.path}/uncertain-history.png',
            ).writeAsBytes(bytes!.buffer.asUint8List());
          });
          await tester.pumpWidget(const SizedBox.shrink());
          await _closePumping(tester, backend!, root);
          backend = null;
          await tester.runAsync(() async {
            final observer = await Process.start(
              _env('MORROW_TEXT_PLAN_HISTORY_OBSERVER'),
              [root.path, '${root.path}/expected.txt', staged ? '1' : '0'],
            );
            final out = observer.stdout.transform(utf8.decoder).join();
            final err = observer.stderr.transform(utf8.decoder).join();
            final code = await observer.exitCode;
            final stdout = await out, stderr = await err;
            await _record(root, 'independent-history', {
              'exit_code': code,
              'stdout': stdout,
              'stderr': stderr,
            });
            expect(code, 0);
            expect(stderr, isEmpty);
            expect(
              stdout,
              contains(
                'VERIFIED_ORIGINAL_TEXT_UPLOAD length=41 objects=${staged ? 1 : 0} exact_bytes_sha=true pending=0',
              ),
            );
            await _record(root, 'accepted', {
              'point': point,
              'actual_original_exit': 86,
              'original_history_before': before,
              'original_history_after': _identity(inspect.record),
              'current_stage': staged ? 'staged' : 'absent',
              'prior_owner_contacted_after_exit': false,
              'old_TaskKey_reconstructed': false,
              'new_upload_or_acknowledgement': false,
              'production_Flutter_history_widgets': true,
              'headless_rendering': true,
              'desktop_UI': false,
              'new_supervised_reader_closed': true,
              'independent_exact_content_check': true,
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
            await _closePumping(tester, backend!, root);
          }
        }
      },
      skip: !enabled,
      timeout: const Timeout(Duration(minutes: 6)),
    );
  }
}
