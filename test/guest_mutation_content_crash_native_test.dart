// Real Core content-transaction exits during guest Commit, before Execute.
// A new ordinary host may read immutable history but cannot inherit a permit.
import 'dart:io';
import 'dart:typed_data';

import 'package:crypto/crypto.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/guest_mutation_execution_session.dart';
import 'package:morrow_studio/plugins/guest_mutation_models.dart';
import 'package:morrow_studio/plugins/mutation_task_models.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';

import 'external_plugin_native_test.dart'
    show entireCatalog, removeTestDirectory;
import 'guest_mutation_crash_support.dart';
import 'mutation_recovery_real_native_test.dart' show closeRecovery, deliver;

void main() {
  final env = Platform.environment;
  final crashHost = env['MORROW_MUTATION_CRASH_HOST'];
  final normalHost = env['MORROW_WORKBENCH_HOST'];
  final builtin = env['MORROW_WORKBENCH_PACKAGE'];
  final guestPackage = env['MORROW_GUEST_MUTATION_PACKAGE'];
  final language = env['MORROW_GUEST_MUTATION_LANGUAGE'];
  final moduleHash = env['MORROW_GUEST_MUTATION_WASM_SHA256'];
  final packageHash = env['MORROW_GUEST_MUTATION_PACKAGE_SHA256'];
  final verifier = env['MORROW_GUEST_MUTATION_STORE_VERIFIER'];
  final point = env['MORROW_TEST_CRASH_AT'];
  final expectCrash = env['MORROW_MUTATION_EXPECT_CRASH'] != '0';

  test(
    'guest content $language $point crash=$expectCrash is read-only after restart',
    () async {
      expect(language, anyOf('rust', 'c', 'cpp'));
      expect(point, isIn(guestContentCrashPoints));
      if (!expectCrash) expect(point, 'file-content-after-commit');
      expect(env['MORROW_GUEST_MUTATION_CONTENT_BYTES'], '16777216');
      expect(moduleHash, matches(RegExp(r'^[0-9a-f]{64}$')));
      expect(packageHash, matches(RegExp(r'^[0-9a-f]{64}$')));
      for (final path in [
        crashHost!,
        normalHost!,
        builtin!,
        guestPackage!,
        verifier!,
      ]) {
        expect(File(path).existsSync(), isTrue, reason: path);
      }
      expect(
        sha256.convert(await File(guestPackage).readAsBytes()).toString(),
        packageHash,
      );

      final directory = await Directory.systemTemp.createTemp(
        'morrow-external-guest-content-$language-',
      );
      RustWorkbench? backend;
      var crashedCurrent = false;
      var preserveStore = false;
      var allHostsExited = false;
      try {
        Future<RustWorkbench> open(String executable) => RustWorkbench.open(
          executable: executable,
          package: builtin,
          directory: directory,
          managed: true,
        );

        void expectNoTargetFiles(Directory targets) {
          expect(
            targets.listSync(followLinks: false),
            isEmpty,
            reason: 'Commit must not create an OS target or temporary file',
          );
        }

        backend = await open(expectCrash ? crashHost : normalHost);
        final oldPid = backend.process.pid;
        final reviewed = await reviewGuestCrash(
          backend: backend,
          guestPackagePath: guestPackage,
          directory: directory,
          kind: 'create',
          point: point!,
          language: language!,
        );
        expect(
          reviewed.content,
          hasLength(GuestMutationValidation.maxContentBytes),
        );
        expect(reviewed.session.state?.permitDelivered, isFalse);
        expect(reviewed.session.state?.effectAttempted, isFalse);
        expect(reviewed.session.canExecute, isFalse);
        expectNoTargetFiles(reviewed.target.parent);
        final originalPlan = Uint8List.fromList(reviewed.plan);
        final originalHash = Uint8List.fromList(reviewed.planSha256);
        await saveGuestContentCrashPlan(directory, originalPlan);

        Object? prepareError;
        final prepareClock = Stopwatch()..start();
        try {
          await reviewed.session.prepare(reviewedPlanSha256: originalHash);
        } catch (error) {
          prepareError = error;
        }
        prepareClock.stop();
        // ignore: avoid_print
        print('GUEST_CONTENT_PREPARE_MS=${prepareClock.elapsedMilliseconds}');
        if (expectCrash) {
          expect(
            await backend.process.exitCode.timeout(const Duration(seconds: 30)),
            86,
          );
          crashedCurrent = true;
          expect(
            prepareError != null ||
                reviewed.session.error != null ||
                reviewed.session.uncertain,
            isTrue,
            reason:
                'A crashed Commit cannot deliver a successful prepare receipt',
          );
          expect(reviewed.session.canExecute, isFalse);
        } else {
          expect(prepareError, isNull);
          expect(reviewed.session.error, isNull);
          expect(reviewed.session.canExecute, isTrue);
          expect(reviewed.session.state?.durableContent, isTrue);
          expect(
            reviewed.session.stagedBytes,
            BigInt.from(reviewed.content.length),
          );
        }
        expect(reviewed.session.state?.permitDelivered, isFalse);
        expect(reviewed.session.state?.effectAttempted, isFalse);
        expectNoTargetFiles(reviewed.target.parent);

        if (!expectCrash) {
          // Explicitly close the prepared task; never call Execute.
          await reviewed.session.release();
          final clock = Stopwatch()..start();
          while (!reviewed.session.canAcknowledge &&
              clock.elapsed < const Duration(seconds: 25)) {
            await reviewed.session.refresh();
            await Future<void>.delayed(const Duration(milliseconds: 10));
          }
          expect(reviewed.session.canAcknowledge, isTrue);
          await reviewed.session.acknowledge();
          expectNoTargetFiles(reviewed.target.parent);
        }
        if (crashedCurrent) {
          try {
            await backend.close();
          } catch (_) {
            // This exact old process has already been observed exiting 86.
          }
        } else {
          await backend.close();
        }
        expect(
          await backend.process.exitCode.timeout(const Duration(seconds: 5)),
          crashedCurrent ? 86 : 0,
        );
        backend = null;
        crashedCurrent = false;

        backend = await open(normalHost);
        expect(backend.process.pid, isNot(oldPid));
        expectNoTargetFiles(reviewed.target.parent);
        final catalog = await entireCatalog(backend);
        final plugin = catalog.entries.singleWhere(
          (entry) => entry.id == reviewed.plugin.id,
        );
        expect(plugin.digest, reviewed.plugin.digest);
        expect(plugin.enabled, isTrue);
        expect(plugin.approvedIo, contains('file-create'));

        final freshGuest = GuestMutationExecutionSession.forBackend(
          backend.guestMutationTasks,
          backend,
        );
        await freshGuest.refresh();
        expect(freshGuest.canExecute, isFalse);

        final recovery = backend.mutationRecovery;
        await recovery.refresh();
        var sequence = 20;
        Future<MutationResult> oneReadOnlyPass() async {
          await recovery.startDiscovery(
            MutationDiscoverRequest(
              submission: guestCrashToken(sequence++),
              packageId: plugin.id,
              packageDigest: plugin.digest,
              registryRevision: catalog.revision,
              subject: guestCrashSubject,
              disposition: MutationDisposition.create,
              scanLimit: 8,
              timeoutMs: 30000,
            ),
          );
          await deliver(recovery, () => recovery.page != null);
          final page = recovery.page!;
          expect(page.done, isTrue);
          expect(page.plans, hasLength(1));
          expect(page.plans!.single, originalPlan);
          expect(page.phase, MutationPhase.none);
          expect(page.effect, MutationEffect.unspecified);
          expectNoTargetFiles(reviewed.target.parent);
          await closeRecovery(recovery, sequence++);
          await recovery.startReconciliation(
            MutationReconcileRequest(
              submission: guestCrashToken(sequence++),
              packageId: plugin.id,
              packageDigest: plugin.digest,
              registryRevision: catalog.revision,
              plan: originalPlan,
              timeoutMs: 30000,
            ),
          );
          await deliver(recovery, () => recovery.reconciliation != null);
          final result = recovery.reconciliation!;
          expect(result.phase, MutationPhase.prepared);
          expect(result.effect, MutationEffect.unspecified);
          expect(result.outcome, isNull);
          expect(result.reference, isNull);
          expect(result.operationId, reviewed.operationId);
          expect(result.record, isNotNull);
          expectNoTargetFiles(reviewed.target.parent);
          expect(freshGuest.canExecute, isFalse);
          await deliver(recovery, () => recovery.canAcknowledge);
          await recovery.acknowledge();
          return result;
        }

        final first = await oneReadOnlyPass();
        final firstRecord = Uint8List.fromList(first.record!);
        final second = await oneReadOnlyPass();
        expect(second.record, firstRecord);
        expect(second.phase, first.phase);
        expect(second.effect, first.effect);
        expect(second.outcome, isNull);
        expectNoTargetFiles(reviewed.target.parent);

        await backend.close();
        expect(
          await backend.process.exitCode.timeout(const Duration(seconds: 5)),
          0,
        );
        backend = null;
        allHostsExited = true;
        await verifyGuestContentCrashStore(
          directory: directory,
          plan: originalPlan,
          content: reviewed.content,
          point: point,
          expectCrash: expectCrash,
        );
        // No code path in this test invokes the guest Execute action.
        // ignore: avoid_print
        print('GUEST_CONTENT_EXECUTE_CALLS=0');
        // ignore: avoid_print
        print(
          'GUEST_CONTENT_CRASH_PASS=$language:$point:${expectCrash ? 1 : 0}',
        );
      } catch (_) {
        preserveStore = true;
        rethrow;
      } finally {
        final current = backend;
        var exited = current == null;
        Object? closeError;
        StackTrace? closeStack;
        if (current != null) {
          try {
            if (crashedCurrent) {
              try {
                await current.close();
              } catch (_) {
                // Suppress only the already-proven exit-86 process close error.
              }
            } else {
              await current.close();
            }
          } catch (error, stack) {
            closeError = error;
            closeStack = stack;
          }
          try {
            await current.process.exitCode.timeout(const Duration(seconds: 5));
            exited = true;
          } catch (_) {
            // Preserve the protected Store while process lifetime is unknown.
          }
        }
        if (exited && allHostsExited && !preserveStore) {
          try {
            await removeTestDirectory(directory);
          } catch (error, stack) {
            if (closeError == null) Error.throwWithStackTrace(error, stack);
            // ignore: avoid_print
            print('GUEST_CONTENT_CLEANUP_FAILED=${directory.path}: $error');
          }
        } else {
          // ignore: avoid_print
          print('GUEST_CONTENT_STORE_PRESERVED=${directory.path}');
        }
        if (closeError != null) {
          Error.throwWithStackTrace(closeError, closeStack!);
        }
      }
    },
    skip:
        !Platform.isWindows ||
        crashHost == null ||
        normalHost == null ||
        builtin == null ||
        guestPackage == null ||
        language == null ||
        moduleHash == null ||
        packageHash == null ||
        verifier == null ||
        point == null,
    timeout: const Timeout(Duration(minutes: 5)),
  );
}
