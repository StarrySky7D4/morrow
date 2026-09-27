// Real SDK Wasm -> private guest wire -> original selected host owner.
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

Uint8List _token(int value) => Uint8List(32)..[0] = value;

Uint8List _body() {
  final target = 3 * GuestMutationValidation.maxChunkBytes + 37;
  final result = BytesBuilder(copy: false);
  for (var counter = 0; result.length < target; counter++) {
    result.add(
      sha256.convert(<int>[
        ...'morrow.guest.native.four-chunks.v1'.codeUnits,
        ...List<int>.generate(8, (i) => (counter >> (8 * i)) & 255),
      ]).bytes,
    );
  }
  return Uint8List.sublistView(result.toBytes(), 0, target);
}

Future<void> _closeTask(GuestMutationExecutionSession session) async {
  expect(session.canRelease, isTrue);
  await session.release();
  final clock = Stopwatch()..start();
  while (!session.canAcknowledge &&
      clock.elapsed < const Duration(seconds: 25)) {
    await session.refresh();
    expect(session.canRepair, isFalse);
    await Future<void>.delayed(const Duration(milliseconds: 10));
  }
  expect(session.canAcknowledge, isTrue, reason: 'Release must stop worker');
  await session.acknowledge();
  expect(session.canStart, isTrue);
}

void main() {
  final env = Platform.environment;
  final host = env['MORROW_WORKBENCH_HOST'];
  final builtin = env['MORROW_WORKBENCH_PACKAGE'];
  final fixture = env['MORROW_GUEST_MUTATION_PACKAGE'];
  final language = env['MORROW_GUEST_MUTATION_LANGUAGE'];
  final moduleHash = env['MORROW_GUEST_MUTATION_WASM_SHA256'];
  test(
    'real guest SDK stages four chunks and requires two exact reviews for Create/Delete',
    () async {
      expect(language, anyOf('rust', 'c', 'cpp'));
      expect(moduleHash, matches(RegExp(r'^[0-9a-f]{64}$')));
      for (final path in [host!, builtin!, fixture!]) {
        expect(File(path).existsSync(), isTrue, reason: path);
      }
      final packageHash = sha256
          .convert(await File(fixture).readAsBytes())
          .toString();
      final directory = await Directory.systemTemp.createTemp(
        'morrow-external-real-guest-mutation-$language-',
      );
      RustWorkbench? backend;
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
        expect(
          candidate.mutationBudget?.maxJobBytes,
          BigInt.from(32 * 1024 * 1024),
        );
        expect(
          candidate.mutationBudget?.maxBytes,
          BigInt.from(256 * 1024 * 1024),
        );
        await backend.importPlugin(fixture, candidate.digest, preview.revision);
        var catalog = await entireCatalog(backend);
        var plugin = catalog.entries.singleWhere((p) => p.id == candidate.id);
        expect(plugin.mutationSupported, isTrue);
        await backend.configureExternalIo(plugin, catalog.revision, [
          'file-create',
          'file-delete',
        ]);
        catalog = await entireCatalog(backend);
        plugin = catalog.entries.singleWhere((p) => p.id == candidate.id);
        await backend.configureExternal(plugin, catalog.revision, [], true);
        catalog = await entireCatalog(backend);
        plugin = catalog.entries.singleWhere((p) => p.id == candidate.id);
        expect(plugin.enabled, isTrue);
        expect(plugin.approvedIo, containsAll(['file-create', 'file-delete']));
        final declaration = plugin.mutationBudget!;
        final approved = ApprovedGuestBudget(
          maxJobBytes: declaration.maxJobBytes,
          maxBytes: declaration.maxBytes,
        );
        final targets = Directory(
          '${directory.path}${Platform.pathSeparator}targets',
        )..createSync();
        final created = File(
          '${targets.path}${Platform.pathSeparator}body.bin',
        );
        final cancelled = File(
          '${targets.path}${Platform.pathSeparator}cancelled.bin',
        );
        final session = GuestMutationExecutionSession.forBackend(
          backend.guestMutationTasks,
          backend,
        );
        await session.refresh();
        expect(session.canStart, isTrue);
        GuestMutationStartRequest request(
          int token,
          MutationDisposition disposition,
          String name,
        ) => GuestMutationStartRequest(
          selection: MutationStartRequest(
            submission: _token(token),
            packageId: plugin.id,
            packageDigest: plugin.digest,
            registryRevision: catalog.revision,
            disposition: disposition,
            selectedPath: disposition == MutationDisposition.create
                ? targets.path
                : created.path,
            relativePath: disposition == MutationDisposition.create ? name : '',
            subject: 'morrow.guest.file-actions.v1',
            approvalSha256: _token(100 + token),
            timeoutMs: 30000,
          ),
          approvedBudget: approved,
        );

        final content = _body();
        await session.review(
          request(1, MutationDisposition.create, 'body.bin'),
          operationId: 'real-guest-create-$language',
          content: content,
        );
        expect(session.error, isNull);
        expect(session.canPrepare, isTrue);
        expect(session.canExecute, isFalse);
        expect(session.plan, isNotEmpty);
        expect(created.existsSync(), isFalse, reason: 'review is read-only');
        final createHash = session.planSha256!;
        await expectLater(
          session.prepare(reviewedPlanSha256: _token(201)),
          throwsStateError,
        );
        expect(session.canPrepare, isTrue);
        expect(created.existsSync(), isFalse);
        await session.prepare(reviewedPlanSha256: createHash);
        expect(session.error, isNull);
        expect(session.canExecute, isTrue);
        expect(session.stagedBytes, BigInt.from(content.length));
        expect(session.state?.durableContent, isTrue);
        expect(
          created.existsSync(),
          isFalse,
          reason: 'Prepare has no OS effect',
        );
        await expectLater(
          session.execute(reviewedPlanSha256: _token(202)),
          throwsStateError,
        );
        expect(created.existsSync(), isFalse);
        await session.execute(reviewedPlanSha256: createHash);
        expect(session.error, isNull);
        expect(session.outcome?.phase, GuestMutationFramePhase.observed);
        expect(session.outcome?.effect, GuestMutationFrameEffect.osSucceeded);
        expect(session.canExecute, isFalse);
        expect(created.readAsBytesSync(), content);
        await session.query();
        expect(session.history?.frame?.phase, GuestMutationFramePhase.observed);
        expect(
          session.history?.frame?.effect,
          GuestMutationFrameEffect.osSucceeded,
        );
        await _closeTask(session);

        await session.review(
          request(2, MutationDisposition.delete, ''),
          operationId: 'real-guest-delete-$language',
        );
        expect(session.canPrepare, isTrue);
        expect(created.existsSync(), isTrue);
        final deleteHash = session.planSha256!;
        await session.prepare(reviewedPlanSha256: deleteHash);
        expect(session.canExecute, isTrue);
        expect(
          created.existsSync(),
          isTrue,
          reason: 'Delete Prepare has no effect',
        );
        await session.execute(reviewedPlanSha256: deleteHash);
        expect(session.outcome?.effect, GuestMutationFrameEffect.osSucceeded);
        expect(created.existsSync(), isFalse);
        await session.query();
        expect(session.history?.frame?.phase, GuestMutationFramePhase.observed);
        await _closeTask(session);

        await session.review(
          request(3, MutationDisposition.create, 'cancelled.bin'),
          operationId: 'real-guest-cancel-$language',
          content: Uint8List.fromList([1, 2, 3]),
        );
        final cancelHash = session.planSha256!;
        await session.prepare(reviewedPlanSha256: cancelHash);
        expect(session.canExecute, isTrue);
        expect(cancelled.existsSync(), isFalse);
        await session.cancelPlan();
        expect(
          session.lastResult?.frame?.phase,
          GuestMutationFramePhase.cancelledBeforeDispatch,
        );
        expect(session.canExecute, isFalse);
        expect(cancelled.existsSync(), isFalse);
        await _closeTask(session);
        expect(session.error, isNull);
        expect(session.cleanupError, isNull);
        // ignore: avoid_print
        print('GUEST_PACKAGE_SHA256=$packageHash');
        // ignore: avoid_print
        print('GUEST_NATIVE_PASS=$language');
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
        host == null ||
        builtin == null ||
        fixture == null ||
        language == null ||
        moduleHash == null,
    timeout: const Timeout(Duration(minutes: 4)),
  );
}
