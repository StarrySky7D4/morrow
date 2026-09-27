// Real Windows owner expiry: a reviewed plan and durable Prepare cannot turn
// into an OS effect after their original 30-second authority has elapsed.
import 'dart:io';
import 'dart:typed_data';

import 'package:crypto/crypto.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/guest_mutation_execution_session.dart';
import 'package:morrow_studio/plugins/guest_mutation_models.dart';
import 'package:morrow_studio/plugins/io_task_models.dart';
import 'package:morrow_studio/plugins/mutation_recovery_session.dart';
import 'package:morrow_studio/plugins/mutation_task_models.dart';
import 'package:morrow_studio/plugins/mutation_workflow.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';

import 'external_plugin_native_test.dart'
    show entireCatalog, removeTestDirectory;
import 'mutation_recovery_real_native_test.dart' show closeRecovery, deliver;

Uint8List _token(int value) => Uint8List(32)..[0] = value;

Uint8List _body() {
  final length = 3 * GuestMutationValidation.maxChunkBytes + 37;
  final builder = BytesBuilder(copy: false);
  for (var counter = 0; builder.length < length; counter++) {
    builder.add(
      sha256.convert(<int>[
        ...'morrow.guest.expiry.four-chunks.v1'.codeUnits,
        ...List<int>.generate(8, (i) => (counter >> (8 * i)) & 255),
      ]).bytes,
    );
  }
  return Uint8List.sublistView(builder.toBytes(), 0, length);
}

void _noTargetEffect(File target) {
  expect(target.existsSync(), isFalse);
  expect(target.parent.listSync(followLinks: false), isEmpty);
}

Future<void> _awaitOriginalExpiry(GuestMutationExecutionSession session) async {
  final clock = Stopwatch()..start();
  while (!session.authorizationExpired &&
      clock.elapsed < const Duration(seconds: 35)) {
    await Future<void>.delayed(const Duration(milliseconds: 25));
  }
  expect(session.authorizationExpired, isTrue);
}

Future<void> _reclaimExpired(GuestMutationExecutionSession session) async {
  final clock = Stopwatch()..start();
  var stopRequested = false;
  while (clock.elapsed < const Duration(seconds: 25)) {
    if (!stopRequested && session.canStop) {
      await session.stop();
      stopRequested = true;
    } else {
      await session.refresh();
    }
    if (session.canRepair) await session.repair();
    if (session.canAcknowledge) {
      expect(session.snapshot?.exit, isNotNull);
      expect(session.snapshot?.storage, IoStoragePhase.reclaimed);
      await session.acknowledge();
      expect(session.canStart, isTrue);
      return;
    }
    await Future<void>.delayed(const Duration(milliseconds: 20));
  }
  throw StateError(
    'Original owner did not exit and return: '
    '${session.snapshot?.storage}/${session.snapshot?.exit}',
  );
}

Future<void> _verifyHistory({
  required RustWorkbench backend,
  required String packageId,
  required Uint8List packageDigest,
  required BigInt registryRevision,
  required Uint8List originalPlan,
  required String operationId,
  required bool prepared,
  required File target,
}) async {
  final history = MutationRecoverySession.forBackend(
    backend.mutationTasks,
    backend,
  );
  await history.refresh();
  expect(history.canStart, isTrue);
  await history.startDiscovery(
    MutationDiscoverRequest(
      submission: _token(40),
      packageId: packageId,
      packageDigest: packageDigest,
      registryRevision: registryRevision,
      subject: guestMutationExecutionSubject,
      disposition: MutationDisposition.create,
      scanLimit: 8,
      timeoutMs: 30000,
    ),
  );
  await deliver(history, () => history.page != null);
  final page = history.page!;
  expect(page.kind, MutationResultKind.plans);
  expect(page.done, isTrue);
  expect(page.plans, hasLength(prepared ? 1 : 0));
  if (prepared) expect(page.plans!.single, orderedEquals(originalPlan));
  _noTargetEffect(target);
  await closeRecovery(history, 41);

  await history.startReconciliation(
    MutationReconcileRequest(
      submission: _token(42),
      packageId: packageId,
      packageDigest: packageDigest,
      registryRevision: registryRevision,
      plan: originalPlan,
      timeoutMs: 30000,
    ),
  );
  await deliver(history, () => history.reconciliation != null);
  final record = history.reconciliation!;
  expect(record.kind, MutationResultKind.reconciled);
  expect(record.effect, MutationEffect.unspecified);
  expect(record.outcome, isNull);
  expect(record.reference, isNull);
  if (prepared) {
    expect(record.operationId, operationId);
    expect(record.phase, MutationPhase.prepared);
    expect(record.record, isNotNull);
  } else {
    expect(record.phase, MutationPhase.none);
    expect(record.operationId, isNull);
    expect(record.record, isNull);
  }
  await deliver(history, () => history.canAcknowledge);
  expect(history.snapshot?.exit, isNotNull);
  await history.acknowledge();
  expect(history.canStart, isTrue);
  _noTargetEffect(target);
}

void main() {
  final env = Platform.environment;
  final host = env['MORROW_WORKBENCH_HOST'];
  final builtin = env['MORROW_WORKBENCH_PACKAGE'];
  final fixture = env['MORROW_GUEST_MUTATION_PACKAGE'];
  final language = env['MORROW_GUEST_MUTATION_LANGUAGE'];
  final moduleHash = env['MORROW_GUEST_MUTATION_WASM_SHA256'];
  final expectedPackageHash = env['MORROW_GUEST_MUTATION_PACKAGE_SHA256'];

  for (final prepared in [false, true]) {
    test(
      'real guest $language expires after ${prepared ? 'nonempty Prepare' : 'plan review'} without Execute',
      () async {
        expect(language, anyOf('rust', 'c', 'cpp'));
        expect(moduleHash, matches(RegExp(r'^[0-9a-f]{64}$')));
        expect(expectedPackageHash, matches(RegExp(r'^[0-9a-f]{64}$')));
        for (final path in [host!, builtin!, fixture!]) {
          expect(File(path).existsSync(), isTrue, reason: path);
        }
        final actualPackageHash = sha256
            .convert(await File(fixture).readAsBytes())
            .toString();
        expect(actualPackageHash, expectedPackageHash);
        final directory = await Directory.systemTemp.createTemp(
          'morrow-external-guest-expiry-${prepared ? 'prepared' : 'plan'}-',
        );
        RustWorkbench? backend;
        var businessPassed = false;
        try {
          backend = await RustWorkbench.open(
            executable: host,
            package: builtin,
            directory: directory,
            managed: true,
          );
          final preview = await backend.inspectPlugin(fixture);
          final candidate = preview.entries.single;
          expect(candidate.mutationSupported, isTrue);
          await backend.importPlugin(
            fixture,
            candidate.digest,
            preview.revision,
          );
          var catalog = await entireCatalog(backend);
          var plugin = catalog.entries.singleWhere((p) => p.id == candidate.id);
          await backend.configureExternalIo(plugin, catalog.revision, [
            'file-create',
          ]);
          catalog = await entireCatalog(backend);
          plugin = catalog.entries.singleWhere((p) => p.id == candidate.id);
          await backend.configureExternal(plugin, catalog.revision, [], true);
          catalog = await entireCatalog(backend);
          plugin = catalog.entries.singleWhere((p) => p.id == candidate.id);
          expect(plugin.enabled, isTrue);
          expect(plugin.approvedIo, contains('file-create'));
          final declaration = plugin.mutationBudget!;
          final targets = Directory(
            '${directory.path}${Platform.pathSeparator}targets',
          )..createSync();
          final target = File(
            '${targets.path}${Platform.pathSeparator}never-created.bin',
          );
          final content = _body();
          final operationId = prepared
              ? 'real-guest-expiry-prepared-$language'
              : 'real-guest-expiry-plan-$language';
          final session = GuestMutationExecutionSession.forBackend(
            backend.guestMutationTasks,
            backend,
          );
          await session.refresh();
          expect(session.canStart, isTrue);
          await session.review(
            GuestMutationStartRequest(
              selection: MutationStartRequest(
                submission: _token(1),
                packageId: plugin.id,
                packageDigest: plugin.digest,
                registryRevision: catalog.revision,
                disposition: MutationDisposition.create,
                selectedPath: targets.path,
                relativePath: 'never-created.bin',
                subject: guestMutationExecutionSubject,
                approvalSha256: _token(2),
                timeoutMs: 30000,
              ),
              approvedBudget: ApprovedGuestBudget(
                maxJobBytes: declaration.maxJobBytes,
                maxBytes: declaration.maxBytes,
              ),
            ),
            operationId: operationId,
            content: content,
          );
          expect(session.error, isNull);
          expect(session.canPrepare, isTrue);
          expect(session.canExecute, isFalse);
          final plan = Uint8List.fromList(session.plan!);
          final planHash = Uint8List.fromList(session.planSha256!);
          _noTargetEffect(target);
          if (prepared) {
            await session.prepare(reviewedPlanSha256: planHash);
            expect(session.error, isNull);
            expect(session.canExecute, isTrue);
            expect(session.stagedBytes, BigInt.from(content.length));
            expect(session.state?.durableContent, isTrue);
            _noTargetEffect(target);
          }

          await _awaitOriginalExpiry(session);
          expect(session.canPrepare, isFalse);
          expect(session.canExecute, isFalse);
          expect(session.state?.effectAttempted, isFalse);
          if (!prepared) {
            await expectLater(
              session.prepare(reviewedPlanSha256: planHash),
              throwsStateError,
            );
          } else {
            await expectLater(
              session.execute(reviewedPlanSha256: planHash),
              throwsStateError,
            );
          }
          _noTargetEffect(target);
          await _reclaimExpired(session);
          _noTargetEffect(target);
          await _verifyHistory(
            backend: backend,
            packageId: plugin.id,
            packageDigest: plugin.digest,
            registryRevision: catalog.revision,
            originalPlan: plan,
            operationId: operationId,
            prepared: prepared,
            target: target,
          );
          expect(session.outcome, isNull);
          businessPassed = true;
        } finally {
          final current = backend;
          Object? closeError;
          StackTrace? closeStack;
          int? exitCode;
          if (current != null) {
            try {
              await current.close();
            } catch (error, stack) {
              closeError = error;
              closeStack = stack;
            }
            try {
              exitCode = await current.process.exitCode.timeout(
                const Duration(seconds: 5),
              );
            } catch (_) {
              // Unknown process lifetime retains the protected Store.
            }
          }
          Object? cleanupError;
          StackTrace? cleanupStack;
          var cleaned = false;
          if (businessPassed && closeError == null && exitCode == 0) {
            try {
              await removeTestDirectory(directory);
              cleaned = true;
            } catch (error, stack) {
              cleanupError = error;
              cleanupStack = stack;
            }
          }
          if (!cleaned) {
            // ignore: avoid_print
            print('GUEST_EXPIRY_STORE_PRESERVED=${directory.path}');
          }
          if (businessPassed) {
            if (closeError != null) {
              Error.throwWithStackTrace(closeError, closeStack!);
            }
            if (exitCode != 0) {
              throw StateError(
                'Recovery host exit unconfirmed or nonzero: $exitCode; '
                'Store retained at ${directory.path}',
              );
            }
            if (cleanupError != null) {
              Error.throwWithStackTrace(cleanupError, cleanupStack!);
            }
            // ignore: avoid_print
            print(
              'GUEST_EXPIRY_NATIVE_PASS=$language:${prepared ? 'prepared' : 'plan'}',
            );
          }
        }
      },
      skip:
          !Platform.isWindows ||
          host == null ||
          builtin == null ||
          fixture == null ||
          language == null ||
          moduleHash == null ||
          expectedPackageHash == null,
      timeout: const Timeout(Duration(minutes: 3)),
    );
  }
}
