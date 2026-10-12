import 'dart:async';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/agent_session_control.dart';
import 'package:morrow_studio/plugins/agent_session_controller.dart';
import 'package:morrow_studio/plugins/agent_session_models.dart';
import 'package:morrow_studio/plugins/agent_session_manager.dart';
import 'package:morrow_studio/plugins/agent_session_workspace.dart';
import 'package:morrow_studio/plugins/agent_task_discovery.dart';
import 'agent_session_controller_cases.dart' as fixture;

final class _Discovery implements AgentTaskDiscovery {
  @override
  bool supportsAgentTaskDiscovery = true;
  int calls = 0;
  AgentTaskObservation observation = AgentTaskObservation(
    AgentTaskKind.runningAgent,
    fixture.task,
  );
  Completer<AgentTaskObservation>? held;
  @override
  Future<AgentTaskObservation> observeAgentTask() async {
    calls++;
    return held == null ? observation : held!.future;
  }
}

typedef _Rig = ({
  fixture.FakeControl backend,
  _Discovery discovery,
  AgentSessionWorkspace workspace,
});
_Rig _rig() {
  final backend = fixture.FakeControl(), discovery = _Discovery();
  return (
    backend: backend,
    discovery: discovery,
    workspace: AgentSessionWorkspace(control: backend, discovery: discovery),
  );
}

Future<void> _completion(
  _Rig rig,
  AgentSessionStatus status, [
  AgentSessionData? data,
]) async {
  rig.backend.handler = (call) async => fixture.result(
    call.operation,
    status,
    token: call.token!.toInt(),
    data: data,
  );
  await rig.workspace.read();
  rig.backend.handler = null;
}

Future<_Rig> _opened() async {
  final rig = _rig();
  await rig.workspace.observe();
  await rig.workspace.attach();
  await rig.workspace.open();
  await _completion(rig, AgentSessionStatus.opened);
  return rig;
}

Future<void> _writer(_Rig rig) async {
  rig.workspace.setDraft(AgentSessionDraft.sessionId, 'notes');
  await rig.workspace.createSession();
  await _completion(rig, AgentSessionStatus.reply, fixture.info());
  await rig.workspace.admitWriter();
  await _completion(
    rig,
    AgentSessionStatus.writerAdmitted,
    AgentSessionWriterAdmitted('notes', BigInt.from(9)),
  );
  await rig.workspace.openWriter();
  await _completion(rig, AgentSessionStatus.reply, fixture.info(epoch: 1));
}

Widget _panel(AgentSessionWorkspace workspace) => MaterialApp(
  home: Scaffold(
    body: SingleChildScrollView(
      child: AgentSessionManager(workspace: workspace),
    ),
  ),
);
Future<void> _tap(WidgetTester tester, String key) async {
  final finder = find.byKey(ValueKey('agent-session-$key'));
  await tester.ensureVisible(finder);
  await tester.tap(finder);
  await tester.pumpAndSettle();
}

bool _enabled(WidgetTester tester, String key) =>
    tester
        .widget<OutlinedButton>(find.byKey(ValueKey('agent-session-$key')))
        .onPressed !=
    null;
Future<void> _reject(FutureOr<Object?> Function() action) =>
    expectLater(Future<Object?>.sync(action), throwsA(anything));

void main() {
  test('observation is explicit and retained exit never attaches', () async {
    final rig = _rig();
    expect(rig.discovery.calls, 0);
    expect(rig.backend.calls, isEmpty);
    expect(rig.workspace.view.canAttach, isFalse);
    rig.discovery.observation = AgentTaskObservation(
      AgentTaskKind.retainedAgentExit,
      fixture.task,
    );
    await rig.workspace.observe();
    expect(rig.workspace.view.canAttach, isFalse);
    await _reject(rig.workspace.attach);
    expect(rig.backend.calls, isEmpty);
    rig.workspace.dispose();
  });

  test('discovery None supplies no key or permission', () async {
    final rig = _rig();
    rig.discovery.observation = AgentTaskObservation(AgentTaskKind.none, null);
    await rig.workspace.observe();
    expect(rig.workspace.view.observation!.taskKey, isNull);
    expect(rig.workspace.view.canAttach, isFalse);
    expect(rig.backend.calls, isEmpty);
    rig.workspace.dispose();
  });

  test(
    'pending open is single-flight; Pending Read permits only another explicit Read',
    () async {
      final rig = _rig();
      await rig.workspace.observe();
      await rig.workspace.attach();
      await rig.workspace.open();
      final token = rig.workspace.view.session!.submission;
      expect(rig.workspace.view.canSubmit, isFalse);
      await _reject(rig.workspace.open);
      await _reject(rig.workspace.listSessions);
      await _completion(rig, AgentSessionStatus.pending);
      expect(rig.workspace.view.session!.submission, token);
      expect(rig.workspace.view.canRead, isTrue);
      expect(
        rig.backend.calls
            .where((c) => c.operation == AgentSessionOperation.read)
            .length,
        1,
      );
      await _completion(rig, AgentSessionStatus.opened);
      expect(rig.workspace.view.canSubmit, isTrue);
      expect(rig.workspace.view.canRead, isFalse);
      rig.workspace.dispose();
    },
  );

  test(
    'writer workflow binds actual SID slot epoch and tail; one explicit append event',
    () async {
      final rig = await _opened();
      await _writer(rig);
      final opened =
          rig.backend.calls
                  .lastWhere((c) => c.command is AgentSessionOpenWriter)
                  .command
              as AgentSessionOpenWriter;
      expect(opened.sessionId, 'notes');
      expect(opened.writerSlot, BigInt.from(9));
      expect(opened.expectedEpoch, BigInt.zero);
      rig.workspace.setDraft(AgentSessionDraft.eventId, 'event-1');
      rig.workspace.setDraft(AgentSessionDraft.eventBody, 'hello');
      await rig.workspace.append();
      final appended = rig.backend.calls.last.command as AgentSessionAppend;
      expect(appended.sessionId, 'notes');
      expect(appended.writerSlot, BigInt.from(9));
      expect(appended.epoch, BigInt.one);
      expect(appended.expectedTail, BigInt.zero);
      expect(appended.events.single.id, 'event-1');
      expect(appended.events.single.body, [104, 101, 108, 108, 111]);
      await _completion(
        rig,
        AgentSessionStatus.reply,
        fixture.info(epoch: 1, tail: 1),
      );
      await rig.workspace.snapshotWriter();
      await _completion(
        rig,
        AgentSessionStatus.reply,
        AgentSessionSnapshot(
          info: fixture.info(epoch: 1, tail: 1),
          gap: false,
          events: [
            AgentSessionStoredEvent(
              BigInt.one,
              AgentSessionEvent('event-1', [104, 101, 108, 108, 111]),
            ),
          ],
        ),
      );
      expect(rig.workspace.view.snapshot!.events.single.sequence, BigInt.one);
      await rig.workspace.closeWriter();
      expect(rig.workspace.view.canAppend, isFalse);
      await rig.workspace.detach();
      expect(rig.workspace.view.canAttach, isFalse);
      expect(rig.workspace.view.drafts[AgentSessionDraft.eventId], 'event-1');
      rig.workspace.dispose();
    },
  );

  test(
    'observed writer epoch change disables Append without adopting a new epoch',
    () async {
      final rig = await _opened();
      await _writer(rig);
      await rig.workspace.snapshotWriter();
      await _completion(
        rig,
        AgentSessionStatus.reply,
        AgentSessionSnapshot(
          info: fixture.info(epoch: 2),
          gap: false,
          events: [],
        ),
      );
      expect(rig.workspace.view.canAppend, isFalse);
      final before = rig.backend.calls.length;
      await _reject(rig.workspace.append);
      expect(rig.backend.calls.length, before);
      expect(rig.workspace.view.canCloseWriter, isTrue);
      rig.workspace.dispose();
    },
  );

  test(
    'lost Read preserves submission and draft, burns Read, permits rights reduction',
    () async {
      final rig = await _opened();
      await _writer(rig);
      rig.workspace.setDraft(AgentSessionDraft.eventId, 'uncertain-event');
      rig.workspace.setDraft(AgentSessionDraft.eventBody, 'keep me');
      await rig.workspace.append();
      final token = rig.workspace.view.session!.submission;
      rig.backend.handler = (_) async =>
          throw const AgentSessionFailure(AgentSessionFailureKind.unavailable);
      await _reject(rig.workspace.read);
      rig.backend.handler = null;
      expect(rig.workspace.view.session!.outcomeUnknown, isTrue);
      expect(rig.workspace.view.session!.submission, token);
      expect(rig.workspace.view.canRead, isFalse);
      expect(rig.workspace.view.canSubmit, isFalse);
      final before = rig.backend.calls.length;
      await _reject(rig.workspace.read);
      await _reject(rig.workspace.append);
      expect(rig.backend.calls.length, before);
      await rig.workspace.closeWriter();
      await rig.workspace.detach();
      expect(rig.workspace.view.session!.outcomeUnknown, isTrue);
      expect(rig.workspace.view.drafts[AgentSessionDraft.eventBody], 'keep me');
      expect(rig.workspace.view.canAttach, isFalse);
      rig.workspace.dispose();
    },
  );

  test(
    'MarkLost allows first known-token drain but cannot clear Unknown or submit again',
    () async {
      final rig = await _opened();
      await rig.workspace.listSessions();
      await rig.workspace.markLost();
      expect(rig.workspace.view.canRead, isTrue);
      await _completion(rig, AgentSessionStatus.reply, AgentSessionInfos([]));
      expect(rig.workspace.view.session!.outcomeUnknown, isTrue);
      expect(rig.workspace.view.canSubmit, isFalse);
      await _reject(rig.workspace.listSessions);
      rig.workspace.dispose();
    },
  );

  test(
    'invalid identity counters and event body fail before another backend call',
    () async {
      final rig = await _opened();
      var before = rig.backend.calls.length;
      rig.workspace.setDraft(AgentSessionDraft.sessionId, '../unsafe');
      await _reject(rig.workspace.createSession);
      expect(rig.backend.calls.length, before);
      rig.workspace.setDraft(AgentSessionDraft.sessionId, 'notes');
      rig.workspace.setDraft(AgentSessionDraft.after, '18446744073709551616');
      await _reject(rig.workspace.snapshotSession);
      expect(rig.backend.calls.length, before);
      rig.workspace.setDraft(AgentSessionDraft.after, '0');
      await _writer(rig);
      before = rig.backend.calls.length;
      rig.workspace.setDraft(AgentSessionDraft.eventId, 'event-1');
      rig.workspace.setDraft(
        AgentSessionDraft.eventBody,
        List.filled(32769, 'x').join(),
      );
      await _reject(rig.workspace.append);
      expect(rig.backend.calls.length, before);
      rig.workspace.dispose();
    },
  );

  test(
    'a changed observation cannot replace the retained controller or exact task',
    () async {
      final rig = await _opened();
      rig.discovery.observation = AgentTaskObservation(
        AgentTaskKind.runningAgent,
        List.filled(32, 9),
      );
      await rig.workspace.observe();
      expect(rig.workspace.view.canAttach, isFalse);
      expect(rig.workspace.view.session!.phase, AgentSessionPhase.opened);
      await rig.workspace.listSessions();
      // The fake returns the original task; the retained controller must accept
      // that exact binding, despite the later observation of a different task.
      await _completion(rig, AgentSessionStatus.reply, AgentSessionInfos([]));
      expect(rig.workspace.view.session!.outcomeUnknown, isFalse);
      rig.workspace.dispose();
    },
  );

  test(
    'owner disposal seals synchronously and discards a late discovery result',
    () async {
      final rig = _rig();
      rig.discovery.held = Completer<AgentTaskObservation>();
      final pending = rig.workspace.observe();
      rig.workspace.dispose();
      expect(rig.workspace.closed, isTrue);
      expect(rig.workspace.view.canAttach, isFalse);
      rig.discovery.held!.complete(
        AgentTaskObservation(AgentTaskKind.runningAgent, fixture.task),
      );
      await expectLater(pending, throwsA(isA<AgentSessionFailure>()));
      expect(rig.workspace.view.observation, isNull);
      expect(rig.backend.calls, isEmpty);
    },
  );

  test('owner disposal never auto Detach CloseWriter or MarkLost', () async {
    final rig = await _opened();
    await _writer(rig);
    final count = rig.backend.calls.length;
    rig.workspace.dispose();
    expect(rig.workspace.view.canRead, isFalse);
    await Future<void>.delayed(Duration.zero);
    expect(rig.backend.calls.length, count);
    await _reject(rig.workspace.detach);
  });

  test(
    'owner disposal discards a late Attach without adopting or cleaning the host owner',
    () async {
      final rig = _rig();
      await rig.workspace.observe();
      final held = Completer<AgentSessionResult>();
      rig.backend.handler = (_) => held.future;
      final attach = rig.workspace.attach();
      rig.workspace.dispose();
      held.complete(
        fixture.result(
          AgentSessionOperation.attach,
          AgentSessionStatus.attached,
          generation: 0,
        ),
      );
      await expectLater(attach, throwsA(isA<AgentSessionFailure>()));
      expect(rig.workspace.closed, isTrue);
      expect(rig.workspace.view.canAttach, isFalse);
      expect(rig.backend.calls.map((c) => c.operation), [
        AgentSessionOperation.attach,
      ]);
    },
  );

  testWidgets(
    'Append and writer Snapshot buttons issue typed commands and display the actual completion',
    (tester) async {
      final rig = await _opened();
      await _writer(rig);
      await tester.pumpWidget(_panel(rig.workspace));
      await tester.pumpAndSettle();
      final id = find.byKey(const ValueKey('agent-session-draft-eventId'));
      final body = find.byKey(const ValueKey('agent-session-draft-eventBody'));
      await tester.ensureVisible(id);
      await tester.enterText(id, 'event-1');
      await tester.ensureVisible(body);
      await tester.enterText(body, 'hello');
      await _tap(tester, 'append');
      expect(rig.backend.calls.last.command, isA<AgentSessionAppend>());
      expect(_enabled(tester, 'append'), isFalse);
      rig.backend.handler = (call) async => fixture.result(
        call.operation,
        AgentSessionStatus.reply,
        token: call.token!.toInt(),
        data: fixture.info(epoch: 1, tail: 1),
      );
      await _tap(tester, 'read');
      rig.backend.handler = null;
      await _tap(tester, 'snapshotWriter');
      final command =
          rig.backend.calls.last.command as AgentSessionSnapshotRequest;
      expect(command.writerSlot, BigInt.from(9));
      expect(command.sessionId, 'notes');
      rig.backend.handler = (call) async => fixture.result(
        call.operation,
        AgentSessionStatus.reply,
        token: call.token!.toInt(),
        data: AgentSessionSnapshot(
          info: fixture.info(epoch: 1, tail: 1),
          events: [
            AgentSessionStoredEvent(
              BigInt.one,
              AgentSessionEvent('event-1', [104, 101, 108, 108, 111]),
            ),
          ],
          gap: false,
        ),
      );
      await _tap(tester, 'read');
      rig.backend.handler = null;
      expect(find.text('1 · event-1\nhello'), findsOneWidget);
      await _tap(tester, 'closeWriter');
      expect(rig.workspace.view.canCloseWriter, isFalse);
      await _tap(tester, 'detach');
      expect(rig.workspace.view.canAttach, isFalse);
      await tester.pumpWidget(const SizedBox());
      rig.workspace.dispose();
    },
  );

  testWidgets(
    'mount and remount never observe or attach; explicit discovery and Attach remain separate',
    (tester) async {
      final rig = _rig();
      await tester.pumpWidget(_panel(rig.workspace));
      await tester.pumpAndSettle();
      expect(rig.discovery.calls, 0);
      expect(rig.backend.calls, isEmpty);
      expect(_enabled(tester, 'attach'), isFalse);
      await _tap(tester, 'observe');
      expect(rig.discovery.calls, 1);
      expect(rig.backend.calls, isEmpty);
      expect(_enabled(tester, 'attach'), isTrue);
      await _tap(tester, 'attach');
      expect(rig.backend.calls.map((c) => c.operation), [
        AgentSessionOperation.attach,
      ]);
      await tester.pumpWidget(const SizedBox());
      await tester.pumpWidget(_panel(rig.workspace));
      await tester.pumpAndSettle();
      expect(rig.discovery.calls, 1);
      expect(rig.backend.calls.length, 1);
      expect(_enabled(tester, 'open'), isTrue);
      await tester.pumpWidget(const SizedBox());
      rig.workspace.dispose();
    },
  );

  testWidgets(
    'pending and draft survive route removal; Read is never automatic',
    (tester) async {
      final rig = await _opened();
      await tester.pumpWidget(_panel(rig.workspace));
      await tester.pumpAndSettle();
      final input = find.byKey(const ValueKey('agent-session-draft-sessionId'));
      await tester.ensureVisible(input);
      await tester.enterText(input, 'notes');
      await _tap(tester, 'createSession');
      final token = rig.workspace.view.session!.submission,
          count = rig.backend.calls.length;
      await tester.pumpWidget(const SizedBox());
      await tester.pumpWidget(_panel(rig.workspace));
      await tester.pumpAndSettle();
      expect(rig.backend.calls.length, count);
      expect(rig.workspace.view.session!.submission, token);
      expect(tester.widget<TextField>(input).controller!.text, 'notes');
      expect(_enabled(tester, 'read'), isTrue);
      expect(_enabled(tester, 'createSession'), isFalse);
      await tester.pumpWidget(const SizedBox());
      rig.workspace.dispose();
    },
  );

  testWidgets(
    'unknown survives remount and enables no business replay or reattach',
    (tester) async {
      final rig = await _opened();
      await rig.workspace.markLost();
      await tester.pumpWidget(_panel(rig.workspace));
      await tester.pumpAndSettle();
      expect(
        find.byKey(const ValueKey('agent-session-unknown')),
        findsOneWidget,
      );
      expect(_enabled(tester, 'attach'), isFalse);
      expect(_enabled(tester, 'createSession'), isFalse);
      expect(_enabled(tester, 'refreshStatus'), isTrue);
      expect(_enabled(tester, 'detach'), isTrue);
      final count = rig.backend.calls.length;
      await tester.pumpWidget(const SizedBox());
      await tester.pumpWidget(_panel(rig.workspace));
      await tester.pumpAndSettle();
      expect(rig.backend.calls.length, count);
      expect(
        find.byKey(const ValueKey('agent-session-unknown')),
        findsOneWidget,
      );
      await tester.pumpWidget(const SizedBox());
      rig.workspace.dispose();
    },
  );

  testWidgets(
    'replacing the displayed owner does not adopt an old discovery completion',
    (tester) async {
      final first = _rig(), second = _rig();
      first.discovery.held = Completer<AgentTaskObservation>();
      await tester.pumpWidget(_panel(first.workspace));
      await tester.tap(find.byKey(const ValueKey('agent-session-observe')));
      await tester.pump();
      await tester.pumpWidget(_panel(second.workspace));
      first.discovery.held!.complete(
        AgentTaskObservation(AgentTaskKind.runningAgent, fixture.task),
      );
      await tester.pumpAndSettle();
      expect(_enabled(tester, 'attach'), isFalse);
      expect(second.discovery.calls, 0);
      expect(second.backend.calls, isEmpty);
      await tester.pumpWidget(const SizedBox());
      first.workspace.dispose();
      second.workspace.dispose();
    },
  );
}
