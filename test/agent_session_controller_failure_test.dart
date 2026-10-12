import 'dart:async';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/agent_session_control.dart';
import 'package:morrow_studio/plugins/agent_session_controller.dart';
import 'package:morrow_studio/plugins/agent_session_models.dart';
import 'agent_session_controller_cases.dart';

// Reuse the existing ordinary controller fixture. No process, storage, native
// Start, protected backend, account, network or recovery is created here.
AgentSessionResult rejection(
  AgentSessionOperation operation,
  AgentSessionStatus status, {
  bool unknown = false,
  bool leaseLive = true,
  List<int>? key,
}) => AgentSessionResult(
  operation: operation,
  status: status,
  taskKey: key ?? task,
  submission: BigInt.zero,
  attachment: BigInt.from(4),
  endpointGeneration: BigInt.one,
  leaseLive: leaseLive,
  deliveryUnknown: unknown,
);

void main() {
  for (final operation in [
    AgentSessionOperation.read,
    AgentSessionOperation.closeWriter,
    AgentSessionOperation.detach,
    AgentSessionOperation.status,
  ]) {
    for (final kind in [
      AgentSessionFailureKind.unavailable,
      AgentSessionFailureKind.invalid,
      AgentSessionFailureKind.busy,
    ]) {
      test(
        'entered ${operation.name} throwing ${kind.name} seals locally',
        () async {
          final x = await opened(), b = x.backend, c = x.controller;
          addTearDown(c.dispose);
          if (operation == AgentSessionOperation.closeWriter) {
            await writer(b, c);
          }
          if (operation == AgentSessionOperation.read) await c.listSessions();
          final token = c.view.submission;
          final failure = AgentSessionFailure(kind);
          final before = b.calls.length;
          b.handler = (_) async => throw failure;
          final Future<AgentSessionResult> Function() action =
              switch (operation) {
                AgentSessionOperation.read => c.read,
                AgentSessionOperation.closeWriter => c.closeWriter,
                AgentSessionOperation.detach => c.detach,
                AgentSessionOperation.status => c.status,
                _ => throw StateError('Unselected fixture operation'),
              };
          await expectLater(action(), throwsA(same(failure)));
          expect(b.calls.length, before + 1);
          expect(
            b.sessionOutcomeUnknown,
            isFalse,
          ); // Local defense is required.
          expect(c.view.outcomeUnknown, isTrue);
          expect(c.view.phase, AgentSessionPhase.unknown);
          expect(c.view.busy, isFalse);
          expect(c.view.submission, token);
          final count = b.calls.length;
          await rejects(c.listSessions, AgentSessionFailureKind.unknown);
          if (operation == AgentSessionOperation.read) {
            expect(c.canRead, isFalse);
            await rejects(c.read, AgentSessionFailureKind.unknown);
          }
          if (operation == AgentSessionOperation.closeWriter) {
            expect(
              c.canCloseWriter,
              isTrue,
            ); // Rights reduction stays available.
            await rejects(
              () => c.append([AgentSessionEvent('blocked', [])]),
              AgentSessionFailureKind.unknown,
            );
          }
          expect(
            b.calls.length,
            count,
          ); // No automatic action or uncertain retry.
        },
      );
    }
  }

  for (final status in [AgentSessionStatus.busy, AgentSessionStatus.invalid]) {
    test('bound ${status.name} receipt preserves known no-effect', () async {
      final x = await opened(), b = x.backend, c = x.controller;
      addTearDown(c.dispose);
      b.handler = (call) async => rejection(call.operation, status);
      await rejects(
        c.listSessions,
        status == AgentSessionStatus.busy
            ? AgentSessionFailureKind.busy
            : AgentSessionFailureKind.invalid,
      );
      expect(c.view.outcomeUnknown, isFalse);
      expect(c.view.phase, AgentSessionPhase.opened);
      expect(c.view.submission, isNull);
      b.handler = null;
      await c.listSessions(); // Only an explicit later call may submit.
      expect(c.pendingOperation, AgentSessionOperation.submit);
      expect(c.canRead, isTrue);
    });
  }

  test('bound rejection carrying Unknown synchronizes phase', () async {
    final x = await opened(), b = x.backend, c = x.controller;
    addTearDown(c.dispose);
    b.handler = (call) async =>
        rejection(call.operation, AgentSessionStatus.busy, unknown: true);
    await rejects(c.listSessions, AgentSessionFailureKind.busy);
    expect(c.view.outcomeUnknown, isTrue);
    expect(c.view.phase, AgentSessionPhase.unknown);
    final count = b.calls.length;
    await rejects(c.listSessions, AgentSessionFailureKind.unknown);
    expect(b.calls.length, count);
  });

  test(
    'rejection cannot clear prior Unknown or upgrade a closed lease',
    () async {
      final x = await opened(), b = x.backend, c = x.controller;
      addTearDown(c.dispose);
      b.handler = (call) async =>
          rejection(call.operation, AgentSessionStatus.busy, leaseLive: false);
      await rejects(c.listSessions, AgentSessionFailureKind.busy);
      expect(c.view.outcomeUnknown, isFalse);
      expect(c.view.leaseLive, isFalse);
      b.handler = (call) async =>
          rejection(call.operation, AgentSessionStatus.busy);
      await rejects(c.status, AgentSessionFailureKind.busy);
      expect(c.view.leaseLive, isFalse);
      final count = b.calls.length;
      await rejects(c.listSessions, AgentSessionFailureKind.unavailable);
      expect(b.calls.length, count);
      b.handler = null;
      await c.markLost();
      b.handler = (call) async =>
          rejection(call.operation, AgentSessionStatus.invalid);
      await rejects(c.status, AgentSessionFailureKind.invalid);
      expect(c.view.outcomeUnknown, isTrue);
      expect(c.view.phase, AgentSessionPhase.unknown);
    },
  );

  test('foreign busy receipt never obtains the no-effect exception', () async {
    final x = await opened(), b = x.backend, c = x.controller;
    addTearDown(c.dispose);
    b.handler = (call) async => rejection(
      call.operation,
      AgentSessionStatus.busy,
      key: List.filled(32, 8),
    );
    await rejects(c.listSessions);
    expect(c.view.outcomeUnknown, isTrue);
    expect(c.view.phase, AgentSessionPhase.unknown);
  });

  test(
    'local unsupported preflight neither exchanges nor burns outcome',
    () async {
      final b = FakeControl()..supportsAgentSessions = false;
      final c = AgentSessionController(b, task);
      addTearDown(c.dispose);
      expect(c.canRead, isFalse);
      expect(c.canCloseWriter, isFalse);
      expect(c.pendingOperation, isNull);
      await rejects(c.attach, AgentSessionFailureKind.unavailable);
      expect(b.calls, isEmpty);
      expect(c.view.outcomeUnknown, isFalse);
      expect(c.view.phase, AgentSessionPhase.detached);
    },
  );

  test(
    'Unknown permits explicit pending Read and writer reduction without revival',
    () async {
      final x = await opened(), b = x.backend, c = x.controller;
      addTearDown(c.dispose);
      await writer(b, c);
      expect(c.canCloseWriter, isTrue);
      await c.append([AgentSessionEvent('one', [])]);
      final token = c.view.submission!;
      final count = b.calls.length;
      expect(c.pendingOperation, AgentSessionOperation.submit);
      expect(c.canRead, isTrue);
      expect(c.canCloseWriter, isTrue);
      expect(b.calls.length, count); // Getters never call the backend.
      await c.markLost();
      expect(c.canRead, isTrue);
      b.handler = (call) async => result(
        call.operation,
        AgentSessionStatus.pending,
        token: token.toInt(),
      );
      await c.read();
      expect(c.canRead, isTrue); // Explicit Pending proves no consumption.
      b.handler = null;
      await c.closeWriter();
      expect(c.canCloseWriter, isFalse);
      expect(c.view.submission, token);
      expect(c.canRead, isTrue);
      await completeData(b, c, info(epoch: 1, tail: 1));
      expect(c.canRead, isFalse);
      expect(c.pendingOperation, isNull);
      expect(
        c.view.writerSessionId,
        isNull,
      ); // Late reply cannot restore grant.
      expect(c.view.outcomeUnknown, isTrue);
      expect(c.view.phase, AgentSessionPhase.unknown);
    },
  );

  test(
    'affordances refuse busy, unsupported, stale and disposed presentation',
    () async {
      final x = await opened(), b = x.backend, c = x.controller;
      await writer(b, c);
      await c.listSessions();
      final token = c.view.submission!;
      final gate = Completer<AgentSessionResult>();
      b.handler = (_) => gate.future;
      final pending = c.status();
      expect(c.canRead, isFalse);
      expect(c.canCloseWriter, isFalse);
      gate.complete(
        result(
          AgentSessionOperation.status,
          AgentSessionStatus.observed,
          token: token.toInt(),
        ),
      );
      await pending;
      expect(c.canRead, isTrue);
      expect(c.canCloseWriter, isTrue);
      b.supportsAgentSessions = false;
      expect(c.canRead, isFalse);
      expect(c.canCloseWriter, isFalse);
      b.supportsAgentSessions = true;
      final replacement = FakeControl();
      c.replaceBackend(replacement);
      expect(c.canRead, isFalse);
      expect(c.canCloseWriter, isFalse);
      expect(replacement.calls, isEmpty);
      await c.dispose();
      expect(c.canRead, isFalse);
      expect(c.canCloseWriter, isFalse);
      expect(c.view.phase, AgentSessionPhase.disposed);
    },
  );
}
