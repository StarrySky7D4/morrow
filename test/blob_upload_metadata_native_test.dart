// Opt-in qualification: production widgets and an original supervised host
// compiled with exactly one fixed metadata failure point. No mocked replies.
import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'package:crypto/crypto.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/blob_plan_manager.dart';
import 'package:morrow_studio/plugins/blob_plan_session.dart';
import 'package:morrow_studio/plugins/blob_upload_models.dart';
import 'package:morrow_studio/plugins/blob_upload_session.dart';
import 'package:morrow_studio/plugins/plugin_library.dart';
import 'package:morrow_studio/plugins/workbench_channel_supervised.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';

String _env(String name) =>
    Platform.environment[name] ?? (throw StateError('Missing $name'));
Future<void> _record(Directory root, String name, Object value) async {
  final file = File('${root.path}/$name.json');
  expect(file.existsSync(), isFalse);
  await file.writeAsString(jsonEncode(value));
}

Future<void> _wait(WidgetTester tester, bool Function() ready) async {
  final deadline = DateTime.now().add(const Duration(seconds: 120));
  while (!ready()) {
    if (DateTime.now().isAfter(deadline)) {
      fail('Original operation did not finish; retain without replay');
    }
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 20)),
    );
    await tester.pump(const Duration(milliseconds: 20));
    final error = tester.takeException();
    if (error != null) throw error;
  }
}

bool _enabled(WidgetTester tester, String key) {
  final found = find.byKey(ValueKey(key));
  return found.evaluate().isNotEmpty &&
      tester.widget<OutlinedButton>(found).onPressed != null;
}

Future<void> _click(WidgetTester tester, String key) async {
  final found = find.byKey(ValueKey(key));
  await tester.ensureVisible(found);
  await _wait(tester, () => found.hitTestable().evaluate().length == 1);
  await tester.tap(found);
  // Establish the animation's first frame before advancing its clock.
  await tester.pump();
  await tester.pump(const Duration(milliseconds: 250));
  await _wait(
    tester,
    () => find.byType(LinearProgressIndicator).evaluate().isEmpty,
  );
  await tester.pump();
}

void main() {
  final enabled =
      Platform.isWindows &&
      Platform.environment['MORROW_METADATA_UI_ROOT'] != null;
  testWidgets(
    'original supervised upload metadata completion',
    (tester) async {
      final root = Directory(_env('MORROW_METADATA_UI_ROOT'));
      final point = _env('MORROW_METADATA_UI_POINT');
      expect([
        'observe-before',
        'observe-after',
        'ack-before',
        'ack-after',
      ], contains(point));
      expect(
        root.absolute.path
            .toLowerCase()
            .replaceAll('\\', '/')
            .startsWith('d:/build_cache/'),
        isTrue,
      );
      expect(root.existsSync(), isFalse);
      tester.view.physicalSize = const Size(1400, 1400);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      RustWorkbench? backend;
      const input = '  metadata original owner\nsecond line  ';
      try {
        await tester.runAsync(() async {
          await root.create();
          await File('${root.path}/expected.txt').writeAsString(input);
          for (final name in [
            'MORROW_WORKBENCH_HOST',
            'MORROW_WORKBENCH_SUPERVISOR',
            'MORROW_WORKBENCH_PACKAGE',
            'MORROW_METADATA_UI_PLUGIN',
            'MORROW_METADATA_UI_OBSERVER',
          ]) {
            expect(
              sha256.convert(await File(_env(name)).readAsBytes()).toString(),
              _env('${name}_SHA256'),
            );
          }
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
            'original-open',
            channel.supervision.value!.record,
          );
        });
        final actual = backend!;
        await tester.pumpWidget(
          MaterialApp(
            home: Scaffold(
              body: SingleChildScrollView(
                child: PluginLibrary(
                  backend: actual,
                  onChanged: () {},
                  ink: Colors.black,
                  muted: Colors.grey,
                  line: Colors.grey,
                  radius: BorderRadius.circular(12),
                  pickPackage: () async => _env('MORROW_METADATA_UI_PLUGIN'),
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
        const id = 'org.example.blob-text.rust';
        await _click(tester, 'plugin-entry-$id');
        await _click(tester, 'plugin-approve-$id');
        await _click(tester, 'plugin-blob-allow-$id');
        await _click(tester, 'plugin-blob-plan-$id');
        expect(find.byType(BlobPlanManager), findsOneWidget);
        await tester.enterText(
          find.byKey(const ValueKey('blob-plan-input')),
          input,
        );
        await _click(tester, 'blob-plan-start');
        await _wait(tester, () => _enabled(tester, 'blob-plan-read'));
        await _click(tester, 'blob-plan-read');
        await _click(tester, 'blob-plan-reviewed');
        await _click(tester, 'blob-plan-approve');
        await _wait(tester, () => _enabled(tester, 'blob-upload-read'));
        await _click(tester, 'blob-upload-read');
        final uploads = BlobUploadSession.forBackend(actual, actual);
        final plans = BlobPlanSession.forBackend(actual, actual);
        expect(uploads.result!.phase, BlobUploadObservation.staged);
        expect(uploads.result!.unknown, isFalse);
        if (point.startsWith('ack-')) {
          expect(_enabled(tester, 'blob-plan-finish'), isTrue);
          await _click(tester, 'blob-plan-finish');
          expect(uploads.acknowledgementUncertain, isTrue);
        }
        expect(_enabled(tester, 'blob-plan-finish'), isFalse);
        expect(_enabled(tester, 'blob-upload-metadata-complete'), isFalse);
        await _click(tester, 'blob-upload-metadata-review');
        final review = uploads.metadataReview!;
        expect(review.writeRequired, point.endsWith('before'));
        expect(
          review.phase,
          point.startsWith('ack-')
              ? BlobUploadMetadataPhase.acknowledgement
              : BlobUploadMetadataPhase.observation,
        );
        expect(review.originalUnknown, isFalse);
        expect(review.originalPhase, BlobUploadObservation.staged);
        final before = (await tester.runAsync(
          actual.listBlobUploadHistory,
        ))!.records.single;
        final expectedRevision = point.startsWith('ack-') ? 2 : 1;
        expect(
          before.revision,
          expectedRevision + (point.endsWith('after') ? 1 : 0),
        );
        expect(before.acknowledgedAt != null, point == 'ack-after');
        await tester.runAsync(
          () => _record(root, 'original-review', {
            'point': point,
            'write_required': review.writeRequired,
            'original_unknown': review.originalUnknown,
            'observed_at': review.observedAt.toString(),
            'acknowledgement_at': review.acknowledgementAt?.toString(),
            'submission': review.submission,
            'key': review.key,
            'phase': review.phase.name,
            'before_revision': before.revision,
          }),
        );
        await _click(tester, 'blob-upload-metadata-complete');
        expect(_enabled(tester, 'blob-upload-metadata-complete'), isFalse);
        expect(_enabled(tester, 'blob-plan-finish'), isTrue);
        await _click(tester, 'blob-plan-finish');
        expect(uploads.submission, isNull);
        expect(plans.request, isNull);
        expect(
          uploads.history.single.result!.phase,
          BlobUploadObservation.staged,
        );
        expect(plans.history.single.snapshot!.originalWorkerJoined, isTrue);
        final saved = (await tester.runAsync(
          actual.listBlobUploadHistory,
        ))!.records.single;
        expect(saved.revision, 3);
        expect(saved.observedAt, review.observedAt);
        expect(saved.acknowledgedAt, isNotNull);
        if (review.acknowledgementAt != null) {
          expect(saved.acknowledgedAt, review.acknowledgementAt);
        }
        expect(saved.originalUnknown, isFalse);
        expect(saved.wholeSha256, sha256.convert(utf8.encode(input)).bytes);
        await _click(tester, 'blob-plan-close');
        await tester.pumpWidget(const SizedBox.shrink());
        bool returned = false;
        Object? closeError;
        await tester.runAsync(() async {
          unawaited(
            actual.close().then(
              (_) {
                returned = true;
              },
              onError: (Object error) {
                closeError = error;
                returned = true;
              },
            ),
          );
        });
        await _wait(tester, () => returned);
        if (closeError != null) throw closeError!;
        expect(await tester.runAsync(() => actual.process.exitCode), 0);
        final closed =
            (actual.channel as SupervisedWorkbenchChannel).supervision.value!;
        expect(closed.released, isTrue);
        await tester.runAsync(() async {
          await _record(root, 'original-closed', closed.record);
          final result = await Process.run(
            _env('MORROW_METADATA_UI_OBSERVER'),
            [root.path, '${root.path}/expected.txt', '1'],
          );
          await _record(root, 'independent-read', {
            'exit_code': result.exitCode,
            'stdout': result.stdout,
            'stderr': result.stderr,
          });
          expect(result.exitCode, 0);
          expect(result.stderr, isEmpty);
          expect(
            result.stdout,
            contains('objects=1 exact_bytes_sha=true pending=0'),
          );
          await _record(root, 'accepted', {
            'point': point,
            'original_review': true,
            'same_original_observed_at': true,
            'same_original_acknowledgement_at':
                review.acknowledgementAt != null,
            'one_upload_history': true,
            'original_workers_joined': true,
            'supervised_normal_close': true,
            'independent_closed_read': true,
            'desktop_UI': false,
            'qualification_host': true,
          });
        });
      } catch (error, stack) {
        await tester.runAsync(() async {
          if (!root.existsSync()) await root.create();
          await _record(root, 'failure-retained', {
            'error': '$error',
            'stack': '$stack',
            'automatic_retry': false,
            'automatic_kill': false,
            'original_owner_created': backend != null,
          });
        });
        if (backend != null) {
          // Keep the original controller alive for explicit inspection. Never
          // turn a failed observation into an implicit close or replay.
          await tester.runAsync(() => Completer<void>().future);
        }
        rethrow;
      }
    },
    skip: !enabled,
    timeout: Timeout.none,
  );
}
