// Offline test-fixture projection from independently generated Rust Cap'n Proto bytes.
import 'dart:convert';
import 'dart:io';
import 'package:morrow_core_client/ui.dart';

String literal(String value) => jsonEncode(value).replaceAll(r'$', r'\$');
void main(List<String> args) {
  if (args.length < 2 || args.length > 3)
    throw ArgumentError('source.capnp output.dart [--check]');
  final doc = UiDocument.decode(File(args[0]).readAsBytesSync());
  final lines = <String>[
    "// Generated from the independent Rust UI fixture. Do not edit.",
    "import 'package:morrow_plugin_ui/morrow_plugin_ui.dart';",
    "final browserFormFixture = UiDocumentModel([",
  ];
  for (final n in doc.nodes) {
    lines.add(
      'UiNode(id: ${literal(n.id)}, parent: ${literal(n.parent)}, kind: Kind.${n.kind.name}, label: ${literal(n.label)}, text: ${literal(n.text)}, action: ${literal(n.action)}, enabled: ${n.enabled}, checked: ${n.checked}, maxBytes: ${n.maxBytes}, tone: Tone.${n.tone.name}),',
    );
  }
  lines.add(']);');
  final temporary = Directory.systemTemp.createTempSync('morrow-ui-fixture-');
  final generated = File('${temporary.path}/fixture.dart');
  late String result;
  try {
    generated.writeAsStringSync(lines.join('\n') + '\n');
    final format = Process.runSync(Platform.resolvedExecutable, [
      'format',
      generated.path,
    ]);
    if (format.exitCode != 0)
      throw StateError('Fixture formatting failed: ${format.stderr}');
    result = generated.readAsStringSync();
  } finally {
    if (generated.existsSync()) generated.deleteSync();
    temporary.deleteSync();
  }
  final file = File(args[1]);
  if (args.length == 3) {
    if (args[2] != '--check' ||
        !file.existsSync() ||
        file.readAsStringSync().replaceAll('\r\n', '\n') !=
            result.replaceAll('\r\n', '\n'))
      throw StateError('Stale renderer model fixture');
  } else {
    file.writeAsStringSync(result);
  }
  print(
    'PASS: renderer model fixture matches independently decoded Rust document',
  );
}
