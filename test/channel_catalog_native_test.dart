import 'dart:io';
import 'dart:typed_data';
import 'package:flutter/material.dart';
import 'package:crypto/crypto.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/plugin_library.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';
import 'package:morrow_studio/plugins/channel_task_models.dart';
import 'package:morrow_studio/plugins/channel_task_session.dart';
import 'external_plugin_native_test.dart'
    show entireCatalog, removeTestDirectory;
import 'plugin_tools_native_test.dart' show pumpHost;

void main() {
  final host = Platform.environment['MORROW_WORKBENCH_HOST'];
  final builtin = Platform.environment['MORROW_WORKBENCH_PACKAGE'];
  final package =
      Platform.environment['MORROW_SDK_CHANNEL_DIRECTORY_PACKAGE_CPP'];
  testWidgets(
    'real catalog and Workbench local channel UI run then reclaim original owners',
    (tester) async {
      expect(host, isNotNull, reason: 'Actual paired host is required');
      expect(builtin, isNotNull, reason: 'Actual built-in package is required');
      expect(
        package,
        isNotNull,
        reason: 'Actual SDK-built Directory CPP package is required',
      );
      Directory? directory;
      RustWorkbench? backend;
      PluginLibraryEntry? selected;
      try {
        await tester.runAsync(() async {
          directory = await Directory.systemTemp.createTemp(
            'morrow-channel-product-ui-014-',
          );
          backend = await RustWorkbench.open(
            executable: host!,
            package: builtin!,
            directory: directory!,
            managed: true,
          );
          final preview = await backend!.inspectPlugin(package!);
          expect(preview.entries.single.available, isTrue);
          expect(preview.entries.single.channelSupported, isTrue);
          expect(preview.entries.single.issue, isEmpty);
          await backend!.importPlugin(
            package,
            preview.entries.single.digest,
            preview.revision,
          );
          var catalog = await entireCatalog(backend!);
          final entry = catalog.entries.singleWhere((value) => !value.builtin);
          expect(entry.enabled, isFalse);
          expect(entry.available, isTrue);
          expect(entry.approved, isEmpty);
          await backend!.configureExternal(entry, catalog.revision, [], true);
          catalog = await entireCatalog(backend!);
          selected = catalog.entries.singleWhere(
            (value) => value.id == entry.id,
          );
          expect(selected!.enabled, isTrue);
          expect(selected!.channelSupported, isTrue);
          expect(backend!.supportsLocalChannels, isTrue);
          // Pure transforms cannot provide the original native channel authority.
          await expectLater(
            backend!.transformExternal(
              selected!,
              catalog.revision,
              selected!.handlers.single,
              Uint8List(1),
            ),
            throwsStateError,
          );
          expect((await entireCatalog(backend!)).revision, catalog.revision);
        });
        await tester.pumpWidget(
          MaterialApp(
            home: Scaffold(
              body: SingleChildScrollView(
                child: PluginLibrary(
                  backend: backend!,
                  onChanged: () {},
                  ink: Colors.black,
                  muted: Colors.grey,
                  line: Colors.grey,
                  radius: BorderRadius.circular(11),
                ),
              ),
            ),
          ),
        );
        final entry = selected!;
        final tile = find.byKey(ValueKey('plugin-entry-${entry.id}'));
        await pumpHost(tester, () => tile.evaluate().isNotEmpty);
        await tester.ensureVisible(tile);
        await tester.tap(tile);
        await tester.pumpAndSettle();
        final open = find.byKey(ValueKey('plugin-channel-${entry.id}'));
        expect(open, findsOneWidget);
        expect(tester.widget<OutlinedButton>(open).onPressed, isNotNull);
        await tester.ensureVisible(open);
        await tester.tap(open);
        await tester.pumpAndSettle();
        await tester.enterText(
          find.byKey(const ValueKey('channel-source-bytes')),
          '0001020304050607\nff807f00',
        );
        final session = ChannelTaskSession.forBackend(backend!);
        await tester.tap(find.byKey(const ValueKey('channel-run')));
        await pumpHost(
          tester,
          () =>
              session.snapshot?.taskState == ChannelTaskState.success ||
              session.uncertain,
        );
        expect(session.uncertain, isFalse, reason: '${session.failure}');
        final completed = session.snapshot!;
        expect(
          completed.taskState,
          ChannelTaskState.success,
          reason: completed.taskError,
        );
        expect(completed.inputSha256.length, 32);
        expect(completed.lastAcked, BigInt.two);
        expect(completed.sourceFrames, 2);
        expect(completed.sourceBytes, BigInt.from(12));
        expect(completed.output.length, 64);
        expect(completed.output.sublist(0, 4), [0x43, 0x48, 0x56, 0x31]);
        expect(completed.output[4], 0);
        final result = ByteData.sublistView(completed.output);
        // CHV1 preserves the guest's exact last protocol reply. Receive may
        // observe EOF (Closed) before the original producer JoinHandle is reaped.
        // This raw guest report cannot certify source or executor reclamation.
        final guestStatus = result.getUint32(8, Endian.little);
        final guestReclaimed = result.getUint32(12, Endian.little);
        expect(
          guestStatus,
          anyOf(
            ChannelStatus.closed.index,
            ChannelStatus.closingUnconfirmed.index,
          ),
        );
        expect(guestReclaimed, anyOf(0, 1));
        if (guestStatus == ChannelStatus.closingUnconfirmed.index) {
          expect(guestReclaimed, 0);
        }
        expect(result.getUint64(16, Endian.little), 2);
        expect(result.getUint64(24, Endian.little), 12);
        expect(
          completed.output.sublist(32),
          sha256.convert([0, 1, 2, 3, 4, 5, 6, 7, 255, 128, 127, 0]).bytes,
        );
        final key = Uint8List.fromList(completed.key);
        await tester.tap(find.byKey(const ValueKey('channel-close')));
        await pumpHost(
          tester,
          () =>
              session.snapshot?.phase == ChannelJobPhase.closed ||
              session.uncertain,
        );
        expect(session.uncertain, isFalse, reason: '${session.failure}');
        final closed = session.snapshot!;
        expect(ChannelValidation.same(closed.key, key), isTrue);
        expect(closed.resourceReclaimed, isTrue);
        expect(closed.cleanupProof, ChannelCleanupProof.joined);
        expect(closed.workerJoined, isTrue);
        expect(closed.closeRequested, isTrue);
        expect(closed.output, isEmpty);
        expect(closed.taskState, ChannelTaskState.unknown);
        expect(session.observedTaskResult!.taskState, ChannelTaskState.success);
        expect(session.observedTaskResult!.output, completed.output);
        expect(session.canPrepare, isTrue);
        expect(find.textContaining('cleanup joined'), findsOneWidget);
        expect(tester.takeException(), isNull);
        await tester.tap(find.text('Dismiss'));
        await tester.pumpAndSettle();
        await tester.runAsync(() async {
          final catalog = await entireCatalog(backend!);
          final entry = catalog.entries.singleWhere((value) => !value.builtin);
          await backend!.removeExternal(entry, catalog.revision);
          expect(
            (await entireCatalog(
              backend!,
            )).entries.where((entry) => !entry.builtin),
            isEmpty,
          );
          expect((await backend!.pluginState()).writable, isTrue);
        });
      } finally {
        await tester.pumpWidget(const SizedBox.shrink());
        await tester.runAsync(() async {
          await backend?.close();
          if (directory != null) await removeTestDirectory(directory!);
        });
      }
    },
    skip: !Platform.isWindows,
    timeout: const Timeout(Duration(minutes: 3)),
  );
}
