import 'dart:async';
import 'dart:typed_data';
import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/generated/host.capnp.dart' as host;
import 'package:morrow_studio/plugins/generated/content_api.capnp.dart'
    as content;
import 'package:morrow_studio/plugins/generated/identity.dart' as contract;
import 'package:morrow_studio/plugins/workbench_channel.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';

// A protocol peer independent of native process APIs. Fragmentation and exact
// revision checks run unchanged in the VM and actual Chrome.
final class Peer implements WorkbenchChannel {
  final frames = StreamController<List<int>>();
  final logs = StreamController<List<int>>();
  final exited = Completer<int>();
  BigInt revision = BigInt.parse('9007199254740993');
  BigInt? configured;
  int reads = 0, closes = 0;
  bool failSend = false;
  final actions = <host.Action>[];
  @override
  Stream<List<int>> get output => frames.stream;
  @override
  Stream<List<int>> get diagnostics => logs.stream;
  @override
  Future<int> get exitCode => exited.future;
  @override
  Future<void> send(Uint8List bytes) async {
    final request = MessageReader.deserialize(
      bytes,
    ).getRoot(host.requestFactory);
    actions.add(request.action!);
    if (failSend) throw StateError('send outcome unknown');
    final message = MessageBuilder();
    final reply = message.initRoot(host.responseFactory);
    reply.version = 1;
    reply.digest = Uint8List.fromList(contract.hostDigest);
    switch (request.action) {
      case host.Action.pageVersioned:
        break;
      case host.Action.readVersioned:
        reads++;
        reply.revisionBigInt = revision;
        final inner = MessageBuilder();
        final envelope = inner.initRoot(content.envelopeFactory);
        envelope.version = 1;
        envelope.digest = Uint8List.fromList(contract.content_apiDigest);
        envelope.kind = content.EnvelopeKind.record;
        envelope.id = 'card';
        envelope.operation = '';
        envelope.revisionBigInt = revision;
        envelope.sourceRevisionBigInt = revision;
        final card = envelope.initRecord();
        card.id = 'card';
        card.title = '本地';
        card.formatVersion = 1;
        card.revisionBigInt = revision;
        card.description = '';
        card.category = '灵感';
        card.stage = '待整理';
        card.hypothesis = '';
        card.conclusion = '';
        card.projectedStage = '待整理';
        reply.payload = inner.serialize();
      case host.Action.pluginState:
        reply.revisionBigInt = revision;
        reply.sha256 = Uint8List(32);
      case host.Action.pluginConfigure:
        configured = request.revisionBigInt;
        reply.revisionBigInt = request.revisionBigInt + BigInt.one;
        reply.sha256 = Uint8List(32);
      default:
        throw StateError('Unexpected action ${request.action}');
    }
    final body = message.serialize();
    final frame = Uint8List(body.length + 4);
    ByteData.sublistView(frame).setUint32(0, body.length, Endian.little);
    frame.setRange(4, frame.length, body);
    // Splits the length header as well as the body across distinct deliveries.
    for (final bounds in [(0, 2), (2, 9), (9, frame.length)]) {
      frames.add(Uint8List.fromList(frame.sublist(bounds.$1, bounds.$2)));
    }
  }

  @override
  Future<void> closeInput() async {
    if (exited.isCompleted) return;
    closes++;
    exited.complete(0);
    unawaited(logs.close());
    unawaited(frames.close());
  }
}

void main() {
  test(
    'shared controller keeps exact revisions and never publishes an older read',
    () async {
      final peer = Peer();
      final owner = await RustWorkbench.connect(peer);
      try {
        final first = await owner.versionedContent.read('card');
        expect(first.revision, BigInt.parse('9007199254740993'));
        peer.revision = (BigInt.one << 64) - BigInt.from(2);
        final second = await owner.versionedContent.read('card');
        expect(owner.knownContentRevision('card'), second.revision);
        peer.revision -= BigInt.one;
        final before = peer.reads;
        await expectLater(
          owner.versionedContent.read('card'),
          throwsStateError,
        );
        expect(peer.reads - before, 3);
        expect(owner.knownContentRevision('card'), second.revision);
        peer.revision = BigInt.parse('9007199254740993');
        final state = await owner.pluginState();
        final updated = await owner.configurePlugin(state, true);
        expect(peer.configured, state.revision);
        expect(updated.revision, state.revision + BigInt.one);
      } finally {
        await owner.close();
      }
      expect(await peer.exitCode, 0);
      expect(peer.closes, 1);
    },
  );
  test(
    'uncertain send fences the channel without automatically replaying it',
    () async {
      final peer = Peer();
      final owner = await RustWorkbench.connect(peer);
      peer.failSend = true;
      await expectLater(owner.versionedContent.read('card'), throwsStateError);
      await owner.close();
      expect(peer.actions, [
        host.Action.pageVersioned,
        host.Action.readVersioned,
      ]);
      expect(peer.closes, 1);
      await expectLater(owner.versionedContent.read('card'), throwsStateError);
      expect(peer.actions.length, 2);
    },
  );
}
