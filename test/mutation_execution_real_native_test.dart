// Real original-owner Create/Delete preparation and explicit execution.
import 'dart:io';
import 'dart:typed_data';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/mutation_execution_session.dart';
import 'package:morrow_studio/plugins/mutation_task_models.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';
import 'external_plugin_native_test.dart'
    show entireCatalog, removeTestDirectory;

Uint8List _token(int value) => Uint8List(32)..[0] = value;

Future<void> _closeTask(MutationExecutionSession session) async {
  expect(session.canRelease, isTrue);
  await session.release();
  final deadline = Stopwatch()..start();
  while (!session.canAcknowledge &&
      deadline.elapsed < const Duration(seconds: 15)) {
    await session.refresh();
    expect(session.canRepair, isFalse);
    await Future<void>.delayed(const Duration(milliseconds: 10));
  }
  expect(session.canAcknowledge, isTrue);
  await session.acknowledge();
  expect(session.canStart, isTrue);
}

void main() {
  final env = Platform.environment;
  final executable = env['MORROW_WORKBENCH_HOST'];
  final builtin = env['MORROW_WORKBENCH_PACKAGE'];
  final fixture = env['MORROW_MUTATION_TASK_PACKAGE'];
  test(
    'real owner prepares nonempty create, deletes explicitly and cancels a plan',
    () async {
      final directory = await Directory.systemTemp.createTemp(
        'morrow-external-mutation-execution-',
      );
      RustWorkbench? backend;
      try {
        backend = await RustWorkbench.open(
          executable: executable!,
          package: builtin!,
          directory: directory,
          managed: true,
        );
        final preview = await backend.inspectPlugin(fixture!);
        final candidate = preview.entries.single;
        await backend.importPlugin(fixture, candidate.digest, preview.revision);
        var catalog = await entireCatalog(backend);
        var plugin = catalog.entries.singleWhere((p) => p.id == candidate.id);
        await backend.configureExternalIo(plugin, catalog.revision, [
          'file-create',
          'file-delete',
        ]);
        catalog = await entireCatalog(backend);
        plugin = catalog.entries.singleWhere((p) => p.id == candidate.id);
        await backend.configureExternal(plugin, catalog.revision, [], true);
        catalog = await entireCatalog(backend);
        plugin = catalog.entries.singleWhere((p) => p.id == candidate.id);
        final targets = Directory('${directory.path}/targets')..createSync();
        final created = File('${targets.path}/body.bin');
        final cancelled = File('${targets.path}/cancelled.bin');
        final session = MutationExecutionSession.forBackend(
          backend.mutationTasks,
          backend,
        );
        await session.refresh();
        expect(session.canStart, isTrue);
        MutationStartRequest request(
          int token,
          MutationDisposition kind,
          String name,
        ) => MutationStartRequest(
          submission: _token(token),
          packageId: plugin.id,
          packageDigest: plugin.digest,
          registryRevision: catalog.revision,
          disposition: kind,
          selectedPath: kind == MutationDisposition.create
              ? targets.path
              : created.path,
          relativePath: kind == MutationDisposition.create ? name : '',
          subject: 'morrow.file-actions.v1',
          approvalSha256: _token(100 + token),
          timeoutMs: 30000,
        );
        final content = Uint8List.fromList(
          List.generate(
            MutationTaskValidation.maxChunkBytes * 2 + 19,
            (i) => i % 251,
          ),
        );
        await session.prepare(
          request(1, MutationDisposition.create, 'body.bin'),
          operationId: 'ui-native-create',
          content: content,
        );
        expect(session.error, isNull);
        expect(session.uncertain, isFalse);
        expect(session.ready, isTrue);
        expect(session.plan, isNotEmpty);
        expect(session.stagedBytes, BigInt.from(content.length));
        expect(created.existsSync(), isFalse);
        await session.execute();
        expect(session.error, isNull);
        expect(session.outcome!.kind, MutationResultKind.created);
        expect(session.outcome!.phase, MutationPhase.observed);
        expect(session.outcome!.effect, MutationEffect.osSucceeded);
        expect(session.canExecute, isFalse);
        expect(created.readAsBytesSync(), content);
        await session.query();
        expect(session.history!.phase, MutationPhase.observed);
        final plan = session.plan;
        await _closeTask(session);
        expect(session.plan, same(plan));
        expect(session.outcome!.effect, MutationEffect.osSucceeded);

        await session.prepare(
          request(2, MutationDisposition.delete, ''),
          operationId: 'ui-native-delete',
        );
        expect(session.error, isNull);
        expect(session.ready, isTrue);
        expect(created.existsSync(), isTrue);
        await session.execute();
        expect(session.error, isNull);
        expect(session.outcome!.kind, MutationResultKind.deleted);
        expect(session.outcome!.effect, MutationEffect.osSucceeded);
        expect(created.existsSync(), isFalse);
        await _closeTask(session);

        await session.prepare(
          request(3, MutationDisposition.create, 'cancelled.bin'),
          operationId: 'ui-native-cancel',
          content: Uint8List.fromList([1, 2, 3]),
        );
        expect(session.ready, isTrue);
        await session.cancelPlan();
        expect(
          session.lastResult!.phase,
          MutationPhase.cancelledBeforeDispatch,
        );
        expect(session.canExecute, isFalse);
        expect(cancelled.existsSync(), isFalse);
        await _closeTask(session);
        expect(session.error, isNull);
        expect(session.cleanupError, isNull);
      } finally {
        try {
          if (backend != null) await backend.close();
        } finally {
          await removeTestDirectory(directory);
        }
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
