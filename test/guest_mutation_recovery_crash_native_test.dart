// A real SDK guest loses its original host during Execute; a new ordinary
// host discovers and reconciles only the immutable protected request history.
import 'dart:io';
import 'dart:typed_data';

import 'package:crypto/crypto.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/guest_mutation_models.dart';
import 'package:morrow_studio/plugins/mutation_recovery_session.dart';
import 'package:morrow_studio/plugins/mutation_recovery_view_state.dart';
import 'package:morrow_studio/plugins/mutation_task_models.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';

import 'external_plugin_native_test.dart'
    show entireCatalog, removeTestDirectory;
import 'guest_mutation_crash_support.dart';
import 'mutation_recovery_real_native_test.dart' show closeRecovery, deliver;

// Capture a consumed failure before the session converts it to a safe phase.
final class _ObservedRecoveryBackend implements MutationTaskBackend {
  _ObservedRecoveryBackend(this.inner);
  final MutationTaskBackend inner;
  MutationFailure? lastFailure;

  @override
  Future<MutationTaskReply> startDiscovery(MutationDiscoverRequest request) =>
      inner.startDiscovery(request);
  @override
  Future<MutationTaskReply> startReconciliation(
    MutationReconcileRequest request,
  ) => inner.startReconciliation(request);
  @override
  Future<MutationTaskReply> submitRelease(
    Uint8List key,
    Uint8List submission,
  ) => inner.submitRelease(key, submission);
  @override
  Future<MutationTaskReply> status(Uint8List key) => inner.status(key);
  @override
  Future<MutationTaskRead> read(Uint8List key, BigInt commandId) async {
    final value = await inner.read(key, commandId);
    if (value.result?.kind == MutationResultKind.failure) {
      lastFailure = value.result!.failure;
    }
    return value;
  }

  @override
  dynamic noSuchMethod(Invocation invocation) =>
      throw StateError('Unexpected recovery backend method');
}

List<String> _createTemporarySite(File target) {
  final names = <String>[];
  for (final entity in target.parent.listSync(followLinks: false)) {
    if (entity is! File) continue;
    final name = entity.path.split(Platform.pathSeparator).last;
    if (!name.startsWith('.morrow-create-')) continue;
    expect(name, matches(RegExp(r'^\.morrow-create-[0-9a-f]{64}\.tmp$')));
    final size = entity.lengthSync();
    expect(size, lessThanOrEqualTo(GuestMutationValidation.maxContentBytes));
    names.add('$name:$size:${sha256.convert(entity.readAsBytesSync())}');
  }
  names.sort();
  return names;
}

void main() {
  final environment = Platform.environment;
  final crashHost = environment['MORROW_MUTATION_CRASH_HOST'];
  final normalHost = environment['MORROW_WORKBENCH_HOST'];
  final builtin = environment['MORROW_WORKBENCH_PACKAGE'];
  final guestPackage = environment['MORROW_GUEST_MUTATION_PACKAGE'];
  final language = environment['MORROW_GUEST_MUTATION_LANGUAGE'];
  final moduleHash = environment['MORROW_GUEST_MUTATION_WASM_SHA256'];
  final packageHash = environment['MORROW_GUEST_MUTATION_PACKAGE_SHA256'];
  final kind = environment['MORROW_MUTATION_CRASH_KIND'];
  final point = environment['MORROW_MUTATION_CRASH_POINT'];
  final expectCrash = environment['MORROW_MUTATION_EXPECT_CRASH'] != '0';
  final faultVariable = kind == 'delete'
      ? 'MORROW_FILE_DELETE_FAULT'
      : 'MORROW_FILE_CREATE_FAULT';

  test(
    'real guest $language $kind $point expectCrash=$expectCrash recovers without replay',
    () async {
      expect(language, anyOf('rust', 'c', 'cpp'));
      expect(kind, anyOf('create', 'delete'));
      guestCrashEffectHappened(kind!, point!, expectCrash);
      expect(environment[faultVariable], point);
      if (!expectCrash) {
        expect(point, 'after-claim');
      }
      expect(moduleHash, matches(RegExp(r'^[0-9a-f]{64}$')));
      expect(packageHash, matches(RegExp(r'^[0-9a-f]{64}$')));
      for (final path in [crashHost!, normalHost!, builtin!, guestPackage!]) {
        expect(File(path).existsSync(), isTrue, reason: path);
      }
      expect(
        sha256.convert(await File(guestPackage).readAsBytes()).toString(),
        packageHash,
      );
      final directory = await Directory.systemTemp.createTemp(
        'morrow-external-guest-crash-recovery-$language-',
      );
      RustWorkbench? backend;
      var crashedCurrent = false;
      var preserveFailureStore = false;
      try {
        Future<RustWorkbench> open(String executable) => RustWorkbench.open(
          executable: executable,
          package: builtin,
          directory: directory,
          managed: true,
        );

        backend = await open(crashHost);
        final oldPid = backend.process.pid;
        final prepared = await prepareGuestCrash(
          backend: backend,
          guestPackagePath: guestPackage,
          directory: directory,
          kind: kind,
          point: point,
          language: language!,
        );
        final originalPlan = prepared.plan;
        expect(prepared.session.state?.reference, hasLength(32));
        final originalContentHash = sha256.convert(prepared.content).toString();

        Object? executeError;
        final executeClock = Stopwatch()..start();
        try {
          await prepared.session.execute(
            reviewedPlanSha256: prepared.planSha256,
          );
        } catch (error) {
          executeError = error;
        }
        executeClock.stop();
        // ignore: avoid_print
        print('GUEST_CRASH_EXECUTE_MS=${executeClock.elapsedMilliseconds}');
        if (expectCrash) {
          expect(
            await backend.process.exitCode.timeout(const Duration(seconds: 30)),
            86,
          );
          crashedCurrent = true;
          expect(
            prepared.session.outcome?.effect,
            isNot(GuestMutationFrameEffect.osSucceeded),
            reason: 'a crashed transport did not deliver an effect receipt',
          );
          expect(
            executeError != null ||
                prepared.session.error != null ||
                prepared.session.uncertain,
            isTrue,
          );
        } else {
          expect(executeError, isNull);
          expect(prepared.session.error, isNull);
          expect(
            prepared.session.outcome?.phase,
            GuestMutationFramePhase.observed,
          );
          expect(
            prepared.session.outcome?.effect,
            GuestMutationFrameEffect.osSucceeded,
          );
          await prepared.session.release();
          final clock = Stopwatch()..start();
          while (!prepared.session.canAcknowledge &&
              clock.elapsed < const Duration(seconds: 25)) {
            await prepared.session.refresh();
            await Future<void>.delayed(const Duration(milliseconds: 10));
          }
          expect(prepared.session.canAcknowledge, isTrue);
          await prepared.session.acknowledge();
        }

        final effectHappened = guestCrashEffectHappened(
          kind,
          point,
          expectCrash,
        );
        expect(
          prepared.target.existsSync(),
          kind == 'create' ? effectHappened : !effectHappened,
        );
        if (prepared.target.existsSync()) {
          expect(
            sha256.convert(prepared.target.readAsBytesSync()).toString(),
            originalContentHash,
          );
        }
        final temporarySite = _createTemporarySite(prepared.target);
        if (kind == 'create' &&
            expectCrash &&
            {
              'after-temp',
              'after-write-chunk',
              'after-write',
              'after-flush',
            }.contains(point)) {
          expect(temporarySite, hasLength(1));
          final temporary = File(
            '${prepared.target.parent.path}${Platform.pathSeparator}${temporarySite.single.split(':').first}',
          );
          final expectedSize = switch (point) {
            'after-temp' => 0,
            'after-write-chunk' =>
              prepared.content.length < 64 * 1024
                  ? prepared.content.length
                  : 64 * 1024,
            _ => prepared.content.length,
          };
          expect(temporary.lengthSync(), expectedSize);
          expect(
            temporary.readAsBytesSync(),
            prepared.content.sublist(0, expectedSize),
          );
        } else {
          expect(temporarySite, isEmpty);
        }
        if (crashedCurrent) {
          try {
            await backend.close();
          } catch (_) {
            // Only this owner's exact exit 86 has already been proven.
          }
        } else {
          await backend.close();
        }
        backend = null;
        crashedCurrent = false;

        backend = await open(normalHost);
        expect(backend.process.pid, isNot(oldPid));
        var catalog = await entireCatalog(backend);
        var plugin = catalog.entries.singleWhere(
          (entry) => entry.id == prepared.plugin.id,
        );
        expect(plugin.digest, prepared.plugin.digest);
        expect(plugin.mutationBudget?.maxBytes, BigInt.from(256 << 20));
        expect(plugin.enabled, isTrue);
        expect(
          plugin.approvedIo,
          contains(kind == 'create' ? 'file-create' : 'file-delete'),
        );

        // A post-crash marker makes an accidental replay visible even when
        // the original OS effect had already completed before the crash.
        if (effectHappened) {
          prepared.target.writeAsBytesSync([9, 7, 5, 3, 1], flush: true);
        }
        final targetPresent = prepared.target.existsSync();
        final targetHash = targetPresent
            ? sha256.convert(prepared.target.readAsBytesSync()).toString()
            : null;
        void expectTargetUnchanged() {
          expect(prepared.target.existsSync(), targetPresent);
          expect(_createTemporarySite(prepared.target), temporarySite);
          if (targetHash != null) {
            expect(
              sha256.convert(prepared.target.readAsBytesSync()).toString(),
              targetHash,
            );
          }
        }

        final observedBackend = _ObservedRecoveryBackend(backend.mutationTasks);
        final recovery = MutationRecoverySession.forBackend(
          observedBackend,
          backend,
        );
        final view = MutationRecoveryViewState.forSession(recovery);
        await recovery.refresh();
        Future<void> atStage(
          String stage,
          Future<void> Function() action,
        ) async {
          // ignore: avoid_print
          print('GUEST_RECOVERY_STAGE=$stage');
          try {
            await action();
          } catch (error) {
            preserveFailureStore = true;
            final io = recovery.snapshot;
            final state = recovery.state;
            final failure = observedBackend.lastFailure;
            final message = error.toString();
            // ignore: avoid_print
            print(
              'GUEST_RECOVERY_DIAGNOSTIC stage=$stage error=${error.runtimeType}:${message.length <= 256 ? message : message.substring(0, 256)} phase=${recovery.phase.name} notice=${recovery.notice?.name} io=${io?.storage.name}/${io?.delivery.name} hasKey=${io?.key != null} hasExit=${io?.exit != null} command=${state?.kind.name}/${state?.commandId}/${state?.delivery.name} terminal=${state?.terminal} reconcile=${state?.reconcileRequired} canRead=${recovery.canRead} canRelease=${recovery.canRelease} failure=${failure?.layer.name}/${failure?.delivery?.name}/${failure?.target?.name}',
            );
            rethrow;
          }
        }

        var sequence = 20;
        MutationDiscoverRequest discovery() => MutationDiscoverRequest(
          submission: guestCrashToken(sequence++),
          packageId: plugin.id,
          packageDigest: plugin.digest,
          registryRevision: catalog.revision,
          subject: guestCrashSubject,
          disposition: prepared.disposition,
          scanLimit: 8,
          timeoutMs: 30000,
        );

        Future<void> discoverOriginal() async {
          await atStage('start-discovery-$sequence', () async {
            await recovery.startDiscovery(discovery());
          });
          await atStage('read-discovery-$sequence', () async {
            await deliver(recovery, () => recovery.page != null);
          });
          expect(recovery.page!.done, isTrue);
          expect(recovery.page!.plans, hasLength(1));
          expect(recovery.page!.plans!.single, originalPlan);
          expect(recovery.page!.phase, MutationPhase.none);
          expect(recovery.page!.effect, MutationEffect.unspecified);
          view.selectPlan(0);
          expect(view.selectedPlan, originalPlan);
          expect(view.selectedRequest!.packageId, prepared.plugin.id);
          expect(view.selectedRequest!.packageDigest, prepared.plugin.digest);
          expect(view.selectedRequest!.subject, guestCrashSubject);
          expect(view.selectedRequest!.disposition, prepared.disposition);
          expectTargetUnchanged();
        }

        Future<MutationResult> reconcileOriginal() async {
          await atStage('start-reconciliation-$sequence', () async {
            await recovery.startReconciliation(
              MutationReconcileRequest(
                submission: guestCrashToken(sequence++),
                packageId: plugin.id,
                packageDigest: plugin.digest,
                registryRevision: catalog.revision,
                plan: view.selectedPlan!,
                timeoutMs: 30000,
              ),
            );
          });
          await atStage('read-reconciliation-$sequence', () async {
            await deliver(recovery, () => recovery.reconciliation != null);
          });
          final value = recovery.reconciliation!;
          await atStage('exit-reconciliation-$sequence', () async {
            await deliver(recovery, () => recovery.canAcknowledge);
          });
          await atStage('ack-reconciliation-$sequence', recovery.acknowledge);
          expectTargetUnchanged();
          return value;
        }

        await discoverOriginal();
        await atStage('close-discovery-$sequence', () async {
          await closeRecovery(recovery, sequence++);
        });
        final first = await reconcileOriginal();
        final observed = !expectCrash || point == 'after-observe';
        expect(
          first.phase,
          observed ? MutationPhase.observed : MutationPhase.outcomeUnknown,
        );
        expect(
          first.effect,
          observed ? MutationEffect.osSucceeded : MutationEffect.unspecified,
        );
        expect(first.operationId, prepared.operationId);
        expect(first.reference, isNull); // Read-only history is not a lease.
        expect(first.record, isNotNull);
        if (observed) {
          expect(first.outcome, isNotNull);
        } else {
          expect(first.outcome, isNull);
        }
        final record = Uint8List.fromList(first.record!);
        final outcome = first.outcome == null
            ? null
            : Uint8List.fromList(first.outcome!);

        // A second independent historical read proves the first one did not
        // promote Unknown, mutate closure evidence, or replay the OS effect.
        await discoverOriginal();
        await atStage('close-discovery-$sequence', () async {
          await closeRecovery(recovery, sequence++);
        });
        final second = await reconcileOriginal();
        expect(second.phase, first.phase);
        expect(second.effect, first.effect);
        expect(second.operationId, first.operationId);
        expect(second.reference, isNull);
        expect(second.record, record);
        expect(second.outcome, outcome);
        expectTargetUnchanged();

        // ignore: avoid_print
        print('GUEST_CRASH_RECOVERY_PACKAGE_SHA256=$packageHash');
        // ignore: avoid_print
        print(
          'GUEST_CRASH_RECOVERY_PASS=$language:$kind:$point:${expectCrash ? 1 : 0}',
        );
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
                // Only an already-proven exit 86 permits this old close error.
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
            // An unknown process lifetime must not lose its protected Store.
          }
        }
        if (exited && !preserveFailureStore) {
          try {
            await removeTestDirectory(directory);
          } catch (error, stack) {
            if (closeError == null) Error.throwWithStackTrace(error, stack);
            // ignore: avoid_print
            print('GUEST_CRASH_CLEANUP_FAILED=${directory.path}: $error');
          }
        } else {
          // ignore: avoid_print
          print('GUEST_CRASH_STORE_PRESERVED=${directory.path}');
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
        kind == null ||
        point == null,
    timeout: const Timeout(Duration(minutes: 5)),
  );
}
