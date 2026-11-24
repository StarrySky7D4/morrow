import 'dart:async';
import 'dart:typed_data';

import 'package:material_ui/material_ui.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/channel_task_manager.dart';
import 'package:morrow_studio/plugins/channel_task_models.dart';
import 'package:morrow_studio/plugins/channel_task_session.dart';
import 'package:morrow_studio/plugins/plugin_library.dart';

import 'channel_task_session_test.dart'
    show Backend, budget, identity, request, snapshot, source, stopPolling;

PluginLibraryEntry entry({bool changes = false}) => PluginLibraryEntry(
  id: 'org.example.channel',
  name: 'Channel package',
  version: '1.0.0',
  digest: Uint8List.fromList(identity(99)),
  enabled: true,
  builtin: false,
  available: true,
  declared: const [],
  approved: const [],
  dependencies: const [],
  handlers: [
    PluginTransformHandler(
      name: changes ? 'changes.metadata' : 'channel.consume',
      inputType: 'morrow.channel.directory.v1',
      outputType: 'summary',
      maxInputBytes: 256,
      maxOutputBytes: 256,
    ),
  ],
  issue: '',
  channelSupported: true,
  changesMetadataSupported: changes,
  channelHandlers: [changes ? 'changes.metadata' : 'channel.consume'],
  channelKinds: [
    changes ? ChannelSourceKind.events : ChannelSourceKind.byteStream,
  ],
  channelBudget: budget(),
);

void main() {
  testWidgets(
    'review displays selected cards and cancel leaves history unopened',
    (tester) async {
      final backend = Backend();
      final session = ChannelTaskSession.forBackend(backend);
      await tester.pumpWidget(
        MaterialApp(
          home: ChannelTaskManager(
            backend: backend,
            entry: entry(changes: true),
            registryRevision: BigInt.one,
          ),
        ),
      );
      await tester.enterText(
        find.byKey(const ValueKey('channel-source-bytes')),
        'card-a\ncard-b',
      );
      final run = find.byKey(const ValueKey('channel-run'));
      await tester.tap(run);
      await tester.pumpAndSettle();
      expect(find.text('Approve card changes'), findsOneWidget);
      expect(find.textContaining('card-a\ncard-b'), findsWidgets);
      expect(backend.prepares, 0);
      await tester.tap(find.text('Cancel'));
      await tester.pumpAndSettle();
      expect(backend.prepares, 0);
      await tester.tap(run);
      await tester.pumpAndSettle();
      await tester.tap(
        find.byKey(const ValueKey('channel-approve-compatibility-budget')),
      );
      await tester.pumpAndSettle();
      expect(backend.prepares, 1);
      expect(backend.requests[1]!.changes!.cards, ['card-a', 'card-b']);
      expect(backend.requests[1]!.compatibilityBudget, isFalse);
      expect(backend.appended, isEmpty);
      await stopPolling(session, backend);
      await tester.pumpWidget(const SizedBox.shrink());
    },
  );
  testWidgets('compatibility budget needs confirmation for this source', (
    tester,
  ) async {
    final backend = Backend();
    final session = ChannelTaskSession.forBackend(backend);
    await tester.pumpWidget(
      MaterialApp(
        home: ChannelTaskManager(
          backend: backend,
          entry: entry(),
          registryRevision: BigInt.one,
        ),
      ),
    );
    await tester.enterText(
      find.byKey(const ValueKey('channel-source-bytes')),
      '00ff',
    );
    final run = find.byKey(const ValueKey('channel-run-compatibility-budget'));
    await tester.tap(run);
    await tester.pumpAndSettle();
    expect(backend.prepares, 0);
    await tester.tap(find.text('Cancel'));
    await tester.pumpAndSettle();
    expect(backend.prepares, 0);
    await tester.tap(run);
    await tester.pumpAndSettle();
    await tester.tap(
      find.byKey(const ValueKey('channel-approve-compatibility-budget')),
    );
    await tester.pumpAndSettle();
    expect(backend.prepares, 1);
    expect(backend.runs, 1);
    expect(backend.requests[1]!.compatibilityBudget, isTrue);
    expect(backend.appended.single.bytes, [0, 255]);
    await stopPolling(session, backend);
    await tester.pumpWidget(const SizedBox.shrink());
  });

  for (final fails in [false, true]) {
    testWidgets(
      'stale peer receipt ${fails ? "error" : "reply"} cannot attach to a new generation',
      (tester) async {
        final backend = Backend();
        final session = ChannelTaskSession.forBackend(backend);
        final frames = source();
        final first = request(1, frames);
        await session.prepareAndRun(first, frames);
        ChannelTaskSnapshot oldState({bool closed = false}) => snapshot(
          first,
          1,
          uploadedFrames: frames.length,
          uploadedBytes: first.totalBytes,
          observedSequence: BigInt.one,
          closed: closed,
        );
        backend.onStatus = (_) async => oldState();
        await session.refresh();
        final receipt = Completer<ChannelSentFrame?>();
        backend.onSent = (_, _) => receipt.future;
        await tester.pumpWidget(
          MaterialApp(
            home: ChannelTaskManager(
              backend: backend,
              entry: entry(),
              registryRevision: BigInt.one,
            ),
          ),
        );
        await tester.tap(find.byKey(const ValueKey('channel-read-sent')));
        await tester.pump();
        backend.onStatus = (_) async => oldState(closed: true);
        await session.refresh();
        await session.prepareAndRun(request(2, frames), frames);
        if (fails) {
          receipt.completeError(StateError('old receipt failed'));
        } else {
          receipt.complete(
            ChannelSentFrame(sequence: BigInt.one, bytes: [222]),
          );
        }
        await tester.pump();
        expect(find.textContaining('Peer receipt unconfirmed'), findsNothing);
        expect(find.textContaining('Native peer original send'), findsNothing);
        expect(session.snapshot!.key, identity(2));
        await stopPolling(session, backend);
        await tester.pumpWidget(const SizedBox.shrink());
      },
    );
  }
  for (final fails in [false, true]) {
    testWidgets(
      'adopted peer receipt ${fails ? "error" : "reply"} clears on generation change',
      (tester) async {
        final backend = Backend();
        final session = ChannelTaskSession.forBackend(backend);
        final frames = source();
        final first = request(1, frames);
        await session.prepareAndRun(first, frames);
        ChannelTaskSnapshot oldState({bool closed = false}) => snapshot(
          first,
          1,
          uploadedFrames: frames.length,
          uploadedBytes: first.totalBytes,
          observedSequence: BigInt.one,
          closed: closed,
        );
        backend.onStatus = (_) async => oldState();
        await session.refresh();
        backend.onSent = (_, _) async {
          if (fails) throw StateError('current receipt failed');
          return ChannelSentFrame(sequence: BigInt.one, bytes: [222]);
        };
        await tester.pumpWidget(
          MaterialApp(
            home: ChannelTaskManager(
              backend: backend,
              entry: entry(),
              registryRevision: BigInt.one,
            ),
          ),
        );
        await tester.tap(find.byKey(const ValueKey('channel-read-sent')));
        await tester.pump();
        expect(
          find.textContaining(
            fails ? 'Peer receipt unconfirmed' : 'Native peer original send',
          ),
          findsOneWidget,
        );
        backend.onStatus = (_) async => oldState(closed: true);
        await session.refresh();
        await session.prepareAndRun(request(2, frames), frames);
        await tester.pump();
        expect(find.textContaining('Peer receipt unconfirmed'), findsNothing);
        expect(find.textContaining('Native peer original send'), findsNothing);
        expect(session.snapshot!.key, identity(2));
        await stopPolling(session, backend);
        await tester.pumpWidget(const SizedBox.shrink());
      },
    );
  }
  testWidgets('receipt from already-reclaimed session clears on first change', (
    tester,
  ) async {
    final backend = Backend();
    final session = ChannelTaskSession.forBackend(backend);
    final frames = source();
    final first = request(1, frames);
    await session.prepareAndRun(first, frames);
    backend.onStatus = (_) async => snapshot(
      first,
      1,
      uploadedFrames: frames.length,
      uploadedBytes: first.totalBytes,
      observedSequence: BigInt.one,
      closed: true,
    );
    await session.refresh();
    expect(session.canPrepare, isTrue);
    backend.onSent = (_, _) async =>
        ChannelSentFrame(sequence: BigInt.one, bytes: [222]);
    await tester.pumpWidget(
      MaterialApp(
        home: ChannelTaskManager(
          backend: backend,
          entry: entry(),
          registryRevision: BigInt.one,
        ),
      ),
    );
    await tester.tap(find.byKey(const ValueKey('channel-read-sent')));
    await tester.pump();
    expect(find.textContaining('Native peer original send'), findsOneWidget);
    // No session notification occurs between mounting/adoption and replacement.
    await session.prepareAndRun(request(2, frames), frames);
    await tester.pump();
    expect(find.textContaining('Native peer original send'), findsNothing);
    await stopPolling(session, backend);
    await tester.pumpWidget(const SizedBox.shrink());
  });
}
