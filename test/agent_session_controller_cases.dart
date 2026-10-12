import 'dart:async';
import 'dart:typed_data';
import 'package:morrow_studio/plugins/agent_session_control.dart';
import 'package:morrow_studio/plugins/agent_session_controller.dart';
import 'package:morrow_studio/plugins/agent_session_models.dart';

void check(bool value, String message) {
  if (!value) throw StateError(message);
}

Future<void> rejects(
  FutureOr<Object?> Function() action, [
  AgentSessionFailureKind? kind,
]) async {
  try {
    await action();
  } catch (error) {
    if (kind != null)
      check(
        error is AgentSessionFailure && error.kind == kind,
        'Wrong failure: $error',
      );
    return;
  }
  throw StateError('Expected rejection');
}

final task = Uint8List.fromList(List.filled(32, 7));
AgentSessionInfo info({String id = 'notes', int epoch = 0, int tail = 0}) =>
    AgentSessionInfo(
      sessionId: id,
      epoch: BigInt.from(epoch),
      tail: BigInt.from(tail),
      floor: BigInt.one,
      archived: false,
      revision: BigInt.one,
    );
AgentSessionResult result(
  AgentSessionOperation op,
  AgentSessionStatus status, {
  int token = 0,
  int attachment = 4,
  int generation = 1,
  bool unknown = false,
  AgentSessionData? data,
  List<int>? key,
}) => AgentSessionResult(
  operation: op,
  status: status,
  taskKey: key ?? task,
  attachment: BigInt.from(attachment),
  submission: BigInt.from(token),
  endpointGeneration: BigInt.from(generation),
  leaseLive: generation > 0,
  deliveryUnknown: unknown,
  data: data,
);

final class FakeCall {
  FakeCall(
    this.operation,
    this.attachment, {
    this.token,
    this.command,
    this.slot,
    this.sid,
  });
  final AgentSessionOperation operation;
  final BigInt attachment;
  final BigInt? token, slot;
  final String? sid;
  final AgentSessionCommand? command;
}

final class FakeControl implements AgentSessionControl {
  @override
  bool supportsAgentSessions = true;
  @override
  bool sessionOutcomeUnknown = false;
  final calls = <FakeCall>[];
  Future<AgentSessionResult> Function(FakeCall)? handler;
  var nextToken = 10;
  Future<AgentSessionResult> _call(FakeCall call) async {
    calls.add(call);
    if (handler != null) return handler!(call);
    return switch (call.operation) {
      AgentSessionOperation.attach => result(
        call.operation,
        AgentSessionStatus.attached,
        generation: 0,
      ),
      AgentSessionOperation.open ||
      AgentSessionOperation.submit ||
      AgentSessionOperation.admitWriter => result(
        call.operation,
        AgentSessionStatus.submitted,
        token: nextToken++,
      ),
      AgentSessionOperation.status => result(
        call.operation,
        AgentSessionStatus.observed,
      ),
      AgentSessionOperation.markLost => result(
        call.operation,
        AgentSessionStatus.observed,
        unknown: true,
      ),
      AgentSessionOperation.detach => result(
        call.operation,
        AgentSessionStatus.detached,
        generation: 0,
      ),
      AgentSessionOperation.closeWriter => result(
        call.operation,
        AgentSessionStatus.observed,
      ),
      AgentSessionOperation.read => throw StateError(
        'Test must provide explicit read receipt',
      ),
    };
  }

  @override
  Future<AgentSessionResult> attachSession(Uint8List key) =>
      _call(FakeCall(AgentSessionOperation.attach, BigInt.zero));
  @override
  Future<AgentSessionResult> openSession(Uint8List key, BigInt a) =>
      _call(FakeCall(AgentSessionOperation.open, a));
  @override
  Future<AgentSessionResult> submitSession(
    Uint8List key,
    BigInt a,
    BigInt generation,
    AgentSessionCommand command,
  ) => _call(FakeCall(AgentSessionOperation.submit, a, command: command));
  @override
  Future<AgentSessionResult> sessionStatus(Uint8List key, BigInt a) =>
      _call(FakeCall(AgentSessionOperation.status, a));
  @override
  Future<AgentSessionResult> readSession(
    Uint8List key,
    BigInt a,
    BigInt token,
  ) => _call(FakeCall(AgentSessionOperation.read, a, token: token));
  @override
  Future<AgentSessionResult> markSessionLost(Uint8List key, BigInt a) =>
      _call(FakeCall(AgentSessionOperation.markLost, a));
  @override
  Future<AgentSessionResult> detachSession(Uint8List key, BigInt a) =>
      _call(FakeCall(AgentSessionOperation.detach, a));
  @override
  Future<AgentSessionResult> admitSessionWriter(
    Uint8List key,
    BigInt a,
    String sid,
  ) => _call(FakeCall(AgentSessionOperation.admitWriter, a, sid: sid));
  @override
  Future<AgentSessionResult> closeSessionWriter(
    Uint8List key,
    BigInt a,
    BigInt slot,
  ) => _call(FakeCall(AgentSessionOperation.closeWriter, a, slot: slot));
}

Future<({FakeControl backend, AgentSessionController controller})>
opened() async {
  final b = FakeControl();
  final controller = AgentSessionController(b, task);
  await controller.attach();
  await controller.open();
  b.handler = (call) async => result(
    call.operation,
    AgentSessionStatus.opened,
    token: call.token!.toInt(),
  );
  await controller.read();
  b.handler = null;
  return (backend: b, controller: controller);
}

Future<void> completeData(
  FakeControl b,
  AgentSessionController c,
  AgentSessionData data,
) async {
  b.handler = (call) async => result(
    call.operation,
    AgentSessionStatus.reply,
    token: call.token!.toInt(),
    data: data,
  );
  await c.read();
  b.handler = null;
}

Future<void> writer(FakeControl b, AgentSessionController c) async {
  await c.admitWriter('notes');
  b.handler = (call) async => result(
    call.operation,
    AgentSessionStatus.writerAdmitted,
    token: call.token!.toInt(),
    data: AgentSessionWriterAdmitted('notes', BigInt.from(9)),
  );
  await c.read();
  b.handler = null;
  await c.openWriter(BigInt.zero);
  await completeData(b, c, info(epoch: 1));
}

typedef SessionCase = ({String name, Future<void> Function() run});
final List<SessionCase> sessionCases = [
  (
    name:
        'typed workflow attach/open/create/admit/openWriter/append/snapshot/revoke/detach',
    run: () async {
      final x = await opened(), b = x.backend, c = x.controller;
      await c.createSession('notes');
      await completeData(b, c, info());
      await writer(b, c);
      await c.append([
        AgentSessionEvent('event-1', [1, 2, 3]),
      ]);
      final append = b.calls.last.command as AgentSessionAppend;
      check(
        append.sessionId == 'notes' &&
            append.writerSlot == BigInt.from(9) &&
            append.epoch == BigInt.one &&
            append.expectedTail == BigInt.zero,
        'Exact writer binding',
      );
      await completeData(b, c, info(epoch: 1, tail: 1));
      await c.snapshotWriter(after: BigInt.zero);
      await completeData(
        b,
        c,
        AgentSessionSnapshot(
          info: info(epoch: 1, tail: 1),
          events: [
            AgentSessionStoredEvent(
              BigInt.one,
              AgentSessionEvent('event-1', [1, 2, 3]),
            ),
          ],
          gap: false,
        ),
      );
      await c.closeWriter();
      check(c.view.writerSessionId == null, 'Writer was not revoked locally');
      await rejects(() => c.append([AgentSessionEvent('e2', [])]));
      await c.detach();
      check(
        c.view.phase == AgentSessionPhase.detached && !c.view.leaseLive,
        'Detached view',
      );
      await c.dispose();
    },
  ),
  (
    name:
        'pending read may be explicitly read again; terminal reply consumes exactly once',
    run: () async {
      final x = await opened(), b = x.backend, c = x.controller;
      await c.listSessions();
      final token = c.view.submission;
      b.handler = (call) async => result(
        call.operation,
        AgentSessionStatus.pending,
        token: call.token!.toInt(),
      );
      await c.read();
      check(c.view.submission == token, 'Pending lost original token');
      await completeData(b, c, AgentSessionInfos([]));
      final count = b.calls.length;
      await rejects(c.read);
      check(b.calls.length == count, 'Terminal read was repeated');
      await c.dispose();
    },
  ),
  (
    name:
        'one in-flight slot rejects a concurrent explicit action without queue or retry',
    run: () async {
      final b = FakeControl();
      final controller = AgentSessionController(b, task),
          gate = Completer<AgentSessionResult>();
      b.handler = (_) => gate.future;
      final pending = controller.attach();
      check(controller.view.busy, 'Busy flag must precede await');
      await rejects(controller.attach, AgentSessionFailureKind.busy);
      check(b.calls.length == 1, 'Concurrent exchange');
      gate.complete(
        result(
          AgentSessionOperation.attach,
          AgentSessionStatus.attached,
          generation: 0,
        ),
      );
      await pending;
      await controller.dispose();
    },
  ),
  (
    name: 'slow session submission returns before read and never polls itself',
    run: () async {
      final x = await opened(), b = x.backend, c = x.controller;
      await c.listSessions();
      final count = b.calls.length;
      await Future<void>.delayed(Duration.zero);
      check(
        !c.view.busy && c.view.submission != null && b.calls.length == count,
        'Automatic polling/blocking',
      );
      await rejects(
        () => c.createSession('other'),
        AgentSessionFailureKind.busy,
      );
      await c.dispose();
    },
  ),
  (
    name: 'effect then lost receipt latches Unknown; status cannot clear it',
    run: () async {
      final x = await opened(), b = x.backend, c = x.controller;
      var effects = 0;
      b.handler = (call) async {
        effects++;
        throw StateError('receipt lost after effect');
      };
      await rejects(c.listSessions);
      check(c.view.outcomeUnknown && effects == 1, 'Unknown not sticky');
      b.handler = (call) async =>
          result(call.operation, AgentSessionStatus.observed);
      await c.status();
      final count = b.calls.length;
      await rejects(c.listSessions, AgentSessionFailureKind.unknown);
      check(b.calls.length == count, 'Unknown replay');
      await c.dispose();
    },
  ),
  (
    name:
        'lost read cannot consume original token again even if status says pending',
    run: () async {
      final x = await opened(), b = x.backend, c = x.controller;
      await c.listSessions();
      final token = c.view.submission!;
      b.handler = (_) async => throw StateError('terminal read receipt lost');
      await rejects(c.read);
      b.handler = (call) async => result(
        call.operation,
        AgentSessionStatus.observed,
        token: token.toInt(),
      );
      await c.status();
      final count = b.calls.length;
      await rejects(c.read, AgentSessionFailureKind.unknown);
      check(
        c.view.outcomeUnknown && b.calls.length == count,
        'Lost read replay',
      );
      await c.dispose();
    },
  ),
  (
    name: 'typed Unknown failure during read also seals the controller',
    run: () async {
      final x = await opened(), b = x.backend, c = x.controller;
      await c.listSessions();
      b.handler = (_) async =>
          throw const AgentSessionFailure(AgentSessionFailureKind.unknown);
      await rejects(c.read, AgentSessionFailureKind.unknown);
      check(c.view.outcomeUnknown, 'Typed unknown ignored');
      await c.dispose();
    },
  ),
  (
    name:
        'Busy is definite no effect and permits only a later explicit submission',
    run: () async {
      final x = await opened(), b = x.backend, c = x.controller;
      b.handler = (call) async =>
          result(call.operation, AgentSessionStatus.busy);
      await rejects(c.listSessions, AgentSessionFailureKind.busy);
      check(
        !c.view.outcomeUnknown && c.view.submission == null,
        'Busy became an accepted effect',
      );
      b.handler = null;
      await c.listSessions();
      check(c.view.submission != null, 'Explicit later submission rejected');
      await c.dispose();
    },
  ),
  (
    name:
        'backend replacement discards old completion and refuses new owner adoption',
    run: () async {
      final b = FakeControl(),
          c = AgentSessionController(b, task),
          gate = Completer<AgentSessionResult>();
      b.handler = (_) => gate.future;
      final pending = c.attach();
      final rejected = rejects(
        () => pending,
        AgentSessionFailureKind.staleBackend,
      );
      final next = FakeControl();
      c.replaceBackend(next);
      gate.complete(
        result(AgentSessionOperation.attach, AgentSessionStatus.attached),
      );
      await rejected;
      check(
        c.view.phase == AgentSessionPhase.staleBackend && c.view.outcomeUnknown,
        'Old completion adopted',
      );
      await rejects(c.attach, AgentSessionFailureKind.staleBackend);
      await rejects(c.status, AgentSessionFailureKind.staleBackend);
      check(next.calls.isEmpty, 'New owner contacted automatically');
      await c.dispose();
    },
  ),
  (
    name:
        'dispose during flight discards completion without remote detach or join claim',
    run: () async {
      final b = FakeControl(),
          c = AgentSessionController(b, task),
          gate = Completer<AgentSessionResult>();
      b.handler = (_) => gate.future;
      final pending = c.attach();
      final rejected = rejects(
        () => pending,
        AgentSessionFailureKind.staleBackend,
      );
      await c.dispose();
      gate.complete(
        result(AgentSessionOperation.attach, AgentSessionStatus.attached),
      );
      await rejected;
      check(
        b.calls.length == 1 && c.view.phase == AgentSessionPhase.disposed,
        'Dispose sent native lifecycle action',
      );
    },
  ),
  (
    name: 'wrong task/attachment/operation receipts seal rather than adopt',
    run: () async {
      for (final bad in [
        result(
          AgentSessionOperation.submit,
          AgentSessionStatus.submitted,
          token: 20,
          key: List.filled(32, 8),
        ),
        result(
          AgentSessionOperation.submit,
          AgentSessionStatus.submitted,
          token: 20,
          attachment: 8,
        ),
        result(
          AgentSessionOperation.open,
          AgentSessionStatus.submitted,
          token: 20,
        ),
      ]) {
        final x = await opened();
        x.backend.handler = (_) async => bad;
        await rejects(x.controller.listSessions);
        check(
          x.controller.view.outcomeUnknown &&
              x.controller.view.submission == null,
          'Foreign receipt adopted',
        );
        await x.controller.dispose();
      }
    },
  ),
  (
    name:
        'wrong read token and wrong data kind never consume a valid completion',
    run: () async {
      for (final wrongToken in [true, false]) {
        final x = await opened();
        await x.controller.listSessions();
        x.backend.handler = (call) async => result(
          call.operation,
          AgentSessionStatus.reply,
          token: call.token!.toInt() + (wrongToken ? 1 : 0),
          data: info(),
        );
        await rejects(x.controller.read);
        check(
          x.controller.view.outcomeUnknown &&
              x.controller.view.lastData == null,
          'Malformed completion accepted',
        );
        await x.controller.dispose();
      }
    },
  ),
  (
    name: 'exact SID writer admission and append epoch/tail are checked',
    run: () async {
      final x = await opened();
      await x.controller.admitWriter('notes');
      x.backend.handler = (call) async => result(
        call.operation,
        AgentSessionStatus.writerAdmitted,
        token: call.token!.toInt(),
        data: AgentSessionWriterAdmitted('other', BigInt.one),
      );
      await rejects(x.controller.read);
      check(
        x.controller.view.writerSessionId == null &&
            x.controller.view.outcomeUnknown,
        'Cross-SID writer admitted',
      );
      await x.controller.dispose();
      final y = await opened();
      await writer(y.backend, y.controller);
      await y.controller.append([AgentSessionEvent('e1', [])]);
      await rejects(
        () => completeData(y.backend, y.controller, info(epoch: 2, tail: 1)),
      );
      check(y.controller.view.outcomeUnknown, 'Wrong epoch append accepted');
      await y.controller.dispose();
    },
  ),
  (
    name:
        'R2 definite rejection stays rejected, storage/commitUnknown remain sealed',
    run: () async {
      for (final kind in [
        AgentSessionRejectionKind.conflict,
        AgentSessionRejectionKind.storage,
        AgentSessionRejectionKind.commitUnknown,
      ]) {
        final x = await opened();
        await x.controller.createSession('notes');
        await completeData(x.backend, x.controller, AgentSessionRejected(kind));
        check(
          x.controller.view.lastData is AgentSessionRejected,
          'R2 rejection converted to success',
        );
        check(
          x.controller.view.outcomeUnknown ==
              (kind != AgentSessionRejectionKind.conflict),
          'R2 uncertainty boundary',
        );
        await x.controller.dispose();
      }
    },
  ),
  (
    name:
        'markLost/detach never clears Unknown and cannot reattach the same controller',
    run: () async {
      final x = await opened();
      await x.controller.markLost();
      await x.controller.detach();
      check(
        x.controller.view.outcomeUnknown && !x.controller.view.leaseLive,
        'Detach cleared outcome uncertainty',
      );
      final count = x.backend.calls.length;
      await rejects(x.controller.attach, AgentSessionFailureKind.unknown);
      check(x.backend.calls.length == count, 'Detached owner replay');
      await x.controller.dispose();
    },
  ),
  (
    name: 'unsupported backend and invalid local fields never exchange',
    run: () async {
      final b = FakeControl()..supportsAgentSessions = false;
      final c = AgentSessionController(b, task);
      await rejects(c.attach, AgentSessionFailureKind.unavailable);
      check(b.calls.isEmpty, 'Unsupported exchange');
      await rejects(() => AgentSessionCreate('../bad'));
      await rejects(() => AgentSessionCreate(String.fromCharCode(0xd800)));
      await rejects(() => AgentSessionEvent('id', List.filled(32769, 0)));
      await rejects(
        () => AgentSessionAppend('notes', BigInt.one, BigInt.one, BigInt.zero, [
          AgentSessionEvent('duplicate', []),
          AgentSessionEvent('duplicate', []),
        ]),
      );
      await rejects(
        () => AgentSessionSnapshotRequest(
          'notes',
          after: AgentSessionValidation.maxU64,
        ),
      );
      await c.dispose();
    },
  ),
  (
    name: 'model byte/list copies cannot be mutated after validation',
    run: () async {
      final body = [1, 2], event = AgentSessionEvent('e1', [1, 2]);
      final copied = AgentSessionEvent('e2', body);
      body[0] = 9;
      check(copied.body[0] == 1, 'Mutable input retained');
      await rejects(() => copied.body[0] = 9);
      final events = [event];
      final command = AgentSessionAppend(
        'notes',
        BigInt.one,
        BigInt.one,
        BigInt.zero,
        events,
      );
      events.clear();
      check(command.events.length == 1, 'Mutable event list retained');
      await rejects(() => command.events.clear());
    },
  ),
  (
    name:
        'real R2 empty session floor1/tail0 and first event at floor are valid',
    run: () async {
      final empty = info();
      check(
        empty.floor == BigInt.one && empty.tail == BigInt.zero,
        'R2 empty floor',
      );
      final snapshot = AgentSessionSnapshot(
        info: info(tail: 1),
        events: [
          AgentSessionStoredEvent(BigInt.one, AgentSessionEvent('e1', [])),
        ],
        gap: false,
      );
      check(
        snapshot.events.first.sequence == snapshot.info.floor,
        'Event at floor rejected',
      );
      await rejects(
        () => AgentSessionInfo(
          sessionId: 'notes',
          epoch: BigInt.zero,
          tail: BigInt.zero,
          floor: BigInt.zero,
          archived: false,
          revision: BigInt.one,
        ),
      );
    },
  ),
  (
    name:
        'incomplete snapshot and changed endpoint generation seal instead of displaying stale success',
    run: () async {
      final x = await opened();
      await x.controller.snapshotSession('notes', after: BigInt.zero);
      await rejects(
        () => completeData(
          x.backend,
          x.controller,
          AgentSessionSnapshot(info: info(tail: 1), events: [], gap: false),
        ),
      );
      check(
        x.controller.view.outcomeUnknown,
        'Missing expected event accepted',
      );
      await x.controller.dispose();
      final y = await opened();
      y.backend.handler = (call) async =>
          result(call.operation, AgentSessionStatus.observed, generation: 2);
      await rejects(y.controller.status);
      check(
        y.controller.view.generation == BigInt.one &&
            y.controller.view.outcomeUnknown,
        'Stale endpoint adopted',
      );
      await y.controller.dispose();
    },
  ),
  (
    name:
        'u64 metadata retains values above signed64 without conversion saturation',
    run: () async {
      final high = (BigInt.one << 63) + BigInt.from(37);
      final metadata = AgentSessionInfo(
        sessionId: 'high',
        epoch: high,
        tail: AgentSessionValidation.maxU64,
        floor: BigInt.one,
        archived: false,
        revision: high,
      );
      check(
        metadata.epoch == high &&
            metadata.tail == AgentSessionValidation.maxU64,
        'u64 narrowed',
      );
      final command = AgentSessionOpenWriter('high', high, high);
      check(
        command.writerSlot == high && command.expectedEpoch == high,
        'Writer u64 narrowed',
      );
      await rejects(
        () => AgentSessionValidation.u64(
          AgentSessionValidation.maxU64 + BigInt.one,
        ),
      );
    },
  ),
];
