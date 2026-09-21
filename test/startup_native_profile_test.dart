import 'dart:convert';
import 'dart:io';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';
import 'package:morrow_studio/plugins/studio_storage.dart';

void main() {
  final executable = Platform.environment['MORROW_WORKBENCH_HOST'];
  final package = Platform.environment['MORROW_WORKBENCH_PACKAGE'];
  final directory = Platform.environment['MORROW_STARTUP_LIBRARY'];
  final output = Platform.environment['MORROW_STARTUP_OUTPUT'];
  test(
    'Measure isolated native startup phases',
    () async {
      final timings = <String, Object>{};
      final clock = Stopwatch()..start();
      var previous = 0;
      void mark(String name) {
        final current = clock.elapsedMicroseconds;
        timings[name] = (current - previous) / 1000;
        previous = current;
      }

      final backend = await RustWorkbench.open(
        executable: executable!,
        package: package!,
        directory: Directory(directory!),
      );
      mark('host_open_ms');
      try {
        final bytes = await backend.readPreferences();
        final preferences = bytes == null
            ? <String, dynamic>{}
            : decodePreferences(bytes);
        mark('preferences_ms');
        final ideas = await backend.load();
        mark('content_ms');
        await backend.readUiLocale();
        mark('locale_ms');
        jsonEncode({
          ...preferences,
          'ideas': ideas.map((v) => v.toJson()).toList(),
        });
        mark('presentation_ms');
        timings['ready_ms'] = clock.elapsedMicroseconds / 1000;
        timings['cards'] = ideas.length;
        timings['attachments'] = ideas.fold<int>(
          0,
          (n, idea) => n + idea.attachments.length,
        );
        timings['writable'] = backend.writable;
        await File(
          output!,
        ).writeAsString(const JsonEncoder.withIndent('  ').convert(timings));
      } finally {
        await backend.close();
      }
    },
    skip:
        executable == null ||
        package == null ||
        directory == null ||
        output == null,
  );
}
