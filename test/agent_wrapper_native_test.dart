import 'dart:async';
import 'dart:typed_data';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/agent_wrapper_codec_native.dart';
import 'package:morrow_studio/plugins/agent_wrapper_control.dart';
import 'package:morrow_studio/plugins/agent_wrapper_models.dart';
import 'package:morrow_studio/plugins/agent_wrapper_native.dart';
import 'agent_wrapper_test_helpers.dart';

void main() {
  test(
    'effect then transport failure latches; reading facts cannot unlock writes',
    () async {
      var effects = 0, exchanges = 0;
      Future<T> exchange<T>(
        Uint8List frame,
        T Function(Uint8List) decode,
      ) async {
        exchanges++;
        final request = AgentWrapperCodec.decodeRequest(frame);
        if (request.mutates) {
          effects++;
          throw StateError('receipt lost after effect');
        }
        return decode(
          AgentWrapperCodec.encodeReply(
            AgentWrapperReply(
              action: request.action,
              status: AgentWrapperStatus.ok,
              id: request.id,
              requestSha256: AgentWrapperCodec.hash(frame),
              result: AgentWrapperResult(revisions: revisions()),
            ),
          ),
        );
      }

      final client = NativeAgentWrapperClient(
        exchange,
        () => true,
        nonce: List.filled(8, 1),
      );
      await expectLater(
        client.enableWrapper(review(), revisions(), true),
        throwsStateError,
      );
      expect(client.wrapperOutcomeUnknown, true);
      await client.wrapperState();
      expect(client.wrapperOutcomeUnknown, true);
      await expectLater(
        client.enableWrapper(review(), revisions(), true),
        throwsA(
          isA<AgentWrapperFailure>().having(
            (e) => e.kind,
            'kind',
            AgentWrapperFailureKind.unknown,
          ),
        ),
      );
      expect(effects, 1);
      expect(exchanges, 2);
    },
  );
  test(
    'Unknown host receipt cannot be replayed; Busy permits a later explicit action',
    () async {
      for (final status in [
        AgentWrapperStatus.unknown,
        AgentWrapperStatus.busy,
      ]) {
        var calls = 0;
        final ids = <String>[];
        Future<T> exchange<T>(
          Uint8List frame,
          T Function(Uint8List) decode,
        ) async {
          calls++;
          final request = AgentWrapperCodec.decodeRequest(frame);
          ids.add(AgentWrapperValidation.hex(request.id));
          return decode(
            AgentWrapperCodec.encodeReply(
              AgentWrapperReply(
                action: request.action,
                status: calls == 1 ? status : AgentWrapperStatus.ok,
                id: request.id,
                requestSha256: AgentWrapperCodec.hash(frame),
                result: AgentWrapperResult(revisions: revisions()),
              ),
            ),
          );
        }

        final client = NativeAgentWrapperClient(
          exchange,
          () => true,
          nonce: List.filled(8, 1),
        );
        await expectLater(
          client.selectWrapper(review(), revisions()),
          throwsA(isA<AgentWrapperFailure>()),
        );
        expect(calls, 1);
        if (status == AgentWrapperStatus.unknown) {
          await expectLater(
            client.selectWrapper(review(), revisions()),
            throwsA(isA<AgentWrapperFailure>()),
          );
          expect(calls, 1);
        } else {
          await client.selectWrapper(review(), revisions());
          expect(calls, 2);
          expect(ids.toSet().length, 2);
        }
      }
    },
  );
  test('malformed acceptance after mutation remains uncertain', () async {
    var calls = 0;
    Future<T> exchange<T>(Uint8List frame, T Function(Uint8List) decode) async {
      calls++;
      return decode(Uint8List.fromList([1, 2, 3]));
    }

    final client = NativeAgentWrapperClient(exchange, () => true);
    await expectLater(
      client.selectWrapper(review(), revisions()),
      throwsFormatException,
    );
    await expectLater(
      client.selectWrapper(review(), revisions()),
      throwsA(isA<AgentWrapperFailure>()),
    );
    expect(calls, 1);
  });
  test(
    'concurrent mutations share one slot and local subset errors never exchange',
    () async {
      var calls = 0;
      final gate = Completer<void>();
      Future<T> exchange<T>(
        Uint8List frame,
        T Function(Uint8List) decode,
      ) async {
        calls++;
        await gate.future;
        final r = AgentWrapperCodec.decodeRequest(frame);
        return decode(
          AgentWrapperCodec.encodeReply(
            AgentWrapperReply(
              action: r.action,
              status: AgentWrapperStatus.ok,
              id: r.id,
              requestSha256: AgentWrapperCodec.hash(frame),
              result: AgentWrapperResult(revisions: revisions()),
            ),
          ),
        );
      }

      final client = NativeAgentWrapperClient(exchange, () => true);
      await expectLater(
        client.approveWrapper(
          review(),
          revisions(),
          AgentWrapperApproval(
            sessionBits: 1,
            processBits: 4,
            sessions: ['main'],
            executionDomain: 'synthetic:test',
          ),
        ),
        throwsFormatException,
      );
      expect(calls, 0);
      final first = client.selectWrapper(review(), revisions());
      await expectLater(
        client.selectWrapper(review(), revisions()),
        throwsA(
          isA<AgentWrapperFailure>().having(
            (e) => e.kind,
            'kind',
            AgentWrapperFailureKind.busy,
          ),
        ),
      );
      expect(calls, 1);
      gate.complete();
      await first;
    },
  );
  test('unsupported device never sends the native profile', () async {
    var calls = 0;
    Future<T> exchange<T>(Uint8List frame, T Function(Uint8List) decode) async {
      calls++;
      throw StateError('must not send');
    }

    final client = NativeAgentWrapperClient(exchange, () => false);
    await expectLater(
      client.wrapperState(),
      throwsA(isA<AgentWrapperFailure>()),
    );
    expect(calls, 0);
  });
}
