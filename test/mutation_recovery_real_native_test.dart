// Real Windows process transport with a new temporary protected content store.
import 'dart:io';
import 'dart:typed_data';
import 'package:crypto/crypto.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/io_task_models.dart';
import 'package:morrow_studio/plugins/mutation_task_models.dart';
import 'package:morrow_studio/plugins/mutation_recovery_session.dart';
import 'package:morrow_studio/plugins/mutation_recovery_view_state.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';
import 'external_plugin_native_test.dart'
    show entireCatalog, removeTestDirectory;

Uint8List token(int id) => Uint8List(32)..[0] = id;
const subject = 'native.recovery';

Future<MutationResult> readCommand(
  RustWorkbench backend,
  MutationTaskReply reply,
) async {
  final timer = Stopwatch()..start();
  while (timer.elapsed < const Duration(seconds: 15)) {
    final value = await backend.mutationTasks.read(
      reply.io.key!,
      reply.commandId,
    );
    if (value.result != null) return value.result!;
    await Future<void>.delayed(const Duration(milliseconds: 10));
  }
  throw StateError('mutation read deadline');
}

Future<void> reclaim(RustWorkbench backend, Uint8List key) async {
  final timer = Stopwatch()..start();
  while (timer.elapsed < const Duration(seconds: 15)) {
    final io = await backend.pollIo(key);
    if (io.exit != null && io.storage == IoStoragePhase.reclaimed) {
      await backend.acknowledgeIo(key);
      return;
    }
    if (io.exit != null && io.storage == IoStoragePhase.recoveryRequired) {
      fail('temporary library unexpectedly needs repair');
    }
    await Future<void>.delayed(const Duration(milliseconds: 10));
  }
  throw StateError('real worker join deadline');
}

Future<void> deliver(
  MutationRecoverySession session,
  bool Function() done,
) async {
  final timer = Stopwatch()..start();
  while (timer.elapsed < const Duration(seconds: 15)) {
    await session.refresh();
    if (session.canRead) await session.read();
    if (done()) return;
    await Future<void>.delayed(const Duration(milliseconds: 10));
  }
  throw StateError(
    'recovery delivery deadline: ${session.phase} / ${session.notice}',
  );
}

Future<void> closeRecovery(MutationRecoverySession session, int id) async {
  expect(session.canRelease, isTrue);
  await session.release(token(id));
  await deliver(session, () => session.canAcknowledge);
  await session.acknowledge();
  expect(session.canStart, isTrue);
}

// Drops one already-consumed reply at the Dart boundary, never replays it.
class LoseReadReply implements MutationTaskBackend {
  LoseReadReply(this.inner);
  final MutationTaskBackend inner;
  int reads = 0;
  bool lose = true;
  @override
  Future<MutationTaskReply> startDiscovery(MutationDiscoverRequest request) =>
      inner.startDiscovery(request);
  @override
  Future<MutationTaskReply> status(Uint8List key) => inner.status(key);
  @override
  Future<MutationTaskReply> submitRelease(
    Uint8List key,
    Uint8List submission,
  ) => inner.submitRelease(key, submission);
  @override
  Future<MutationTaskRead> read(Uint8List key, BigInt command) async {
    reads++;
    final value = await inner.read(key, command);
    if (lose && value.result != null) {
      lose = false;
      throw StateError(
        'simulated lost Dart reply after actual host consumption',
      );
    }
    return value;
  }

  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}

void main() {
  final executable = Platform.environment['MORROW_WORKBENCH_HOST'];
  final builtin = Platform.environment['MORROW_WORKBENCH_PACKAGE'];
  final fixture = Platform.environment['MORROW_MUTATION_TASK_PACKAGE'];
  test(
    'real host restart discovers original plans and independently reconciles Observed and Prepared',
    () async {
      final directory = await Directory.systemTemp.createTemp(
        'morrow-external-mutation-recovery-',
      );
      RustWorkbench? backend;
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
          containsAll(['file-create', 'file-delete']),
        );
        await backend.importPlugin(fixture, candidate.digest, preview.revision);
        var catalog = await entireCatalog(backend);
        var plugin = catalog.entries.singleWhere((e) => e.id == candidate.id);
        await backend.configureExternalIo(plugin, catalog.revision, [
          'file-create',
          'file-delete',
        ]);
        catalog = await entireCatalog(backend);
        plugin = catalog.entries.singleWhere((e) => e.id == candidate.id);
        await backend.configureExternal(plugin, catalog.revision, [], true);
        catalog = await entireCatalog(backend);
        plugin = catalog.entries.singleWhere((e) => e.id == candidate.id);
        final target = Directory('${directory.path}/targets')..createSync();
        final retained = File('${target.path}/retained.bin')
          ..writeAsBytesSync([4, 5, 6]);
        final originals = <MutationDisposition, Uint8List>{};
        var sequence = 1;
        for (final disposition in [
          MutationDisposition.create,
          MutationDisposition.delete,
        ]) {
          final start = await backend.mutationTasks.startSelected(
            MutationStartRequest(
              submission: token(sequence++),
              packageId: plugin.id,
              packageDigest: plugin.digest,
              registryRevision: catalog.revision,
              disposition: disposition,
              selectedPath: disposition == MutationDisposition.create
                  ? target.path
                  : retained.path,
              relativePath: disposition == MutationDisposition.create
                  ? 'created.bin'
                  : '',
              subject: subject,
              approvalSha256: token(99),
              timeoutMs: 30000,
            ),
          );
          final key = start.io.key!;
          expect(
            (await readCommand(backend, start)).kind,
            MutationResultKind.selected,
          );
          final built = await readCommand(
            backend,
            await backend.mutationTasks.submitBuildPlan(
              key,
              token(sequence++),
              operationId: 'native-${disposition.name}',
              contentLength: BigInt.zero,
              contentSha256: disposition == MutationDisposition.create
                  ? Uint8List.fromList(sha256.convert([]).bytes)
                  : null,
            ),
          );
          originals[disposition] = built.plan!;
          expect(
            (await readCommand(
              backend,
              await backend.mutationTasks.submitPrepare(
                key,
                token(sequence++),
                built.plan!,
              ),
            )).phase,
            MutationPhase.prepared,
          );
          if (disposition == MutationDisposition.create) {
            expect(
              (await readCommand(
                backend,
                await backend.mutationTasks.submitCommitContent(
                  key,
                  token(sequence++),
                ),
              )).durableContent,
              isTrue,
            );
            final executed = await readCommand(
              backend,
              await backend.mutationTasks.submitExecute(key, token(sequence++)),
            );
            expect(executed.effect, MutationEffect.osSucceeded);
            expect(File('${target.path}/created.bin').existsSync(), isTrue);
          }
          expect(
            (await readCommand(
              backend,
              await backend.mutationTasks.submitRelease(key, token(sequence++)),
            )).kind,
            MutationResultKind.released,
          );
          await reclaim(backend, key);
        }
        await backend.close();
        backend = null;
        backend =
            await open(); // Actual new Rust process, same temporary protected store.
        catalog = await entireCatalog(backend);
        plugin = catalog.entries.singleWhere((e) => e.id == candidate.id);
        final session = backend.mutationRecovery;
        final view = MutationRecoveryViewState.forSession(session);
        expect(identical(session, backend.mutationRecovery), isTrue);
        expect(
          identical(
            session,
            MutationRecoverySession.forBackend(backend.mutationTasks, backend),
          ),
          isTrue,
        );
        await session.refresh();
        await session.startDiscovery(
          MutationDiscoverRequest(
            submission: token(sequence++),
            packageId: plugin.id,
            packageDigest: plugin.digest,
            registryRevision: catalog.revision,
            subject: subject,
            disposition: MutationDisposition.create,
            scanLimit: 1,
            timeoutMs: 30000,
          ),
        );
        await deliver(session, () => session.page != null);
        expect(session.page!.done, isFalse);
        expect(
          session.page!.plans!.single,
          originals[MutationDisposition.create],
        );
        final continuation = session.checkpoint!;
        await closeRecovery(session, sequence++);
        await session.startDiscovery(
          MutationDiscoverRequest(
            submission: token(sequence++),
            packageId: plugin.id,
            packageDigest: plugin.digest,
            registryRevision: catalog.revision,
            subject: subject,
            disposition: MutationDisposition.create,
            scanLimit: 1,
            timeoutMs: 30000,
            checkpoint: continuation,
          ),
        );
        await deliver(session, () => session.page != null);
        expect(
          session.page!.plans,
          isEmpty,
        ); // Only foreign delete candidate remains.
        expect(session.page!.done, isTrue);
        expect(session.checkpoint, isNull);
        await closeRecovery(session, sequence++);
        for (final disposition in [
          MutationDisposition.create,
          MutationDisposition.delete,
        ]) {
          await session.startDiscovery(
            MutationDiscoverRequest(
              submission: token(sequence++),
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
          view.selectPlan(0);
          final original = view.selectedPlan!;
          expect(original, originals[disposition]);
          expect(view.selectedRequest!.disposition, disposition);
          expect(view.selectedRequest!.subject, subject);
          expect(session.page!.effect, MutationEffect.unspecified);
          await closeRecovery(session, sequence++);
          expect(view.selectedPlan, original);
          expect(session.page, isNull);
          await session.startReconciliation(
            MutationReconcileRequest(
              submission: token(sequence++),
              packageId: plugin.id,
              packageDigest: plugin.digest,
              registryRevision: catalog.revision,
              plan: original,
              timeoutMs: 30000,
            ),
          );
          await deliver(session, () => session.reconciliation != null);
          final result = session.reconciliation!;
          expect(
            result.phase,
            disposition == MutationDisposition.create
                ? MutationPhase.observed
                : MutationPhase.prepared,
          );
          expect(
            result.effect,
            disposition == MutationDisposition.create
                ? MutationEffect.osSucceeded
                : MutationEffect.unspecified,
          );
          await deliver(session, () => session.canAcknowledge);
          await session.acknowledge();
          expect(view.lastReconciliation, same(result));
          expect(view.selectedPlan, original);
          expect(session.reconciliation, isNull);
        }
        final lossy = LoseReadReply(backend.mutationTasks);
        final recovery = MutationRecoverySession(lossy, backend);
        await recovery.refresh();
        MutationDiscoverRequest discovery(int id) => MutationDiscoverRequest(
          submission: token(id),
          packageId: plugin.id,
          packageDigest: plugin.digest,
          registryRevision: catalog.revision,
          subject: subject,
          disposition: MutationDisposition.create,
          scanLimit: 1,
          timeoutMs: 30000,
        );
        await recovery.startDiscovery(discovery(sequence++));
        await deliver(
          recovery,
          () => recovery.notice == MutationRecoveryNotice.readUnknown,
        );
        final readsAtLoss = lossy.reads;
        await recovery.refresh();
        expect(recovery.phase, MutationRecoveryPhase.resultLost);
        expect(recovery.page, isNull);
        expect(recovery.canRead, isFalse);
        expect(recovery.canNext, isFalse);
        expect(lossy.reads, readsAtLoss);
        await closeRecovery(recovery, sequence++);
        await recovery.startDiscovery(discovery(sequence++));
        await deliver(recovery, () => recovery.page != null);
        expect(
          recovery.page!.plans!.single,
          originals[MutationDisposition.create],
        );
        await closeRecovery(recovery, sequence++);
        await recovery.startDiscovery(
          MutationDiscoverRequest(
            submission: token(sequence++),
            packageId: plugin.id,
            packageDigest: token(254),
            registryRevision: catalog.revision,
            subject: subject,
            disposition: MutationDisposition.create,
            scanLimit: 1,
            timeoutMs: 30000,
          ),
        );
        await recovery.refresh();
        expect(recovery.snapshot!.key, isNull);
        expect(
          recovery.snapshot!.submission,
          isNotNull,
        ); // Failed host admission burns token.
        expect(recovery.canStart, isFalse);
        expect(recovery.canAbandon, isTrue);
        await recovery.abandon();
        expect(recovery.canStart, isTrue);
        await recovery.startDiscovery(discovery(sequence++));
        await deliver(recovery, () => recovery.page != null);
        expect(
          recovery.page!.plans!.single,
          originals[MutationDisposition.create],
        );
        await closeRecovery(recovery, sequence++);
        expect(retained.readAsBytesSync(), [4, 5, 6]);
        expect(File('${target.path}/created.bin').readAsBytesSync(), isEmpty);
      } finally {
        await backend?.close();
        await removeTestDirectory(directory);
      }
    },
    skip:
        !Platform.isWindows ||
        executable == null ||
        builtin == null ||
        fixture == null,
    timeout: const Timeout(Duration(minutes: 3)),
  );
}
