import 'dart:io';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/channel_task_models.dart';
import 'package:morrow_studio/plugins/plugin_content_task.dart';
import 'package:morrow_studio/plugins/plugin_library.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';
import 'package:morrow_studio/plugins/workbench_channel_supervised.dart';

import 'external_plugin_native_test.dart' show entireCatalog;

Future<void> closeOwner(RustWorkbench product) async {
  final transport = product.channel as SupervisedWorkbenchChannel;
  await product.close();
  expect(await product.process.exitCode, 0);
  expect(transport.supervision.value!.released, isTrue);
}

Future<PluginLibraryEntry> install(
  RustWorkbench product,
  String archive,
  List<String> approved,
) async {
  final preview = await product.inspectPlugin(archive);
  final original = preview.entries.single;
  await product.importPlugin(archive, original.digest, preview.revision);
  final catalog = await entireCatalog(product);
  await product.configureExternal(
    catalog.entries.singleWhere((e) => e.id == original.id),
    catalog.revision,
    approved,
    true,
  );
  return (await entireCatalog(product)).entries
      .singleWhere((e) => e.id == original.id);
}

void main() {
  final tools = Platform.environment;
  final root = tools['MORROW_CHANGES_PRODUCT_EVIDENCE'];
  final host = tools['MORROW_WORKBENCH_HOST'];
  final supervisor = tools['MORROW_WORKBENCH_SUPERVISOR'];
  final builtin = tools['MORROW_WORKBENCH_PACKAGE'];
  for (final language in ['rust', 'c', 'cpp']) {
    final package = tools['MORROW_CHANGES_${language.toUpperCase()}'];
    test(
      '$language reads only reviewed card changes and resumes its durable cursor after restart',
      () async {
        final folder = Directory('$root/$language');
        expect(folder.existsSync(), isFalse);
        await folder.create(recursive: true);
        Future<RustWorkbench> open() => RustWorkbench.open(
          executable: host!,
          supervisorExecutable: supervisor!,
          package: builtin!,
          directory: folder,
          managed: true,
        );
        RustWorkbench? owner;
        try {
          var product = owner = await open();
          final writer = await install(
            product,
            File('sdk/compat/guest-v1-rc1/rust-task.mplugin').absolute.path,
            PluginContentAction.values.map((e) => e.capability).toList(),
          );
          for (final card in ['card-a', 'other', 'card-b']) {
            final catalog = await entireCatalog(product);
            final receipt = await product.runPluginContent(
              writer,
              catalog.revision,
              PluginContentRequest(
                action: PluginContentAction.create,
                card: card,
                requestId: 'create-$card',
                typeId: 'text',
                title: card,
                body: [65, 66],
              ),
            );
            expect(receipt.card, card);
          }
          final changes = await install(product, package!, []);
          expect(changes.changesMetadataSupported, isTrue);
          expect(changes.approved, isEmpty);
          var submission = 0;
          Future<ChannelPrepareRequest> prepare(
            List<String> cards,
            Uint8List? cursor,
          ) async {
            final catalog = await entireCatalog(product);
            final entry = catalog.entries.singleWhere(
              (e) => e.id == changes.id,
            );
            return ChannelPrepareRequest(
              submission: List.filled(32, ++submission),
              packageId: entry.id,
              packageDigest: entry.digest,
              registryRevision: catalog.revision,
              handler: 'changes.metadata',
              kind: ChannelSourceKind.events,
              duplex: false,
              budget: entry.channelBudget!,
              compatibilityBudget: true,
              lifetimeMs: 20000,
              frameCount: 0,
              totalBytes: BigInt.zero,
              changes: ChangesWindowReview(
                cards: cards,
                pageSize: 1,
                previousCursor: cursor,
              ),
            );
          }

          Future<Uint8List> runPage(Uint8List? cursor, int count) async {
            final request = await prepare(['card-a', 'card-b'], cursor);
            final row = await product.prepareChannel(request);
            var progress = await product.runChannel(
              row.key,
              row.directory.wire,
            );
            final clock = Stopwatch()..start();
            while (!progress.workerJoined &&
                clock.elapsed < const Duration(seconds: 15)) {
              await Future<void>.delayed(const Duration(milliseconds: 12));
              progress = await product.statusChannel(row.key);
            }
            expect(
              progress.taskState,
              ChannelTaskState.success,
              reason: progress.taskError,
            );
            expect(progress.workerJoined, isTrue);
            expect(progress.lastAcked, BigInt.from(count));
            expect(progress.changesCursor.length, count == 0 ? 0 : 73);
            expect(progress.outputType, 'morrow.changes.metadata.count.v1');
            expect(progress.output, [count, 0, 0, 0, 0, 0, 0, 0]);
            expect(progress.resourceReclaimed, isTrue);
            expect(progress.producerOutcome, ChannelProducerOutcome.eof);
            var closed = await product.closeChannel(row.key);
            while (closed.phase != ChannelJobPhase.closed &&
                clock.elapsed < const Duration(seconds: 17)) {
              await Future<void>.delayed(const Duration(milliseconds: 12));
              closed = await product.statusChannel(row.key);
            }
            expect(closed.phase, ChannelJobPhase.closed);
            expect(closed.cleanupProof, ChannelCleanupProof.joined);
            expect(closed.changesCursor, progress.changesCursor);
            return progress.changesCursor;
          }

          final first = await runPage(null, 1);
          await closeOwner(product);
          owner = null;
          product = owner = await open();
          final foreign = await prepare(['other'], first);
          await expectLater(product.prepareChannel(foreign), throwsA(anything));
          for (final index in [33, 72]) {
            final unconfirmed = Uint8List.fromList(first);
            unconfirmed[index] ^= 2;
            final invalid = await prepare(['card-a', 'card-b'], unconfirmed);
            await expectLater(
              product.prepareChannel(invalid),
              throwsA(anything),
            );
          }
          final second = await runPage(first, 1);
          expect(second, isNot(first));
          await runPage(second, 0);
          final catalog = await entireCatalog(product);
          await product.configureExternal(
            catalog.entries.singleWhere((e) => e.id == changes.id),
            catalog.revision,
            [],
            false,
          );
          await expectLater(
            product.prepareChannel(await prepare(['card-a', 'card-b'], second)),
            throwsA(anything),
          );
        } finally {
          if (owner != null) await closeOwner(owner);
        }
      },
      skip:
          !Platform.isWindows ||
          [root, host, supervisor, builtin, package].contains(null),
      timeout: const Timeout(Duration(minutes: 2)),
    );
  }
}
