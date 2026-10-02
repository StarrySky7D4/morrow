import 'dart:async';
import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/channel_task_manager.dart';
import 'package:morrow_studio/plugins/channel_task_models.dart';
import 'package:morrow_studio/plugins/channel_task_session.dart';
import 'package:morrow_studio/plugins/plugin_library.dart';

import 'channel_task_session_test.dart'
    show Backend, budget, identity, request, snapshot, source, stopPolling;

PluginLibraryEntry entry() => PluginLibraryEntry(
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
  handlers: const [
    PluginTransformHandler(
      name: 'channel.consume',
      inputType: 'morrow.channel.directory.v1',
      outputType: 'summary',
      maxInputBytes: 256,
      maxOutputBytes: 256,
    ),
  ],
  issue: '',
  channelSupported: true,
  channelHandlers: const ['channel.consume'],
  channelKinds: const [ChannelSourceKind.byteStream],
  channelBudget: budget(),
);

void main() {
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
}
