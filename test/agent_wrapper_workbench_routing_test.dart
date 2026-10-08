// Actual original pipe and Dart scheduling, with simulated protocol replies only.
// This is not Rust authorization, a real service worker, or protected library proof.
import 'dart:io';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/agent_wrapper_control.dart';
import 'package:morrow_studio/plugins/agent_wrapper_models.dart';
import 'package:morrow_studio/plugins/generated/host.capnp.dart' as host;
import 'package:morrow_studio/plugins/workbench_channel_native.dart';
import 'agent_wrapper_workbench_routing_helper.dart';

void main() {
  final python =
      Platform.environment['MORROW_TEST_PYTHON'] ??
      Platform.environment['MORROW_CLOSE_TEST_PYTHON'];
  test(
    'original pipe sends admin directly while service business is pending, then joins EOF',
    () async {
      final script = ControlledRoutingScript();
      final child = await ControlledRoutingChild.open(python!, script);
      var assertionsFailed = false;
      try {
        expect(child.backend.channel, isA<NativeWorkbenchChannel>());
        expect(child.backend.supportsAgentWrappers, true);
        await child.backend.startServiceRun(routingStart());
        var businessComplete = false;
        final business = child.backend.readUiLocale().then((value) {
          businessComplete = true;
          return value;
        });
        business.ignore();
        await script.submitted.future.timeout(const Duration(seconds: 10));
        await script.statusObserved.future.timeout(const Duration(seconds: 10));
        expect(businessComplete, false);
        await expectLater(
          child.backend.wrapperState(),
          throwsA(
            isA<AgentWrapperFailure>().having(
              (failure) => failure.kind,
              'kind',
              AgentWrapperFailureKind.busy,
            ),
          ),
        );
        expect(businessComplete, false);
        expect(child.backend.wrapperOutcomeUnknown, false);
        expect(script.adminRequests.map((r) => r.action), [
          AgentWrapperAction.state,
        ]);
        expect(script.adminFrames.length, 1);
        expect(script.businessActions, [host.Action.readUiLocale]);
        expect(
          script.actions.where((a) => a == host.Action.commandSubmit).length,
          1,
        );
        expect(script.actions, isNot(contains(host.Action.commandRead)));
        expect(
          script.events,
          containsAllInOrder([
            'serviceRunStart',
            'commandSubmit',
            'commandStatus',
            'admin:state',
          ]),
        );
        script.businessReady = true;
        expect(await business.timeout(const Duration(seconds: 10)), 'en');
        expect(
          script.actions.where((a) => a == host.Action.commandRead).length,
          1,
        );
        expect(script.businessActions.length, 1);
        final process = child.backend.process;
        await child.close();
        expect(await process.exitCode, 0);
        expect(
          await File('${child.directory.path}/eof-observed').readAsString(),
          'EOF',
        );
        expect(
          await File('${child.directory.path}/drain-complete').readAsString(),
          'joined',
        );
        expect(File('${child.directory.path}/child-error').existsSync(), false);
      } catch (_) {
        assertionsFailed = true;
        rethrow;
      } finally {
        try {
          await child.dispose();
        } catch (error) {
          if (!assertionsFailed) {
            rethrow;
          }
          stderr.writeln(
            'Secondary controlled routing cleanup failure: $error',
          );
        }
      }
    },
    skip: python == null
        ? 'Root must supply a real ordinary Python executable in MORROW_TEST_PYTHON'
        : false,
    timeout: const Timeout(Duration(seconds: 45)),
  );
}
