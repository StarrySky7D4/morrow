// Run after verify_plugin_ui_sdk has produced fresh, host-validated guest outputs.
import 'dart:io';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_core_client/ui.dart' show UiDocument;
import 'package:morrow_plugin_ui/morrow_plugin_ui.dart';

void main() {
  for (var index = 0; index < 3; index++) {
    testWidgets('actual guest $index form and updated snapshot render', (
      tester,
    ) async {
      final initial = UiDocument.decode(
        File(
          '../../build/plugin-ui-sdk/artifacts/guest-$index-form.capnp',
        ).readAsBytesSync(),
      );
      final updated = UiDocument.decode(
        File(
          '../../build/plugin-ui-sdk/artifacts/guest-$index-updated.capnp',
        ).readAsBytesSync(),
      );
      final intents = <UiIntent>[];
      Widget render(UiDocument d) => MaterialApp(
        home: Scaffold(
          body: PluginForm(
            document: d,
            viewIdentity: 'guest-$index',
            onIntent: intents.add,
          ),
        ),
      );
      await tester.pumpWidget(render(initial));
      expect(find.text('插件表单'), findsOneWidget);
      expect(find.text('灵感🌈'), findsOneWidget);
      await tester.pumpWidget(render(updated));
      await tester.pump();
      expect(find.text('从 Dart 编辑🌈'), findsOneWidget);
      expect(intents, isEmpty);
      await tester.tap(find.byType(FilledButton));
      expect(intents.single.action, 'apply');
      expect(tester.takeException(), isNull);
    });
  }
}
