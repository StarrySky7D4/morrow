// A real SDK guest writes nonempty content; a new host reads its original
// protected history without regaining the selected target or replaying it.
import 'dart:io';
import 'dart:typed_data';

import 'package:crypto/crypto.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/guest_mutation_execution_session.dart';
import 'package:morrow_studio/plugins/guest_mutation_models.dart';
import 'package:morrow_studio/plugins/mutation_recovery_view_state.dart';
import 'package:morrow_studio/plugins/mutation_task_models.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';

import 'external_plugin_native_test.dart'
    show entireCatalog, removeTestDirectory;
import 'mutation_recovery_real_native_test.dart' show closeRecovery, deliver;

const _subject = 'morrow.guest.file-actions.v1';

Uint8List _token(int value) => Uint8List(32)..[0] = value;

Uint8List _body() {
  final length = 3 * GuestMutationValidation.maxChunkBytes + 73;
  final builder = BytesBuilder(copy: false);
  for (var counter = 0; builder.length < length; counter++) {
    builder.add(
      sha256.convert(<int>[
        ...'morrow.guest.recovery.nonempty.v1'.codeUnits,
        ...List<int>.generate(8, (index) => (counter >> (index * 8)) & 255),
      ]).bytes,
    );
  }
  return Uint8List.sublistView(builder.toBytes(), 0, length);
}

Future<void> _closeGuest(GuestMutationExecutionSession session) async {
  expect(session.canRelease, isTrue);
  await session.release();
  final clock = Stopwatch()..start();
  while (!session.canAcknowledge &&
      clock.elapsed < const Duration(seconds: 25)) {
    await session.refresh();
    expect(session.canRepair, isFalse);
    await Future<void>.delayed(const Duration(milliseconds: 10));
  }
  expect(session.canAcknowledge, isTrue, reason: 'guest worker must exit');
  await session.acknowledge();
  expect(session.canStart, isTrue);
}

void main() {
  final environment = Platform.environment;
  final executable = environment['MORROW_WORKBENCH_HOST'];
  final builtin = environment['MORROW_WORKBENCH_PACKAGE'];
  final fixture = environment['MORROW_GUEST_MUTATION_PACKAGE'];
  final language = environment['MORROW_GUEST_MUTATION_LANGUAGE'];
  final expectedModuleHash = environment['MORROW_GUEST_MUTATION_WASM_SHA256'];
  final expectedPackageHash =
      environment['MORROW_GUEST_MUTATION_PACKAGE_SHA256'];

  test(
    'real $language guest nonempty Create survives host restart as read-only history',
    () async {
      expect(language, anyOf('rust', 'c', 'cpp'));
      expect(expectedModuleHash, matches(RegExp(r'^[0-9a-f]{64}$')));
      expect(expectedPackageHash, matches(RegExp(r'^[0-9a-f]{64}$')));
      for (final path in [executable!, builtin!, fixture!]) {
        expect(File(path).existsSync(), isTrue, reason: path);
      }
      expect(
        sha256.convert(await File(fixture).readAsBytes()).toString(),
        expectedPackageHash,
      );
      final directory = await Directory.systemTemp.createTemp(
        'morrow-external-guest-recovery-$language-',
      );
      RustWorkbench? backend;
      try {
        Future<RustWorkbench> open() => RustWorkbench.open(
          executable: executable,
          package: builtin,
          directory: directory,
          managed: true,
        );

        backend = await open();
        final originalHostPid = backend.process.pid;
        final preview = await backend.inspectPlugin(fixture);
        final candidate = preview.entries.single;
        expect(candidate.mutationSupported, isTrue);
        expect(candidate.mutationBudget?.maxJobBytes, BigInt.from(32 << 20));
        expect(candidate.mutationBudget?.maxBytes, BigInt.from(256 << 20));
        await backend.importPlugin(fixture, candidate.digest, preview.revision);
        var catalog = await entireCatalog(backend);
        var plugin = catalog.entries.singleWhere(
          (entry) => entry.id == candidate.id,
        );
        await backend.configureExternalIo(plugin, catalog.revision, [
          'file-create',
          'file-delete',
        ]);
        catalog = await entireCatalog(backend);
        plugin = catalog.entries.singleWhere(
          (entry) => entry.id == candidate.id,
        );
        await backend.configureExternal(plugin, catalog.revision, [], true);
        catalog = await entireCatalog(backend);
        plugin = catalog.entries.singleWhere(
          (entry) => entry.id == candidate.id,
        );
        expect(plugin.digest, candidate.digest);
        expect(plugin.approvedIo, contains('file-create'));
        expect(plugin.enabled, isTrue);

        final targets = Directory(
          '${directory.path}${Platform.pathSeparator}targets',
        )..createSync();
        final target = File(
          '${targets.path}${Platform.pathSeparator}created.bin',
        );
        final content = _body();
        final contentHash = sha256.convert(content).toString();
        final guest = GuestMutationExecutionSession.forBackend(
          backend.guestMutationTasks,
          backend,
        );
        await guest.refresh();
        await guest.review(
          GuestMutationStartRequest(
            selection: MutationStartRequest(
              submission: _token(1),
              packageId: plugin.id,
              packageDigest: plugin.digest,
              registryRevision: catalog.revision,
              disposition: MutationDisposition.create,
              selectedPath: targets.path,
              relativePath: 'created.bin',
              subject: _subject,
              approvalSha256: _token(99),
              timeoutMs: 30000,
            ),
            approvedBudget: ApprovedGuestBudget(
              maxJobBytes: plugin.mutationBudget!.maxJobBytes,
              maxBytes: plugin.mutationBudget!.maxBytes,
            ),
          ),
          operationId: 'real-guest-recovery-$language',
          content: content,
        );
        final originalPlan = Uint8List.fromList(guest.plan!);
        final reviewHash = Uint8List.fromList(guest.planSha256!);
        expect(target.existsSync(), isFalse);
        await guest.prepare(reviewedPlanSha256: reviewHash);
        expect(guest.stagedBytes, BigInt.from(content.length));
        expect(guest.state?.durableContent, isTrue);
        expect(target.existsSync(), isFalse);
        await guest.execute(reviewedPlanSha256: reviewHash);
        expect(guest.outcome?.phase, GuestMutationFramePhase.observed);
        expect(guest.outcome?.effect, GuestMutationFrameEffect.osSucceeded);
        expect(
          sha256.convert(target.readAsBytesSync()).toString(),
          contentHash,
        );
        await guest.query();
        expect(guest.history?.frame?.phase, GuestMutationFramePhase.observed);
        await guest.hostQuery();
        final originalHistory = guest.history?.owner;
        expect(originalHistory?.kind, MutationResultKind.history);
        expect(originalHistory?.phase, MutationPhase.observed);
        expect(originalHistory?.effect, MutationEffect.unspecified);
        expect(originalHistory?.operationId, 'real-guest-recovery-$language');
        final originalRecord = Uint8List.fromList(originalHistory!.record!);
        expect(originalHistory.outcome, isNull);
        await _closeGuest(guest);
        final originalRecovery = backend.mutationRecovery;
        await originalRecovery.refresh();
        await originalRecovery.startReconciliation(
          MutationReconcileRequest(
            submission: _token(20),
            packageId: plugin.id,
            packageDigest: plugin.digest,
            registryRevision: catalog.revision,
            plan: originalPlan,
            timeoutMs: 30000,
          ),
        );
        await deliver(
          originalRecovery,
          () => originalRecovery.reconciliation != null,
        );
        final beforeRestart = originalRecovery.reconciliation!;
        expect(beforeRestart.phase, MutationPhase.observed);
        expect(beforeRestart.effect, MutationEffect.osSucceeded);
        expect(beforeRestart.operationId, 'real-guest-recovery-$language');
        expect(beforeRestart.record, originalRecord);
        final originalOutcome = Uint8List.fromList(beforeRestart.outcome!);
        await deliver(originalRecovery, () => originalRecovery.canAcknowledge);
        await originalRecovery.acknowledge();
        await backend.close();
        backend = null;

        // The target is no longer the original bytes. Historical inspection
        // must not recreate or overwrite it on the newly opened host.
        final replacement = Uint8List.fromList([9, 7, 5, 3, 1]);
        target.writeAsBytesSync(replacement, flush: true);
        final replacementHash = sha256.convert(replacement).toString();
        backend = await open();
        expect(backend.process.pid, isNot(originalHostPid));
        catalog = await entireCatalog(backend);
        plugin = catalog.entries.singleWhere(
          (entry) => entry.id == candidate.id,
        );
        expect(plugin.digest, candidate.digest);
        expect(plugin.mutationBudget?.maxJobBytes, BigInt.from(32 << 20));
        expect(plugin.mutationBudget?.maxBytes, BigInt.from(256 << 20));
        expect(plugin.approvedIo, contains('file-create'));
        expect(plugin.enabled, isTrue);
        final recovery = backend.mutationRecovery;
        final view = MutationRecoveryViewState.forSession(recovery);
        await recovery.refresh();
        var sequence = 50;

        MutationDiscoverRequest discovery(Uint8List digest) =>
            MutationDiscoverRequest(
              submission: _token(sequence++),
              packageId: plugin.id,
              packageDigest: digest,
              registryRevision: catalog.revision,
              subject: _subject,
              disposition: MutationDisposition.create,
              scanLimit: 8,
              timeoutMs: 30000,
            );

        // A wrong digest is a rejected, burned admission, never a fallback
        // to the installed package or a guest replay.
        await recovery.startDiscovery(discovery(_token(249)));
        await recovery.refresh();
        expect(recovery.snapshot?.key, isNull);
        expect(recovery.page, isNull);
        expect(recovery.canAbandon, isTrue);
        await recovery.abandon();
        expect(
          sha256.convert(target.readAsBytesSync()).toString(),
          replacementHash,
        );

        // Current approval is required after restart; historical approval
        // cannot be borrowed from the original guest task.
        await backend.configureExternalIo(plugin, catalog.revision, []);
        catalog = await entireCatalog(backend);
        plugin = catalog.entries.singleWhere(
          (entry) => entry.id == candidate.id,
        );
        expect(plugin.approvedIo, isNot(contains('file-create')));
        await recovery.startDiscovery(discovery(plugin.digest));
        await recovery.refresh();
        expect(recovery.snapshot?.key, isNull);
        expect(recovery.page, isNull);
        expect(recovery.canAbandon, isTrue);
        await recovery.abandon();
        expect(
          sha256.convert(target.readAsBytesSync()).toString(),
          replacementHash,
        );

        await backend.configureExternalIo(plugin, catalog.revision, [
          'file-create',
          'file-delete',
        ]);
        catalog = await entireCatalog(backend);
        plugin = catalog.entries.singleWhere(
          (entry) => entry.id == candidate.id,
        );
        expect(plugin.approvedIo, contains('file-create'));

        for (var repetition = 0; repetition < 2; repetition++) {
          await recovery.startDiscovery(discovery(plugin.digest));
          await deliver(recovery, () => recovery.page != null);
          expect(recovery.page!.done, isTrue);
          expect(recovery.page!.plans, hasLength(1));
          view.selectPlan(0);
          expect(view.selectedPlan, originalPlan);
          expect(view.selectedRequest!.subject, _subject);
          expect(view.selectedRequest!.disposition, MutationDisposition.create);
          await closeRecovery(recovery, sequence++);
          await recovery.startReconciliation(
            MutationReconcileRequest(
              submission: _token(sequence++),
              packageId: plugin.id,
              packageDigest: plugin.digest,
              registryRevision: catalog.revision,
              plan: view.selectedPlan!,
              timeoutMs: 30000,
            ),
          );
          await deliver(recovery, () => recovery.reconciliation != null);
          final result = recovery.reconciliation!;
          expect(result.phase, MutationPhase.observed);
          expect(result.effect, MutationEffect.osSucceeded);
          expect(result.operationId, 'real-guest-recovery-$language');
          expect(result.record, originalRecord);
          expect(result.outcome, originalOutcome);
          await deliver(recovery, () => recovery.canAcknowledge);
          await recovery.acknowledge();
          expect(
            sha256.convert(target.readAsBytesSync()).toString(),
            replacementHash,
          );
        }
        // ignore: avoid_print
        print('GUEST_RECOVERY_PACKAGE_SHA256=$expectedPackageHash');
        // ignore: avoid_print
        print('GUEST_RECOVERY_ORIGINAL_SHA256=$contentHash');
        // ignore: avoid_print
        print('GUEST_RECOVERY_PASS=$language');
      } finally {
        try {
          await backend?.close();
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
        language == null ||
        expectedModuleHash == null ||
        expectedPackageHash == null,
    timeout: const Timeout(Duration(minutes: 4)),
  );
}
