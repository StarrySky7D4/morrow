// SOURCE ONLY. ROOT runs this with ordinary Python and an independent retained
// fixture directory. No application/host PE, protected database or VM is used.
import 'dart:io';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/agent_session_control.dart';
import 'package:morrow_studio/plugins/agent_session_manager.dart';
import 'package:morrow_studio/plugins/agent_session_workspace.dart';
import 'package:morrow_studio/plugins/generated/host.capnp.dart' as host;
import 'agent_session_workbench_routing_helper.dart';

void main() {
  final python = Platform.environment['MORROW_TEST_PYTHON'];
  final evidence = Platform.environment['MORROW_SESSION_PIPE_EVIDENCE'];
  final bool skip = python == null || evidence == null;

  testWidgets(
    'original product getter retains one workspace; mount is silent and owner close seals before EOF',
    (tester) async {
      final script = SessionPipeScript();
      SessionPipeChild? child;
      try {
        await tester.runAsync(() async {
          child = await SessionPipeChild.open(
            python!,
            Directory(evidence!),
            script,
          ).timeout(const Duration(seconds: 15));
        });
        final original = child!;
        final backend = original.backend;
        expect(backend, isA<WorkbenchAgentSessionSupport>());
        final workspace = backend.agentSessions;
        expect(identical(workspace, backend.agentSessions), isTrue);
        expect(workspace.closed, isFalse);
        expect(workspace.view.observation, isNull);
        expect(workspace.view.session, isNull);
        // The real original RustWorkbench.open handshake is allowed. No
        // discovery or session operation is needed or synthesized by this test.
        expect(script.business.actions, [host.Action.pageVersioned]);
        expect(script.privateOps, isEmpty);
        expect(script.business.adminRequests, isEmpty);

        Widget panel() => MaterialApp(
          home: Scaffold(
            body: SingleChildScrollView(
              child: AgentSessionManager(workspace: backend.agentSessions),
            ),
          ),
        );
        await tester.pumpWidget(panel());
        await tester.pumpAndSettle();
        await tester.pumpWidget(const SizedBox());
        expect(workspace.closed, isFalse);
        await tester.pumpWidget(panel());
        await tester.pumpAndSettle();
        expect(identical(workspace, backend.agentSessions), isTrue);
        expect(workspace.view.observation, isNull);
        expect(workspace.view.session, isNull);

        await tester.runAsync(() async {
          // No await between requesting the original owner close and checking
          // local sealing. _closeProcess has not yet passed its first await.
          final closing = backend.close();
          expect(workspace.closed, isTrue);
          expect(workspace.view.canObserve, isFalse);
          expect(workspace.view.canAttach, isFalse);
          expect(
            File('${original.directory.path}/eof-observed').existsSync(),
            isFalse,
          );
          expect(
            File('${original.directory.path}/drain-complete').existsSync(),
            isFalse,
          );
          expect(identical(workspace, backend.agentSessions), isTrue);
          // Only now await the ordinary Python process and actual EOF markers.
          // This is synthetic channel shutdown, never original Agent join proof.
          await closing.timeout(const Duration(seconds: 10));
          expect(
            await backend.process.exitCode.timeout(const Duration(seconds: 10)),
            0,
          );
          await original.waitMarker('eof-observed');
          await original.waitMarker('drain-complete');
          expect(
            File('${original.directory.path}/child-error').existsSync(),
            isFalse,
          );
          // Fixed finite files only. A new AD31/PS30/legacy request, even one
          // rejected by the script, would create request.1.bin before EOF.
          expect(
            File('${original.directory.path}/request.0.bin').existsSync(),
            isTrue,
          );
          expect(
            File('${original.directory.path}/request.1.bin').existsSync(),
            isFalse,
          );
          expect(script.business.actions, [host.Action.pageVersioned]);
          expect(script.privateFrames, isEmpty);
          expect(script.business.adminFrames, isEmpty);
          expect(
            () =>
                workspace.setDraft(AgentSessionDraft.sessionId, 'after-close'),
            throwsA(
              isA<AgentSessionFailure>().having(
                (error) => error.kind,
                'synchronously sealed owner',
                AgentSessionFailureKind.disposed,
              ),
            ),
          );
        });
        await tester.pumpAndSettle();
        final observe = tester.widget<OutlinedButton>(
          find.byKey(const ValueKey('agent-session-observe')),
        );
        expect(observe.onPressed, isNull);
      } finally {
        await tester.pumpWidget(const SizedBox());
        final original = child;
        if (original != null) {
          await tester.runAsync(
            () => original.close().timeout(const Duration(seconds: 15)),
          );
        }
        // The helper retains every raw request/reply and synthetic directory.
      }
    },
    skip: skip,
    timeout: const Timeout(Duration(seconds: 60)),
  );

  for (final closePending in [false, true]) {
    test(
      closePending
          ? 'first product getter during original close returns one synchronously sealed workspace'
          : 'first product getter after original close returns one sealed workspace',
      () async {
        final script = SessionPipeScript();
        SessionPipeChild? child;
        try {
          child = await SessionPipeChild.open(
            python!, Directory(evidence!), script,
          ).timeout(const Duration(seconds: 15));
          final original = child;
          // Deliberately do not read agentSessions before close().
          final closing = original.backend.close();
          if (!closePending) {
            await closing.timeout(const Duration(seconds: 10));
          }
          final workspace = original.backend.agentSessions;
          expect(identical(workspace, original.backend.agentSessions), isTrue);
          expect(workspace.closed, isTrue);
          expect(workspace.view.canObserve, isFalse);
          expect(workspace.view.canAttach, isFalse);
          expect(workspace.view.canRead, isFalse);
          expect(workspace.view.canCloseWriter, isFalse);
          expect(workspace.view.observation, isNull);
          expect(workspace.view.session, isNull);
          expect(
            () => workspace.setDraft(AgentSessionDraft.sessionId, 'retired'),
            throwsA(isA<AgentSessionFailure>().having(
              (error) => error.kind, 'local owner sealing',
              AgentSessionFailureKind.disposed,
            )),
          );
          if (closePending) {
            // The local close prefix has run; actual input EOF and process
            // ownership have not been awaited or represented as complete.
            expect(File('${original.directory.path}/eof-observed').existsSync(), isFalse);
            expect(File('${original.directory.path}/drain-complete').existsSync(), isFalse);
          }
          await closing.timeout(const Duration(seconds: 10));
          await workspace.changes.drain<void>().timeout(const Duration(seconds: 5));
          await _expectOnlyOriginalHandshake(original);
        } finally {
          final original = child;
          if (original != null) {
            await original.close().timeout(const Duration(seconds: 15));
          }
        }
      },
      skip: skip,
      timeout: const Timeout(Duration(seconds: 60)),
    );
  }

  for (final cachedBeforeFailure in [false, true]) {
    test(
      cachedBeforeFailure
          ? 'original channel EOF seals and notifies a cached product workspace without another getter'
          : 'first product getter after original channel EOF cannot create a live workspace',
      () async {
        final script = SessionPipeScript();
        SessionPipeChild? child;
        try {
          child = await SessionPipeChild.open(
            python!, Directory(evidence!), script,
          ).timeout(const Duration(seconds: 15));
          final original = child;
          final cached = cachedBeforeFailure ? original.backend.agentSessions : null;
          final closedNotice = cached?.changes.firstWhere((view) => view.closed);
          // Close only the original ordinary Python channel input. This sends
          // no host/admin/AD31/PS30 request and does not call owner.close().
          // The resulting stdout EOF drives RustWorkbench._ended -> _fail.
          await original.backend.channel.closeInput().timeout(const Duration(seconds: 10));
          await original.backend.process.exitCode.timeout(const Duration(seconds: 10));
          if (cached != null) {
            await closedNotice!.timeout(const Duration(seconds: 10));
            // This assertion precedes the next getter: _fail must notify the
            // already mounted workspace, not rely on a later settings visit.
            expect(cached.closed, isTrue);
          } else {
            // Process exit and stream EOF are different observations. Wait on
            // the pure support flag until _ended has recorded permanent failure.
            final elapsed = Stopwatch()..start();
            while (original.backend.agentSessionControl().supportsAgentSessions) {
              if (elapsed.elapsed > const Duration(seconds: 10)) {
                fail('Original channel EOF did not record permanent failure');
              }
              await Future<void>.delayed(const Duration(milliseconds: 2));
            }
          }
          final workspace = original.backend.agentSessions;
          if (cached != null) {
            expect(identical(cached, workspace), isTrue);
          }
          expect(identical(workspace, original.backend.agentSessions), isTrue);
          expect(workspace.closed, isTrue);
          expect(workspace.view.canObserve, isFalse);
          expect(workspace.view.canAttach, isFalse);
          expect(workspace.view.canSubmit, isFalse);
          expect(workspace.view.canRead, isFalse);
          expect(workspace.view.canCloseWriter, isFalse);
          expect(
            () => workspace.setDraft(AgentSessionDraft.eventBody, 'after EOF'),
            throwsA(isA<AgentSessionFailure>().having(
              (error) => error.kind, 'permanent failure sealing',
              AgentSessionFailureKind.disposed,
            )),
          );
          await workspace.changes.drain<void>().timeout(const Duration(seconds: 5));
          await _expectOnlyOriginalHandshake(original);
        } finally {
          final original = child;
          if (original != null) {
            await original.close().timeout(const Duration(seconds: 15));
          }
        }
      },
      skip: skip,
      timeout: const Timeout(Duration(seconds: 60)),
    );
  }
}


Future<void> _expectOnlyOriginalHandshake(SessionPipeChild original) async {
  expect(await original.backend.process.exitCode.timeout(const Duration(seconds: 10)), 0);
  await original.waitMarker('eof-observed');
  await original.waitMarker('drain-complete');
  expect(File('${original.directory.path}/request.0.bin').existsSync(), isTrue);
  expect(File('${original.directory.path}/request.1.bin').existsSync(), isFalse);
  expect(File('${original.directory.path}/child-error').existsSync(), isFalse);
  expect(original.script.business.actions, [host.Action.pageVersioned]);
  expect(original.script.privateFrames, isEmpty);
  expect(original.script.privateOps, isEmpty);
  expect(original.script.business.adminFrames, isEmpty);
  expect(original.script.business.adminRequests, isEmpty);
}
