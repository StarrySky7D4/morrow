// Pure model compatibility; no IO, Process, FFI or native call.
import 'dart:convert';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/workbench_supervision.dart';
void main() {
  test('default startup observation serializes nullable quarantine without release', () {
    final observation = WorkbenchStartupObservation(
      stage: 'before_original_close', primaryFailure: StateError('synthetic startup'),
      initialValidatedState: null, latestValidatedState: null,
      diagnosticBytes: const [], diagnosticBytesObserved: 0,
      actualExitCode: null, stdoutEof: false, stderrEof: false,
    );
    final value = observation.toJson();
    expect(value.containsKey('failed_startup_quarantine'), isTrue);
    expect(value['failed_startup_quarantine'], isNull);
    final decoded = jsonDecode(jsonEncode(value)) as Map<String, dynamic>;
    expect(decoded.containsKey('failed_startup_quarantine'), isTrue);
    expect(decoded['failed_startup_quarantine'], isNull);
    expect(decoded['accepted'], isFalse);
    expect(decoded['incomplete'], isTrue);
    expect(decoded['actual_exit_known'], isFalse);
    expect(decoded['original_close_settled'], isFalse);
    expect(decoded['stdout_eof'], isFalse);
    expect(decoded['stderr_eof'], isFalse);
    expect(() => value['failed_startup_quarantine'] = const {}, throwsUnsupportedError);
  });
}
