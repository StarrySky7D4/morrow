// SOURCE ONLY until ROOT explicitly runs Flutter --no-pub in its new candidate.
// Tests the real Dart original owner/pipe with synthetic Python replies only.
import 'dart:async';
import 'dart:io';
import 'dart:typed_data';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/agent_session_control.dart';
import 'package:morrow_studio/plugins/generated/host.capnp.dart' as host;
import 'package:morrow_studio/plugins/workbench_channel_native.dart';
import 'agent_session_workbench_routing_helper.dart';
import 'agent_wrapper_workbench_routing_helper.dart' as legacy;

void main() {
  final python = Platform.environment['MORROW_TEST_PYTHON'];
  final evidence = Platform.environment['MORROW_SESSION_PIPE_EVIDENCE'];
  final skip = python == null || evidence == null
      ? 'ROOT must supply actual Python and independent retained synthetic evidence parent'
      : false;
  Future<SessionPipeChild> start(SessionPipeScript script) =>
      SessionPipeChild.open(python!, Directory(evidence!), script);
  Future<void> finish(
    SessionPipeChild child, {
    bool deliberateEof = false,
  }) async {
    // No directory is deleted. Even failures preserve all raw fixture bytes.
    try {
      await child.close();
    } catch (error) {
      if (!deliberateEof ||
          !await File('${child.directory.path}/deliberate-eof').exists() ||
          await child.backend.process.exitCode != 0) {
        rethrow;
      }
      await File(
        '${child.directory.path}/expected-close-error.txt',
      ).writeAsString(error.runtimeType.toString(), mode: FileMode.writeOnly);
    }
    expect(await child.backend.process.exitCode, 0);
    await child.waitMarker('drain-complete');
    expect(File('${child.directory.path}/child-error').existsSync(), false);
  }

  final decoderError = isA<FormatException>().having(
    (e) => e.message,
    'fixed codec rejection',
    'Private session receipt mismatch',
  );
  final pipeSizeError = isA<FormatException>().having(
    (e) => e.message,
    'original receive cap',
    '内容服务消息过大',
  );
  final sealed = isA<AgentSessionFailure>().having(
    (e) => e.kind,
    'no replay',
    AgentSessionFailureKind.unknown,
  );

  test(
    'original direct private RPC bypasses pending business and retains fixed codec/client',
    () async {
      final script = SessionPipeScript();
      final child = await start(script);
      Future<String>? business;
      try {
        expect(child.backend.channel, isA<NativeWorkbenchChannel>());
        final control = child.backend.agentSessionControl();
        expect(identical(control, child.backend.agentSessionControl()), true);
        expect(control.supportsAgentSessions, true);
        await child.backend.startServiceRun(legacy.routingStart());
        var complete = false;
        business = child.backend.readUiLocale().then((value) {
          complete = true;
          return value;
        });
        business.ignore();
        await script.business.submitted.future.timeout(
          const Duration(seconds: 10),
        );
        await script.business.statusObserved.future.timeout(
          const Duration(seconds: 10),
        );
        expect(complete, false);
        final attached = await control
            .attachSession(pipeTask)
            .timeout(const Duration(seconds: 5));
        expect(attached.status, AgentSessionStatus.attached);
        expect(attached.attachment, pipeAttachment);
        final submitted = await control
            .openSession(pipeTask, attached.attachment)
            .timeout(const Duration(seconds: 5));
        expect(submitted.status, AgentSessionStatus.submitted);
        expect(submitted.submission, pipeToken);
        // Explicit token returned immediately; no automatic Read/poll occurred.
        expect(script.privateOps, [1, 2]);
        expect(complete, false);
        final pending = await control.readSession(
          pipeTask,
          pipeAttachment,
          pipeToken,
        );
        expect(pending.status, AgentSessionStatus.pending);
        final opened = await control.readSession(
          pipeTask,
          pipeAttachment,
          pipeToken,
        );
        expect(opened.status, AgentSessionStatus.opened);
        expect(opened.endpointGeneration, pipeGeneration);
        expect(script.privateOps, [1, 2, 5, 5]);
        expect(complete, false);
        expect(script.business.businessActions, [host.Action.readUiLocale]);
        expect(
          script.business.actions
              .where((a) => a == host.Action.commandSubmit)
              .length,
          1,
        );
        expect(
          script.business.actions,
          isNot(contains(host.Action.commandRead)),
        );
        script.business.businessReady = true;
        expect(await business.timeout(const Duration(seconds: 10)), 'en');
        expect(
          script.business.actions
              .where((a) => a == host.Action.commandRead)
              .length,
          1,
        );
        expect(control.sessionOutcomeUnknown, false);
      } finally {
        script.business.businessReady = true;
        try {
          if (business != null) {
            await business.timeout(const Duration(seconds: 10));
          }
        } finally {
          await finish(child);
        }
      }
    },
    skip: skip,
    timeout: const Timeout(Duration(seconds: 45)),
  );

  test(
    'production Pending context keeps token but terminal Read is consumed once',
    () async {
      final script = SessionPipeScript();
      final child = await start(script);
      try {
        final control = child.backend.agentSessionControl();
        await control.attachSession(pipeTask);
        await control.openSession(pipeTask, pipeAttachment);
        expect(
          (await control.readSession(
            pipeTask,
            pipeAttachment,
            pipeToken,
          )).status,
          AgentSessionStatus.pending,
        );
        expect(
          (await control.readSession(
            pipeTask,
            pipeAttachment,
            pipeToken,
          )).status,
          AgentSessionStatus.opened,
        );
        final frames = script.privateFrames.length;
        await expectLater(
          control.readSession(pipeTask, pipeAttachment, pipeToken),
          throwsA(sealed),
        );
        expect(script.privateFrames.length, frames);
        // No token reset, no new native owner or implicit second Open.
        expect(script.privateOps.where((op) => op == 2).length, 1);
      } finally {
        await finish(child);
      }
    },
    skip: skip,
    timeout: const Timeout(Duration(seconds: 30)),
  );

  test(
    '300KiB private frame reaches fixed decoder, then original receive cap restores 256KiB',
    () async {
      final script = SessionPipeScript()..largePrivate = true;
      final child = await start(script);
      try {
        final control = child.backend.agentSessionControl();
        await expectLater(
          control.attachSession(pipeTask),
          throwsA(decoderError),
        );
        expect(control.sessionOutcomeUnknown, true);
        final frames = script.privateFrames.length;
        await expectLater(control.attachSession(pipeTask), throwsA(sealed));
        expect(script.privateFrames.length, frames);
        // Decoder rejection does not pretend this invalid 300KiB payload was a valid Reply.
        expect(await child.backend.readUiLocale(), 'en');
        script.largeLegacy = true;
        await expectLater(child.backend.readUiLocale(), throwsA(pipeSizeError));
      } finally {
        await finish(child);
      }
    },
    skip: skip,
    timeout: const Timeout(Duration(seconds: 30)),
  );

  test(
    'private reply above 384KiB is rejected by original pipe and seals client',
    () async {
      final script = SessionPipeScript()..tooLargePrivate = true;
      final child = await start(script);
      try {
        final control = child.backend.agentSessionControl();
        await expectLater(
          control.attachSession(pipeTask),
          throwsA(pipeSizeError),
        );
        expect(control.sessionOutcomeUnknown, true);
        expect(script.privateOps, [1]);
      } finally {
        await finish(child);
      }
    },
    skip: skip,
    timeout: const Timeout(Duration(seconds: 30)),
  );

  test(
    'small malformed private receipt seals without automatic replay',
    () async {
      final script = SessionPipeScript()..malformedPrivate = true;
      final child = await start(script);
      try {
        final control = child.backend.agentSessionControl();
        await expectLater(
          control.attachSession(pipeTask),
          throwsA(decoderError),
        );
        expect(control.sessionOutcomeUnknown, true);
        await expectLater(control.attachSession(pipeTask), throwsA(sealed));
        expect(script.privateOps, [1]);
        expect(await child.backend.readUiLocale(), 'en');
      } finally {
        await finish(child);
      }
    },
    skip: skip,
    timeout: const Timeout(Duration(seconds: 30)),
  );

  test(
    'actual synthetic stdout EOF after Read burns token and prevents repeat execution',
    () async {
      final script = SessionPipeScript()..dropRead = true;
      final child = await start(script);
      try {
        final control = child.backend.agentSessionControl();
        await control.attachSession(pipeTask);
        await control.openSession(pipeTask, pipeAttachment);
        await expectLater(
          control.readSession(pipeTask, pipeAttachment, pipeToken),
          throwsA(anything),
        );
        await child.waitMarker('deliberate-eof');
        expect(control.sessionOutcomeUnknown, true);
        await expectLater(
          control.readSession(pipeTask, pipeAttachment, pipeToken),
          throwsA(isA<AgentSessionFailure>().having(
            (e) => e.kind,
            'physical pipe unavailable; uncertain token is never replayed',
            AgentSessionFailureKind.unavailable,
          )),
        );
        expect(control.sessionOutcomeUnknown, true);
        expect(script.privateOps, [1, 2, 5]);
      } finally {
        await finish(child, deliberateEof: true);
      }
    },
    skip: skip,
    timeout: const Timeout(Duration(seconds: 30)),
  );

  test(
    'without current private pending, unsolicited 300KiB is rejected under original 256KiB cap',
    () async {
      final script = SessionPipeScript()..idleAfterAttach = true;
      final child = await start(script);
      try {
        final control = child.backend.agentSessionControl();
        expect(
          (await control.attachSession(pipeTask)).status,
          AgentSessionStatus.attached,
        );
        await child.emitIdle(Uint8List(300 * 1024));
        // Observe the existing public availability property instead of guessing
        // when stdout has been drained or introducing a test-only production API.
        final timer = Stopwatch()..start();
        while (control.supportsAgentSessions) {
          if (timer.elapsed > const Duration(seconds: 10)) {
            throw TimeoutException(
              'Original owner did not reject unsolicited frame',
            );
          }
          await Future<void>.delayed(const Duration(milliseconds: 2));
        }
        await expectLater(child.backend.readUiLocale(), throwsA(pipeSizeError));
        expect(script.privateOps, [1]);
      } finally {
        await finish(child);
      }
    },
    skip: skip,
    timeout: const Timeout(Duration(seconds: 30)),
  );
}
