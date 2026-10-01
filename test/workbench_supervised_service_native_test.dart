import 'dart:convert';
import 'dart:io';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/service_run_control.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';
import 'package:morrow_studio/plugins/workbench_channel_supervised.dart';
import 'service_run_real_native_fixture.dart';

void main() {
  final evidence = Platform.environment['MORROW_PRODUCT_SUPERVISOR_EVIDENCE'];
  final supervisor = Platform.environment['MORROW_WORKBENCH_SUPERVISOR'];
  test(
    'supervised production graceful close joins original live service and closes actual listener',
    () async {
      final f = await RealServiceFixture.open(
        openBackend: (directory) => RustWorkbench.open(
          executable: Platform.environment['MORROW_WORKBENCH_HOST']!,
          supervisorExecutable: supervisor!,
          package: Platform.environment['MORROW_WORKBENCH_PACKAGE']!,
          directory: directory,
          managed: true,
        ),
      );
      final channel = f.backend.channel as SupervisedWorkbenchChannel;
      try {
        await f.session.start(f.request(lifetime: 30000));
        final running = await f.observe(ServiceRunPhase.running);
        final reply = await f.post();
        expect(reply, startsWith('HTTP/1.1 202 '));
        expect(reply, endsWith('executed-before'));
        final watch = Stopwatch()..start();
        // No service stop command or fake join: ordinary EOF drains its original owner.
        await f.backend.close();
        expect(channel.supervision.value!.released, isTrue);
        await expectLater(
          f.post(second: true),
          throwsA(isA<SocketException>()),
        );
        final archive = Directory('$evidence/service-library');
        expect(archive.existsSync(), isFalse);
        await archive.create();
        // Preserve the original committed owner/Store/plugin evidence before this
        // private fixture removes only its own temporary directory in teardown.
        await for (final entity in f.directory.list(
          recursive: true,
          followLinks: false,
        )) {
          final relative = entity.path.substring(f.directory.path.length + 1);
          final target = '${archive.path}/$relative';
          if (entity is Directory) {
            await Directory(target).create(recursive: true);
          } else if (entity is File) {
            await File(target).parent.create(recursive: true);
            await entity.copy(target);
          } else {
            throw StateError(
              'Unexpected link in private service fixture evidence',
            );
          }
        }
        await File('$evidence/service-normal.json').writeAsString(
          jsonEncode({
            'status': 'passed',
            'real_service_worker': true,
            'real_loopback_http': true,
            'guest':
                'existing two-request WAT routing fixture; builtin business guest remains actual Rust Wasmi',
            'original_task_key': running.task.key.toString(),
            'listener': f.address,
            'reply': reply,
            'normal_close_ms': watch.elapsedMilliseconds,
            'final': channel.supervision.value!.record,
            'automatic_replay': false,
          }),
        );
      } finally {
        await f.close(observeBeforeClose: false);
      }
    },
    skip:
        !RealServiceFixture.available || supervisor == null || evidence == null,
    timeout: const Timeout(Duration(minutes: 2)),
  );
}
