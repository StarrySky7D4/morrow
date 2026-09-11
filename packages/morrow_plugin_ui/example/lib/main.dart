import 'dart:js_interop';
import 'package:flutter/material.dart';
import 'package:flutter/semantics.dart';
import 'package:flutter/services.dart';
import 'package:morrow_plugin_ui/morrow_plugin_ui.dart';
import 'form_fixture.dart';

@JS('coreProbeResult')
external set probeResult(JSString value);
@JS('rendererEdits')
external set edits(JSArray<JSString> value);
final records = <String>[];
late final SemanticsHandle semantics;
bool failed = false;
Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  semantics = SemanticsBinding.instance.ensureSemantics();
  FlutterError.onError = (details) {
    failed = true;
    probeResult = 'FAIL: ${details.exceptionAsString()}'.toJS;
    FlutterError.presentError(details);
  };
  try {
    for (final entry in {
      'MorrowTest': 'text',
      'MorrowEmoji': 'emoji',
    }.entries) {
      final loader = FontLoader(entry.key)
        ..addFont(rootBundle.load('test-fonts/${entry.value}'));
      await loader.load();
    }
  } catch (error) {
    failed = true;
    probeResult = 'FAIL: test fonts: $error'.toJS;
    return;
  }
  runApp(const Preview());
  WidgetsBinding.instance.addPostFrameCallback((_) {
    if (!failed) probeResult = 'READY'.toJS;
  });
}

class Preview extends StatelessWidget {
  const Preview({super.key});
  @override
  Widget build(BuildContext context) => MaterialApp(
    debugShowCheckedModeBanner: false,
    theme: ThemeData(
      useMaterial3: true,
      fontFamily: 'MorrowTest',
      fontFamilyFallback: const ['MorrowEmoji'],
      colorScheme: ColorScheme.fromSeed(seedColor: Colors.teal),
      inputDecorationTheme: InputDecorationTheme(
        filled: true,
        border: OutlineInputBorder(borderRadius: BorderRadius.circular(13)),
      ),
      filledButtonTheme: FilledButtonThemeData(
        style: FilledButton.styleFrom(
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(12),
          ),
        ),
      ),
    ),
    home: Scaffold(
      body: SafeArea(
        child: Padding(
          padding: const EdgeInsets.all(24),
          child: PluginForm(
            document: browserFormFixture,
            viewIdentity: 'browser-preview',
            onIntent: (intent) {
              records.add(
                '${intent.kind.name}|${intent.node}|${intent.text}|${intent.checked}',
              );
              edits = records.map((s) => s.toJS).toList().toJS;
            },
          ),
        ),
      ),
    ),
  );
}
