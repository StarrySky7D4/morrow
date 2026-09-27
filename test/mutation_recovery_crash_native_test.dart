// Fault-injected Windows host process, followed by read-only recovery from a
// newly opened host over the same temporary protected library.
import 'dart:io';
import 'dart:typed_data';

import 'package:crypto/crypto.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/mutation_recovery_view_state.dart';
import 'package:morrow_studio/plugins/mutation_task_models.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';

import 'external_plugin_native_test.dart'
    show entireCatalog, removeTestDirectory;
import 'mutation_recovery_real_native_test.dart'
    show closeRecovery, deliver, readCommand, reclaim;

Uint8List _token(int value) => Uint8List(32)..[0] = value;

void main() {
  final environment = Platform.environment;
  final executable = environment['MORROW_MUTATION_CRASH_HOST'];
  final builtin = environment['MORROW_WORKBENCH_PACKAGE'];
  final fixture = environment['MORROW_MUTATION_TASK_PACKAGE'];
  final kind = environment['MORROW_MUTATION_CRASH_KIND'];
  final point = environment['MORROW_MUTATION_CRASH_POINT'];
  final expectCrash = environment['MORROW_MUTATION_EXPECT_CRASH'] != '0';
  final faultVariable = kind == 'delete'
      ? 'MORROW_FILE_DELETE_FAULT'
      : 'MORROW_FILE_CREATE_FAULT';
  const subject = 'native.crash.recovery';

  test(
    'real ${kind ?? 'unset'} ${point ?? 'unset'} expectCrash=$expectCrash recovers without replay',
    () async {
      expect(kind, anyOf('create', 'delete'));
      expect(point, anyOf('after-claim', 'after-effect', 'after-observe'));
      expect(environment[faultVariable], point);
      if (!expectCrash) expect(kind, 'create');

      final directory = await Directory.systemTemp.createTemp(
        'morrow-external-mutation-crash-recovery-',
      );
      RustWorkbench? backend;
      var crashedCurrent = false;
      try {
        Future<RustWorkbench> open() => RustWorkbench.open(
          executable: executable!,
          package: builtin!,
          directory: directory,
          managed: true,
        );

        backend = await open();
        final preview = await backend.inspectPlugin(fixture!);
        final candidate = preview.entries.single;
        expect(
          candidate.declaredIo,
          contains(kind == 'create' ? 'file-create' : 'file-delete'),
        );
        await backend.importPlugin(fixture, candidate.digest, preview.revision);
        var catalog = await entireCatalog(backend);
        var plugin = catalog.entries.singleWhere((e) => e.id == candidate.id);
        await backend.configureExternalIo(plugin, catalog.revision, [
          kind == 'create' ? 'file-create' : 'file-delete',
        ]);
        catalog = await entireCatalog(backend);
        plugin = catalog.entries.singleWhere((e) => e.id == candidate.id);
        await backend.configureExternal(plugin, catalog.revision, [], true);
        catalog = await entireCatalog(backend);
        plugin = catalog.entries.singleWhere((e) => e.id == candidate.id);

        final targets = Directory('${directory.path}/targets')..createSync();
        final created = File('${targets.path}/created.bin');
        final deleted = File('${targets.path}/deleted.bin')
          ..writeAsBytesSync([4, 5, 6]);
        // Inspect the fixture before native selection takes an exclusive handle.
        expect(deleted.readAsBytesSync(), [4, 5, 6]);
        final isCreate = kind == 'create';
        final disposition = isCreate
            ? MutationDisposition.create
            : MutationDisposition.delete;
        final target = isCreate ? created : deleted;
        final start = await backend.mutationTasks.startSelected(
          MutationStartRequest(
            submission: _token(1),
            packageId: plugin.id,
            packageDigest: plugin.digest,
            registryRevision: catalog.revision,
            disposition: disposition,
            selectedPath: isCreate ? targets.path : deleted.path,
            relativePath: isCreate ? 'created.bin' : '',
            subject: subject,
            approvalSha256: _token(99),
            timeoutMs: 30000,
          ),
        );
        final key = start.io.key!;
        expect(
          (await readCommand(backend, start)).kind,
          MutationResultKind.selected,
        );
        final planned = await readCommand(
          backend,
          await backend.mutationTasks.submitBuildPlan(
            key,
            _token(2),
            operationId: 'crash-$kind-$point',
            contentLength: BigInt.zero,
            contentSha256: isCreate
                ? Uint8List.fromList(sha256.convert([]).bytes)
                : null,
          ),
        );
        final originalPlan = planned.plan!;
        expect(originalPlan, isNotEmpty);
        expect(
          (await readCommand(
            backend,
            await backend.mutationTasks.submitPrepare(
              key,
              _token(3),
              originalPlan,
            ),
          )).phase,
          MutationPhase.prepared,
        );
        if (isCreate) {
          final committed = await readCommand(
            backend,
            await backend.mutationTasks.submitCommitContent(key, _token(4)),
          );
          expect(committed.durableContent, isTrue);
          expect(created.existsSync(), isFalse);
        }

        Object? executeError;
        MutationResult? executed;
        try {
          final reply = await backend.mutationTasks.submitExecute(
            key,
            _token(5),
          );
          executed = await readCommand(backend, reply);
        } catch (error) {
          executeError = error;
        }

        if (expectCrash) {
          expect(executeError, isNotNull);
          expect(executed, isNull); // No business failure was delivered.
          expect(await backend.process.exitCode, 86);
          crashedCurrent = true;
        } else {
          expect(executeError, isNull);
          expect(executed!.kind, MutationResultKind.created);
          expect(executed.effect, MutationEffect.osSucceeded);
          expect(created.readAsBytesSync(), isEmpty);
          expect(
            (await readCommand(
              backend,
              await backend.mutationTasks.submitRelease(key, _token(6)),
            )).kind,
            MutationResultKind.released,
          );
          await reclaim(backend, key);
        }

        final shouldExist = !expectCrash || point != 'after-claim';
        expect(target.existsSync(), isCreate ? shouldExist : !shouldExist);
        if (target.existsSync() && !isCreate) {
          expect(target.readAsBytesSync(), [4, 5, 6]);
        }
        if (crashedCurrent) {
          try {
            await backend.close();
          } catch (_) {
            // close() reports the already-proven exit 86 for this old owner.
          }
        } else {
          await backend.close();
        }
        backend = null;
        crashedCurrent = false;

        backend = await open(); // New host, same protected library.
        catalog = await entireCatalog(backend);
        plugin = catalog.entries.singleWhere((e) => e.id == candidate.id);
        final session = backend.mutationRecovery;
        final view = MutationRecoveryViewState.forSession(session);
        var sequence = 10;
        Future<void> discoverOriginal() async {
          await session.startDiscovery(
            MutationDiscoverRequest(
              submission: _token(sequence++),
              packageId: plugin.id,
              packageDigest: plugin.digest,
              registryRevision: catalog.revision,
              subject: subject,
              disposition: disposition,
              scanLimit: 8,
              timeoutMs: 30000,
            ),
          );
          await deliver(session, () => session.page != null);
          expect(session.page!.done, isTrue);
          expect(session.page!.plans, hasLength(1));
          expect(session.page!.plans!.single, originalPlan);
          expect(session.page!.effect, MutationEffect.unspecified);
          expect(session.page!.phase, MutationPhase.none);
        }

        Future<MutationResult> reconcileOriginal() async {
          await session.startReconciliation(
            MutationReconcileRequest(
              submission: _token(sequence++),
              packageId: plugin.id,
              packageDigest: plugin.digest,
              registryRevision: catalog.revision,
              plan: view.selectedPlan!,
              timeoutMs: 30000,
            ),
          );
          await deliver(session, () => session.reconciliation != null);
          final value = session.reconciliation!;
          await deliver(session, () => session.canAcknowledge);
          await session.acknowledge();
          return value;
        }

        await session.refresh();
        await discoverOriginal();
        view.selectPlan(0);
        expect(view.selectedPlan, originalPlan);
        expect(view.selectedRequest!.subject, subject);
        expect(view.selectedRequest!.disposition, disposition);
        await closeRecovery(session, sequence++);
        final beforeReconcile = target.existsSync();

        final reconciliation = await reconcileOriginal();
        final observed = !expectCrash || point == 'after-observe';
        expect(
          reconciliation.phase,
          observed ? MutationPhase.observed : MutationPhase.outcomeUnknown,
        );
        expect(
          reconciliation.effect,
          observed ? MutationEffect.osSucceeded : MutationEffect.unspecified,
        );
        expect(target.existsSync(), beforeReconcile);
        expect(view.lastReconciliation, same(reconciliation));

        // A second historical read is still only discovery of the original
        // request. It neither renews execution nor changes the result.
        await discoverOriginal();
        expect(target.existsSync(), beforeReconcile);
        expect(view.lastReconciliation, same(reconciliation));
        await closeRecovery(session, sequence++);
        final verifiedAgain = await reconcileOriginal();
        expect(verifiedAgain.phase, reconciliation.phase);
        expect(verifiedAgain.effect, reconciliation.effect);
        expect(verifiedAgain.operationId, reconciliation.operationId);
        expect(verifiedAgain.record, reconciliation.record);
        expect(verifiedAgain.outcome, reconciliation.outcome);
        expect(target.existsSync(), beforeReconcile);
      } finally {
        try {
          if (backend != null) {
            if (crashedCurrent) {
              try {
                await backend.close();
              } catch (_) {
                // Only the previously proven crashed owner may fail close.
              }
            } else {
              await backend.close();
            }
          }
        } finally {
          await removeTestDirectory(directory);
        }
      }
    },
    skip:
        !Platform.isWindows ||
        executable == null ||
        builtin == null ||
        fixture == null ||
        kind == null ||
        point == null,
    timeout: const Timeout(Duration(minutes: 3)),
  );
}
