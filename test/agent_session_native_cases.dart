import 'dart:async';
import 'dart:typed_data';
import 'package:morrow_studio/plugins/agent_session_control.dart';
import 'package:morrow_studio/plugins/agent_session_models.dart';
import 'package:morrow_studio/plugins/agent_session_native.dart';
import 'agent_session_controller_cases.dart'
    show SessionCase, task, result, check, rejects;

/// Deliberately not a binary codec. These tests exercise client lifetime only;
/// Rust-generated layout/canonical interoperability is a separate required test.
final class FakeCodec implements AgentSessionWireCodec {
  final requests = <AgentSessionWireRequest>[];
  late AgentSessionResult Function(AgentSessionWireRequest) reply;
  int? requestBytes;
  @override
  Uint8List encode(AgentSessionWireRequest r) {
    requests.add(r);
    return requestBytes == null
        ? Uint8List.fromList(r.id)
        : Uint8List(requestBytes!);
  }

  @override
  AgentSessionResult decode(AgentSessionWireRequest r, Uint8List bytes) =>
      reply(r);
}

final List<SessionCase> nativeCases = [
  (
    name:
        'native client retains exact admission SID/token/generation and consumes it once',
    run: () async {
      final codec = FakeCodec();
      Future<T> exchange<T>(
        Uint8List frame,
        T Function(Uint8List) decode,
      ) async => decode(Uint8List.fromList([1]));
      final client = NativeAgentSessionClient(
        exchange,
        () => true,
        codec,
        nonce: List.filled(24, 1),
      );
      codec.reply = (r) =>
          result(r.operation, AgentSessionStatus.attached, generation: 0);
      await client.attachSession(task);
      codec.reply = (r) => result(
        r.operation,
        AgentSessionStatus.submitted,
        token: 1,
        generation: 0,
      );
      await client.openSession(task, BigInt.from(4));
      codec.reply = (r) =>
          result(r.operation, AgentSessionStatus.opened, token: 1);
      await client.readSession(task, BigInt.from(4), BigInt.one);
      codec.reply = (r) =>
          result(r.operation, AgentSessionStatus.submitted, token: 2);
      await client.admitSessionWriter(task, BigInt.from(4), 'exact-sid');
      codec.reply = (r) {
        check(
          r.sessionId.isEmpty && r.command == null,
          'Read must not send a new SID or command',
        );
        final context = r.pendingCommand as AgentSessionAdmitWriter;
        check(
          context.sessionId == 'exact-sid' &&
              r.expectedGeneration == BigInt.one &&
              r.pendingOperation == AgentSessionOperation.admitWriter,
          'Original admission context lost',
        );
        return result(
          r.operation,
          AgentSessionStatus.writerAdmitted,
          token: 2,
          data: AgentSessionWriterAdmitted(context.sessionId, BigInt.from(9)),
        );
      };
      final admitted = await client.readSession(
        task,
        BigInt.from(4),
        BigInt.from(2),
      );
      check(
        (admitted.data as AgentSessionWriterAdmitted).sessionId == 'exact-sid',
        'SID fabricated',
      );
      final count = codec.requests.length;
      await rejects(
        () => client.readSession(task, BigInt.from(4), BigInt.from(2)),
        AgentSessionFailureKind.unknown,
      );
      check(codec.requests.length == count, 'Consumed context reused');
    },
  ),
  (
    name:
        'native client lost read burns token and a later status cannot unlock effects',
    run: () async {
      final codec = FakeCodec();
      var fail = false, exchanges = 0;
      Future<T> exchange<T>(
        Uint8List frame,
        T Function(Uint8List) decode,
      ) async {
        exchanges++;
        if (fail) throw StateError('lost read');
        return decode(Uint8List.fromList([1]));
      }

      final client = NativeAgentSessionClient(exchange, () => true, codec);
      codec.reply = (r) =>
          result(r.operation, AgentSessionStatus.submitted, token: 1);
      await client.openSession(task, BigInt.from(4));
      fail = true;
      await rejects(() => client.readSession(task, BigInt.from(4), BigInt.one));
      fail = false;
      codec.reply = (r) =>
          result(r.operation, AgentSessionStatus.observed, token: 1);
      await client.sessionStatus(task, BigInt.from(4));
      final count = exchanges;
      await rejects(() => client.readSession(task, BigInt.from(4), BigInt.one));
      await rejects(
        () => client.openSession(task, BigInt.from(4)),
        AgentSessionFailureKind.unknown,
      );
      check(
        client.sessionOutcomeUnknown && exchanges == count,
        'Unknown retry',
      );
    },
  ),
  (
    name:
        'native client preserves Pending context and allocates a new outer ID for explicit Read',
    run: () async {
      final codec = FakeCodec();
      Future<T> exchange<T>(
        Uint8List frame,
        T Function(Uint8List) decode,
      ) async => decode(Uint8List.fromList([1]));
      final client = NativeAgentSessionClient(
        exchange,
        () => true,
        codec,
        nonce: List.filled(24, 2),
      );
      codec.reply = (r) =>
          result(r.operation, AgentSessionStatus.submitted, token: 1);
      await client.openSession(task, BigInt.from(4));
      codec.reply = (r) =>
          result(r.operation, AgentSessionStatus.pending, token: 1);
      await client.readSession(task, BigInt.from(4), BigInt.one);
      codec.reply = (r) =>
          result(r.operation, AgentSessionStatus.opened, token: 1);
      await client.readSession(task, BigInt.from(4), BigInt.one);
      check(
        codec.requests.map((r) => r.id.join(',')).toSet().length == 3,
        'Outer ID reused',
      );
    },
  ),
  (
    name: 'private request remains 128KiB and reply cap is separately 384KiB',
    run: () async {
      final codec = FakeCodec()..requestBytes = 128 * 1024 + 1;
      var exchanges = 0, replyBytes = 1;
      Future<T> exchange<T>(
        Uint8List frame,
        T Function(Uint8List) decode,
      ) async {
        exchanges++;
        return decode(Uint8List(replyBytes));
      }

      final client = NativeAgentSessionClient(exchange, () => true, codec);
      await rejects(() => client.attachSession(task));
      check(
        exchanges == 0 && !client.sessionOutcomeUnknown,
        'Oversize request exchanged',
      );
      codec.requestBytes = 128 * 1024;
      codec.reply = (r) =>
          result(r.operation, AgentSessionStatus.attached, generation: 0);
      replyBytes = 384 * 1024;
      await client.attachSession(task);
      replyBytes++;
      await rejects(() => client.sessionStatus(task, BigInt.from(4)));
      check(client.sessionOutcomeUnknown, 'Oversize reply did not seal');
    },
  ),
  (
    name:
        'more than 64 observations still allow terminal read/revoke/detach without ID rotation',
    run: () async {
      final codec = FakeCodec();
      Future<T> exchange<T>(
        Uint8List frame,
        T Function(Uint8List) decode,
      ) async => decode(Uint8List.fromList([1]));
      final client = NativeAgentSessionClient(
        exchange,
        () => true,
        codec,
        nonce: List.filled(24, 0),
      );
      codec.reply = (r) =>
          result(r.operation, AgentSessionStatus.submitted, token: 1);
      await client.openSession(task, BigInt.from(4));
      codec.reply = (r) => result(
        r.operation,
        r.operation == AgentSessionOperation.read
            ? AgentSessionStatus.pending
            : AgentSessionStatus.observed,
        token: 1,
      );
      for (var i = 0; i < 70; i++) {
        await client.sessionStatus(task, BigInt.from(4));
        await client.readSession(task, BigInt.from(4), BigInt.one);
      }
      codec.reply = (r) =>
          result(r.operation, AgentSessionStatus.opened, token: 1);
      await client.readSession(task, BigInt.from(4), BigInt.one);
      codec.reply = (r) => result(r.operation, AgentSessionStatus.observed);
      await client.closeSessionWriter(task, BigInt.from(4), BigInt.one);
      codec.reply = (r) => result(r.operation, AgentSessionStatus.detached);
      await client.detachSession(task, BigInt.from(4));
      check(
        codec.requests.length == 144 &&
            codec.requests.map((r) => r.id.join(',')).toSet().length == 144 &&
            codec.requests.every((r) => r.id.any((b) => b != 0)),
        'Observation exhausted drain IDs/reset',
      );
    },
  ),
  (
    name:
        'unsupported native adapter never encodes or exchanges private frames',
    run: () async {
      final codec = FakeCodec();
      Future<T> exchange<T>(
        Uint8List frame,
        T Function(Uint8List) decode,
      ) async => throw StateError('Must not exchange');
      final client = NativeAgentSessionClient(exchange, () => false, codec);
      await rejects(
        () => client.attachSession(task),
        AgentSessionFailureKind.unavailable,
      );
      check(codec.requests.isEmpty, 'Unsupported profile encoded');
    },
  ),
];
