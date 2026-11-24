import 'dart:io';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/changes_page_wait.dart';
import 'package:morrow_studio/plugins/channel_task_models.dart';
import 'package:morrow_studio/plugins/channel_task_session.dart';
import 'package:morrow_studio/plugins/plugin_content_task.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';

import 'changes_cursor_product_native_test.dart' show closeOwner, install;
import 'external_plugin_native_test.dart' show entireCatalog;

void main() {
  final env = Platform.environment;
  final root = env['MORROW_WAIT_PRODUCT_EVIDENCE'];
  final host = env['MORROW_WORKBENCH_HOST'],
      supervisor = env['MORROW_WORKBENCH_SUPERVISOR'];
  final builtin = env['MORROW_WORKBENCH_PACKAGE'];
  for (final language in ['rust', 'c', 'cpp']) {
    final frozen = env['MORROW_CHANGES_${language.toUpperCase()}'];
    test(
      '$language delivers a future approved card change once and preserves its ACK after restart',
      () async {
        final library = Directory('$root/$language');
        expect(library.existsSync(), isFalse);
        await library.create(recursive: true);
        Future<RustWorkbench> open() => RustWorkbench.open(
          executable: host!,
          supervisorExecutable: supervisor!,
          package: builtin!,
          directory: library,
          managed: true,
        );
        RustWorkbench? owner;
        try {
          var product = owner = await open();
          final writer = await install(
            product,
            File('sdk/compat/guest-v1-rc1/rust-task.mplugin').absolute.path,
            PluginContentAction.values.map((a) => a.capability).toList(),
          );
          final changes = await install(product, frozen!, []);
          int serial = 0;
          Future<ChannelPrepareRequest> review({
            Uint8List? after,
            int lifetime = 20000,
          }) async {
            final catalog = await entireCatalog(product);
            final selected = catalog.entries.singleWhere(
              (p) => p.id == changes.id,
            );
            return ChannelPrepareRequest(
              submission: List.filled(32, ++serial),
              packageId: selected.id,
              packageDigest: selected.digest,
              registryRevision: catalog.revision,
              handler: 'changes.metadata',
              kind: ChannelSourceKind.events,
              duplex: false,
              budget: selected.channelBudget!,
              compatibilityBudget: true,
              lifetimeMs: lifetime,
              frameCount: 0,
              totalBytes: BigInt.zero,
              changes: ChangesWindowReview(
                cards: ['chosen'],
                pageSize: 1,
                previousCursor: after,
              ),
            );
          }

          Future<ReviewedChangesProgress> inspect(Uint8List? after) async {
            final v = await review(after: after);
            return product.inspectChangesCheckpoint(
              v.packageId,
              v.packageDigest,
              v.registryRevision,
              v.changes!,
            );
          }

          Future<void> write(String card, {int revision = 0}) async {
            final edit = revision > 0;
            final catalog = await entireCatalog(product);
            final receipt = await product.runPluginContent(
              catalog.entries.singleWhere((p) => p.id == writer.id),
              catalog.revision,
              PluginContentRequest(
                action: edit
                    ? PluginContentAction.edit
                    : PluginContentAction.create,
                card: card,
                requestId: '${edit ? 'edit' : 'create'}-$card-$revision',
                revision: edit ? BigInt.from(revision) : null,
                typeId: edit ? '' : 'text',
                title: card,
                body: [65 + revision],
              ),
            );
            expect(receipt.card, card);
            expect(receipt.revision, BigInt.from(revision + 1));
          }

          Future<Uint8List> finish(ChannelTaskSnapshot row) async {
            var current = await product.runChannel(row.key, row.directory.wire);
            final clock = Stopwatch()..start();
            while (!current.workerJoined &&
                clock.elapsed < const Duration(seconds: 15)) {
              await Future<void>.delayed(const Duration(milliseconds: 15));
              current = await product.statusChannel(row.key);
            }
            expect(
              current.taskState,
              ChannelTaskState.success,
              reason: current.taskError,
            );
            expect(current.workerJoined && current.resourceReclaimed, isTrue);
            expect(current.lastAcked, BigInt.one);
            expect(current.output, [1, 0, 0, 0, 0, 0, 0, 0]);
            expect(current.changesCursor, hasLength(73));
            var closed = await product.closeChannel(row.key);
            while (closed.phase != ChannelJobPhase.closed &&
                clock.elapsed < const Duration(seconds: 17)) {
              await Future<void>.delayed(const Duration(milliseconds: 15));
              closed = await product.statusChannel(row.key);
            }
            expect(closed.phase, ChannelJobPhase.closed);
            expect(closed.cleanupProof, ChannelCleanupProof.joined);
            return current.changesCursor;
          }

          final cancelled = await review();
          expect(
            (await product.startChangesWait(cancelled)).phase,
            ChangesWaitPhase.waiting,
          );
          await expectLater(
            product.prepareChannel(cancelled),
            throwsA(anything),
          );
          expect(
            (await product.changesWaitStatus(
              cancelled.submission,
              cancel: true,
            )).phase,
            ChangesWaitPhase.cancelled,
          );
          await expectLater(
            product.claimChangesWait(cancelled.submission),
            throwsA(anything),
          );
          final first = await review();
          expect(
            (await product.startChangesWait(first)).phase,
            ChangesWaitPhase.waiting,
          );
          await write('other');
          expect(
            (await product.changesWaitStatus(first.submission)).phase,
            ChangesWaitPhase.waiting,
          );
          expect((await inspect(null)).latest, isNull);
          await write('chosen');
          expect(
            (await product.changesWaitStatus(first.submission)).phase,
            ChangesWaitPhase.available,
          );
          expect((await inspect(null)).latest, isNull);
          final prepared = await product.claimChangesWait(first.submission);
          expect(prepared.submission, orderedEquals(first.submission));
          expect(
            (await product.changesWaitStatus(first.submission)).preparedKey,
            orderedEquals(prepared.key),
          );
          await expectLater(
            product.claimChangesWait(first.submission),
            throwsA(anything),
          );
          final cursor = await finish(prepared);
          expect((await inspect(cursor)).pending, isFalse);

          final expiring = await review(after: cursor, lifetime: 350);
          expect(
            (await product.startChangesWait(expiring)).phase,
            ChangesWaitPhase.waiting,
          );
          await Future<void>.delayed(const Duration(milliseconds: 400));
          expect(
            (await product.changesWaitStatus(expiring.submission)).phase,
            ChangesWaitPhase.expired,
          );
          await expectLater(
            product.claimChangesWait(expiring.submission),
            throwsA(anything),
          );
          final concurrent = await review(after: cursor);
          expect(
            (await product.startChangesWait(concurrent)).phase,
            ChangesWaitPhase.waiting,
          );
          await write('chosen', revision: 1);
          // A separate normally approved page advances the same durable head.
          final advanced = await finish(
            await product.prepareChannel(await review(after: cursor)),
          );
          expect(
            (await product.changesWaitStatus(concurrent.submission)).phase,
            ChangesWaitPhase.changed,
          );
          await expectLater(
            product.claimChangesWait(concurrent.submission),
            throwsA(anything),
          );
          await closeOwner(product);
          owner = null;
          product = owner = await open();
          expect(
            (await product.changesWaitStatus(first.submission)).phase,
            ChangesWaitPhase.unconfirmed,
          );
          final reopened = await inspect(advanced);
          expect(reopened.latest, orderedEquals(advanced));
          expect(reopened.pending, isFalse);
          expect(reopened.bookmark, ChangesBookmarkState.current);

          final session = ChannelTaskSession.forBackend(product);
          var deliveries = 0;
          final watcher = ChangesPageWait(product, (approved) async {
            deliveries++;
            await session.prepareAndRun(
              approved,
              const [],
              fromApprovedWait: true,
            );
          });
          try {
            await watcher.start(await review(after: advanced));
            expect(watcher.phase, ChangesWaitPhase.waiting);
            await write('chosen', revision: 2);
            final clock = Stopwatch()..start();
            while (session.snapshot?.workerJoined != true &&
                !session.uncertain &&
                clock.elapsed < const Duration(seconds: 18)) {
              await Future<void>.delayed(const Duration(milliseconds: 25));
            }
            expect(watcher.failure, isNull);
            expect(session.failure, isNull);
            final delivered = session.snapshot!;
            expect(delivered.taskState, ChannelTaskState.success);
            expect(
              delivered.workerJoined && delivered.resourceReclaimed,
              isTrue,
            );
            expect(delivered.output, [1, 0, 0, 0, 0, 0, 0, 0]);
            expect(delivered.changesCursor, hasLength(73));
            expect(delivered.lastAcked, BigInt.one);
            await watcher.check();
            expect(deliveries, 1);
            await session.close();
            while (session.snapshot?.phase != ChannelJobPhase.closed &&
                clock.elapsed < const Duration(seconds: 20)) {
              await Future<void>.delayed(const Duration(milliseconds: 25));
            }
            expect(session.snapshot?.phase, ChannelJobPhase.closed);
            expect(session.snapshot?.cleanupProof, ChannelCleanupProof.joined);
            final wait = await review(after: delivered.changesCursor);
            expect(
              (await product.startChangesWait(wait)).phase,
              ChangesWaitPhase.waiting,
            );
            final beforeRevoke = await entireCatalog(product);
            await product.configureExternal(
              beforeRevoke.entries.singleWhere((p) => p.id == changes.id),
              beforeRevoke.revision,
              [],
              false,
            );
            expect(
              (await product.changesWaitStatus(wait.submission)).phase,
              isIn([ChangesWaitPhase.changed, ChangesWaitPhase.cancelled]),
            );
            await expectLater(
              product.claimChangesWait(wait.submission),
              throwsA(anything),
            );
            final disabled = await entireCatalog(product);
            await product.configureExternal(
              disabled.entries.singleWhere((p) => p.id == changes.id),
              disabled.revision,
              [],
              true,
            );
            final afterRevoke = await inspect(delivered.changesCursor);
            expect(afterRevoke.latest, orderedEquals(delivered.changesCursor));
            expect(afterRevoke.pending, isFalse);
          } finally {
            watcher.dispose();
          }
        } finally {
          if (owner != null) await closeOwner(owner);
        }
      },
      skip:
          !Platform.isWindows ||
          [root, host, supervisor, builtin, frozen].contains(null),
      timeout: const Timeout(Duration(minutes: 2)),
    );
  }
}
