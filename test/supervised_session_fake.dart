// Ordinary synthetic protocol peer; never opens a real host or protected library.
import 'dart:async';
import 'dart:collection';
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';
import 'support/workbench_supervision_fixture.dart';

final root = File.fromUri(Platform.script).parent;
final commands = Queue<(int, Uint8List)>();
final state = <String, dynamic>{
  'version': 2,
  'phase': 'Running',
  'owner_phase': 'Active',
  'profile': 'a' * 64,
  'incarnation': 'b' * 64,
  'host_sha256': 'c' * 64,
  'package_sha256': 'd' * 64,
  'supervisor_sha256': 'e' * 64,
  'generation': 1,
  'child_pid': pid + 1,
  'supervisor_pid': pid,
  'supervisor_creation_filetime': 1,
  'child_creation_filetime': 2,
  'protocol_ack': 'not_applicable',
  'business_outcome': 'Unknown',
  'normal_shutdown': false,
  'durable_proof': false,
  'child_exit_code': null,
  for (final k in resourceKeys) k: false,
};
const resourceKeys = [
  'resource_reclaimed',
  'tree_empty',
  'child_exit',
  'stdout_eof',
  'stderr_eof',
  'control_reaped',
  'gate_closed',
];
final summary = <String, dynamic>{
  'ordinary_synthetic_only': true,
  'real_native_host': false,
  'heartbeats': 0,
  'graceful_close_requested': false,
  'request_count': 0,
  'kind1_chunks': 0,
  'max_kind1_payload': 0,
  'status_IPC_CapnProto': true,
};
File file(String name) => File('${root.path}/$name');
void writeNew(String name, List<int> bytes) {
  final target = file(name)..createSync(exclusive: true);
  target.writeAsBytesSync(bytes, flush: true);
}

// Retained fixture controls/diagnostics are local test data, never status IPC.
void writeJson(String name, Object value) =>
    writeNew(name, utf8.encode(jsonEncode(value)));
Map<String, Object?> detail(
  Object error,
  StackTrace stack,
  String stage,
  int? request,
) {
  String bounded(String value, int limit) {
    final bytes = utf8.encode(value);
    return bytes.length <= limit
        ? value
        : utf8.decode(bytes.sublist(0, limit), allowMalformed: true);
  }

  return {
    'ordinary_synthetic_only': true,
    'type': error.runtimeType.toString(),
    'stage': stage,
    'request_index': request,
    'message': bounded('$error', 2048),
    'traceback': bounded('$stack', 16384),
    'diagnostic_cap_bytes': 32768,
  };
}

Uint8List header(int length) =>
    Uint8List(4)..buffer.asByteData().setUint32(0, length, Endian.little);
Future<void> emit(int kind, List<int> payload) async {
  final data = Uint8List.fromList([
    ...header(payload.length + 1),
    kind,
    ...payload,
  ]);
  stdout.add(data.sublist(0, 2));
  await stdout.flush();
  stdout.add(data.sublist(2));
  await stdout.flush();
}

Future<void> status() => emit(2, encodeSupervisionFixture(state));
Future<void> hostBytes(List<int> data) async {
  final frame = Uint8List.fromList([...header(data.length), ...data]);
  for (var start = 0; start < frame.length;) {
    final end = start == 0 ? 2 : (start + 16384).clamp(0, frame.length);
    final block = frame.sublist(start, end);
    summary['kind1_chunks'] = (summary['kind1_chunks'] as int) + 1;
    if (block.length > summary['max_kind1_payload']) {
      summary['max_kind1_payload'] = block.length;
    }
    await emit(1, block);
    start = end;
  }
}

Future<void> main() async {
  final buffer = <int>[];
  bool readerFailed = false;
  stdout.done.ignore();
  final input = stdin.listen(
    (bytes) {
      if (readerFailed) return;
      try {
        buffer.addAll(bytes);
        while (buffer.length >= 4) {
          final n = ByteData.sublistView(
            Uint8List.fromList(buffer.sublist(0, 4)),
          ).getUint32(0, Endian.little);
          if (n < 1 || n > 131073) {
            throw const FormatException('fixed original outer input cap');
          }
          if (buffer.length < n + 4) break;
          commands.add((
            buffer[4],
            Uint8List.fromList(buffer.sublist(5, n + 4)),
          ));
          buffer.removeRange(0, n + 4);
        }
      } catch (e, s) {
        readerFailed = true;
        writeJson('reader-error', detail(e, s, 'stdin-reader', null));
        commands.add((-2, Uint8List(0)));
      }
    },
    onDone: () => commands.add((-1, Uint8List(0))),
    onError: (Object e, StackTrace s) {
      if (readerFailed) return;
      readerFailed = true;
      writeJson('reader-error', detail(e, s, 'stdin-reader', null));
      commands.add((-2, Uint8List(0)));
    },
  );
  int? pending;
  var index = 0, normal = false, stage = 'initial-status', code = 0;
  Stopwatch? omitted;
  try {
    await status();
    while (true) {
      stage = 'command-wait';
      if (commands.isEmpty) {
        await Future<void>.delayed(const Duration(milliseconds: 2));
      }
      final command = commands.isEmpty ? null : commands.removeFirst();
      if (command != null) {
        final (kind, payload) = command;
        if (kind == -1) break;
        if (kind == -2) throw const FormatException('synthetic stdin failure');
        if (kind == 2) {
          if (payload.isNotEmpty) throw const FormatException('heartbeat body');
          summary['heartbeats'] = (summary['heartbeats'] as int) + 1;
        } else if (kind == 3) {
          if (payload.isNotEmpty) throw const FormatException('close body');
          summary['graceful_close_requested'] = true;
          state['phase'] = 'Closing';
          state['owner_phase'] = 'Closing';
          state['gate_closed'] = true;
          await status();
        } else if (kind == 1) {
          if (pending != null || payload.isEmpty || payload.length > 131072) {
            throw const FormatException('single ordinary RPC slot');
          }
          stage = 'request-write';
          writeNew('request.$index.tmp', payload);
          stage = 'request-publish';
          file(
            'request.$index.tmp',
          ).renameSync(file('request.$index.bin').path);
          summary['request_count'] = (summary['request_count'] as int) + 1;
          pending = index;
        } else {
          throw const FormatException('unexpected outer kind');
        }
      }
      stage = 'status-command-probe';
      if (file('status-command.json').existsSync() &&
          !file('status-command-observed').existsSync()) {
        stage = 'status-command-read-and-emit';
        state.addAll(
          jsonDecode(file('status-command.json').readAsStringSync())
              as Map<String, dynamic>,
        );
        await status();
        writeJson('status-command-observed', {'synthetic': true});
      }
      stage = 'reply-control-probe';
      if (pending != null && file('reply.$pending.json').existsSync()) {
        stage = 'reply-control-read';
        final control =
            jsonDecode(file('reply.$pending.json').readAsStringSync())
                as Map<String, dynamic>;
        stage = 'reply-data-read';
        final data = file('reply.$pending.bin').readAsBytesSync();
        if (data.length > 393217) {
          throw const FormatException('synthetic output evidence bound');
        }
        if (control['drift'] == true) {
          state['generation']++;
          await status();
        }
        if (control['gate'] == true) {
          state['gate_closed'] = true;
          await status();
        }
        if (control['drop'] == true) {
          writeJson('deliberate-eof', {'synthetic_missing_read_reply': true});
          break;
        }
        stage = 'reply-emit';
        if (control['outer_bad'] == true) {
          stdout.add(header(131074));
          await stdout.flush();
          writeJson('outer-budget-probe', {'declared_outer_bytes': 131074});
        } else {
          await hostBytes(data);
        }
        pending = null;
        index++;
      }
      stage = 'graceful-release-probe';
      if (summary['graceful_close_requested'] == true && pending == null) {
        if (file('omit-release-proof').existsSync()) {
          omitted ??= Stopwatch()..start();
          if (!file('allow-exit-without-proof').existsSync()) {
            if (omitted.elapsed >= const Duration(seconds: 10)) {
              throw TimeoutException('synthetic omitted-proof hold deadline');
            }
            continue;
          }
          summary['release_proof_deliberately_omitted'] = true;
          break;
        }
        state.addAll({
          'phase': 'Released',
          'owner_phase': 'Released',
          'normal_shutdown': true,
          'durable_proof': true,
          'child_exit_code': 0,
          for (final k in resourceKeys) k: true,
        });
        stage = 'graceful-release-emit';
        await status();
        normal = true;
        break;
      }
    }
  } catch (e, s) {
    if (e is FileSystemException &&
        [109, 232, 233].contains(e.osError?.errorCode)) {
      summary['synthetic_output_pipe_closed'] = true;
    } else {
      writeJson('child-error', detail(e, s, stage, pending));
      code = 1;
    }
  } finally {
    summary['synthetic_protocol_released'] = normal;
    writeJson('transport-summary.json', summary);
    writeJson('drain-complete', {'synthetic_process_finally': true});
    await input.cancel();
  }
  exit(code);
}
