import 'dart:async';
import 'dart:typed_data';

import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/generated/host.capnp.dart' as host;
import 'package:morrow_studio/plugins/generated/identity.dart' as contract;
import 'package:morrow_studio/plugins/workbench_channel.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';

// Holds the second write until after EOF has failed the pending response.
final class _ExitDuringSendChannel implements WorkbenchChannel {
  final _output = StreamController<List<int>>();
  final _diagnostics = StreamController<List<int>>();
  final _exit = Completer<int>();
  final secondSendStarted = Completer<void>();
  final _finishSecondSend = Completer<void>();
  final actions = <host.Action>[];
  int closeInputCalls = 0;

  @override
  Stream<List<int>> get output => _output.stream;
  @override
  Stream<List<int>> get diagnostics => _diagnostics.stream;
  @override
  Future<int> get exitCode => _exit.future;

  @override
  Future<void> send(Uint8List frame) {
    final request = MessageReader.deserialize(
      frame,
    ).getRoot(host.requestFactory);
    actions.add(request.action!);
    if (actions.length == 2) {
      secondSendStarted.complete();
      return _finishSecondSend.future;
    }
    final message = MessageBuilder();
    final response = message.initRoot(host.responseFactory);
    response.version = 1;
    response.digest = Uint8List.fromList(contract.hostDigest);
    final body = message.serialize();
    final framed = Uint8List(body.length + 4);
    ByteData.sublistView(framed).setUint32(0, body.length, Endian.little);
    framed.setRange(4, framed.length, body);
    _output.add(framed);
    return Future.value();
  }

  Future<void> endOutput() async {
    await _diagnostics.close();
    await _output.close();
  }

  void exit(int code) => _exit.complete(code);
  void finishSecondSend() => _finishSecondSend.complete();

  @override
  Future<void> closeInput() async {
    closeInputCalls++;
  }
}

void main() {
  test(
    'EOF before send completion preserves the exit error without replay',
    () async {
      final uncaught = <Object>[];
      final finished = Completer<void>();
      runZonedGuarded(() {
        unawaited(() async {
          try {
            final channel = _ExitDuringSendChannel();
            final owner = await RustWorkbench.connect(channel);
            final read = owner.versionedContent.read('card');
            final readError = expectLater(
              read,
              throwsA(
                isA<StateError>().having(
                  (error) => error.toString(),
                  'original exit error',
                  contains('内容服务已退出 (86)'),
                ),
              ),
            );
            await channel.secondSendStarted.future;
            await channel.endOutput();

            // Closing the owner cannot finish merely because stdout ended.
            final close = owner.close();
            var closeSettled = false;
            unawaited(
              close.then<void>(
                (_) => closeSettled = true,
                onError: (Object _, StackTrace _) => closeSettled = true,
              ),
            );
            await Future<void>.delayed(Duration.zero);
            expect(closeSettled, isFalse);

            channel.exit(86);
            // Give _ended/_fail a full event-loop turn while send is still held.
            await Future<void>.delayed(Duration.zero);
            await Future<void>.delayed(Duration.zero);
            channel.finishSecondSend();

            await readError;
            await expectLater(
              owner.versionedContent.read('card'),
              throwsA(isA<StateError>()),
            );
            await expectLater(
              close,
              throwsA(
                isA<StateError>().having(
                  (error) => error.toString(),
                  'joined exit',
                  contains('Content service shutdown failed (86)'),
                ),
              ),
            );
            expect(channel.actions, [
              host.Action.pageVersioned,
              host.Action.readVersioned,
            ]);
            expect(channel.closeInputCalls, 1);
            await Future<void>.delayed(Duration.zero);
            finished.complete();
          } catch (error, stack) {
            finished.completeError(error, stack);
          }
        }());
      }, (error, _) => uncaught.add(error));
      await finished.future;
      expect(uncaught, isEmpty);
    },
  );
}
