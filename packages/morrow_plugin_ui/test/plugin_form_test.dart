import 'dart:io';
import 'dart:ui' as ui;
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:morrow_core_client/ui.dart' show UiDocument, UiEvent;
import 'package:morrow_core_client/src/generated/ui.capnp.dart' as wire;
import 'package:morrow_core_client/src/generated/contract_identity.dart'
    as contract;
import 'package:morrow_plugin_ui/morrow_plugin_ui.dart';

final fixture = File('test/fixtures/form.capnp').readAsBytesSync();
UiDocument sample({
  String text = '',
  int maxBytes = 32,
  bool enabled = true,
  bool row = false,
}) {
  final m = MessageBuilder();
  final r = m.initRoot(wire.documentFactory);
  r.version = contract.uiProtocolVersion;
  r.schemaDigest = Uint8List.fromList(contract.uiDigest);
  final nodes = r.initNodes(row ? 4 : 2);
  nodes[0].id = 'root';
  nodes[0].kind = wire.Kind.column;
  if (row) {
    nodes[1].id = 'row';
    nodes[1].parent = 'root';
    nodes[1].kind = wire.Kind.row;
  }
  for (var i = row ? 2 : 1; i < (row ? 4 : 2); i++) {
    nodes[i].id = 'field$i';
    nodes[i].parent = row ? 'row' : 'root';
    nodes[i].kind = wire.Kind.textInput;
    nodes[i].label = '标题';
    nodes[i].text = text;
    nodes[i].maxBytes = maxBytes;
    nodes[i].action = 'edit$i';
    nodes[i].enabled = enabled;
  }
  return UiDocument.decode(m.serialize());
}

ThemeData theme({bool dark = false, Color color = Colors.teal}) => ThemeData(
  useMaterial3: true,
  brightness: dark ? Brightness.dark : Brightness.light,
  fontFamily: 'MorrowTest',
  fontFamilyFallback: const ['MorrowEmoji'],
  colorScheme: ColorScheme.fromSeed(
    seedColor: color,
    brightness: dark ? Brightness.dark : Brightness.light,
  ),
  inputDecorationTheme: InputDecorationTheme(
    filled: true,
    border: OutlineInputBorder(borderRadius: BorderRadius.circular(13)),
  ),
  filledButtonTheme: FilledButtonThemeData(
    style: FilledButton.styleFrom(
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(12)),
    ),
  ),
);
Widget view(
  UiDocument doc,
  ValueChanged<UiIntent> callback, {
  Object identity = 'view',
  ThemeData? style,
  double scale = 1,
}) => RepaintBoundary(
  key: const ValueKey('capture'),
  child: MaterialApp(
    debugShowCheckedModeBanner: false,
    theme: style ?? theme(),
    home: Scaffold(
      body: MediaQuery(
        data: MediaQueryData(textScaler: TextScaler.linear(scale)),
        child: Padding(
          padding: const EdgeInsets.all(24),
          child: PluginForm(
            document: doc,
            viewIdentity: identity,
            onIntent: callback,
          ),
        ),
      ),
    ),
  ),
);
void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  setUpAll(() async {
    for (final entry in {
      'MorrowTest':
          Platform.environment['MORROW_UI_TEST_FONT'] ??
          'C:/Windows/Fonts/msyh.ttc',
      'MorrowEmoji':
          Platform.environment['MORROW_UI_TEST_EMOJI_FONT'] ??
          'C:/Windows/Fonts/seguiemj.ttf',
    }.entries) {
      final font = File(entry.value);
      if (!font.existsSync()) {
        throw StateError('Missing test font: ${font.path}');
      }
      final loader = FontLoader(entry.key)
        ..addFont(font.readAsBytes().then((b) => ByteData.sublistView(b)));
      await loader.load();
    }
  });
  testWidgets('Rust form renders controls and emits local typed intents', (
    tester,
  ) async {
    final intents = <UiIntent>[];
    await tester.pumpWidget(view(UiDocument.decode(fixture), intents.add));
    expect(find.text('插件表单'), findsOneWidget);
    expect(find.byType(TextField), findsOneWidget);
    expect(find.byType(SwitchListTile), findsOneWidget);
    expect(find.byType(FilledButton), findsOneWidget);
    await tester.enterText(find.byType(TextField), '从 Dart 编辑🌈');
    await tester.pump();
    expect(intents.length, 1);
    expect(intents.single.kind, EventKind.editText);
    expect(intents.single.node, 'title');
    expect(intents.single.action, 'title.edit');
    final event = UiEvent(
      view: 'view',
      generation: (BigInt.one << 64) - BigInt.one,
      revision: BigInt.one,
      serial: BigInt.one,
      node: intents.single.node,
      action: intents.single.action,
      kind: intents.single.kind,
      text: intents.single.text,
    );
    final file = File('../../build/ui-protocol/widget-event.capnp');
    file.parent.createSync(recursive: true);
    file.writeAsBytesSync(event.encode());
    await tester.tap(find.byType(SwitchListTile));
    await tester.pumpAndSettle();
    expect(intents.last.kind, EventKind.setToggle);
    expect(intents.last.checked, isTrue);
    await tester.tap(find.byType(FilledButton));
    expect(intents.last.kind, EventKind.activate);
    expect(intents.last.text, isEmpty);
    expect(tester.takeException(), isNull);
  });
  testWidgets(
    'rows wrap at narrow widths without overflow at large text scale',
    (tester) async {
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetDevicePixelRatio);
      addTearDown(tester.view.resetPhysicalSize);
      tester.view.physicalSize = const Size(900, 700);
      await tester.pumpWidget(view(sample(row: true), (_) {}, scale: 2));
      await tester.pumpAndSettle();
      var fields = find.byType(TextField);
      expect(
        tester.getTopLeft(fields.at(0)).dy,
        tester.getTopLeft(fields.at(1)).dy,
      );
      tester.view.physicalSize = const Size(320, 640);
      await tester.pumpAndSettle();
      fields = find.byType(TextField);
      expect(
        tester.getTopLeft(fields.at(1)).dy,
        greaterThan(tester.getTopLeft(fields.at(0)).dy),
      );
      expect(tester.takeException(), isNull);
    },
  );
  testWidgets(
    'theme and unrelated rebuild preserve draft selection and focus',
    (tester) async {
      final intents = <UiIntent>[];
      await tester.pumpWidget(view(UiDocument.decode(fixture), intents.add));
      await tester.enterText(find.byType(TextField), 'local draft');
      var edit = tester.widget<EditableText>(find.byType(EditableText));
      edit.controller.selection = const TextSelection.collapsed(offset: 3);
      await tester.pumpWidget(
        view(
          UiDocument.decode(fixture),
          intents.add,
          style: theme(dark: true, color: Colors.purple),
        ),
      );
      await tester.pumpAndSettle();
      edit = tester.widget<EditableText>(find.byType(EditableText));
      expect(edit.controller.text, 'local draft');
      expect(edit.controller.selection.extentOffset, 3);
      expect(edit.focusNode.hasFocus, isTrue);
      final text = tester.widget<Text>(find.text('插件表单'));
      expect(
        text.style?.color,
        theme(dark: true, color: Colors.purple).colorScheme.primary,
      );
      expect(intents.length, 1);
    },
  );
  testWidgets(
    'authoritative field update is applied without publishing an edit',
    (tester) async {
      final intents = <UiIntent>[];
      await tester.pumpWidget(view(sample(text: 'old'), intents.add));
      await tester.enterText(find.byType(TextField), 'draft');
      await tester.pumpWidget(view(sample(text: 'updated'), intents.add));
      await tester.pump();
      expect(
        tester.widget<EditableText>(find.byType(EditableText)).controller.text,
        'updated',
      );
      expect(intents.length, 1);
    },
  );
  testWidgets(
    'composition is not sent before commit and UTF8 limit preserves whole text',
    (tester) async {
      final intents = <UiIntent>[];
      await tester.pumpWidget(view(sample(maxBytes: 6), intents.add));
      await tester.showKeyboard(find.byType(TextField));
      tester.testTextInput.updateEditingValue(
        const TextEditingValue(
          text: '中文文',
          selection: TextSelection.collapsed(offset: 3),
          composing: TextRange(start: 0, end: 3),
        ),
      );
      await tester.pump();
      expect(intents, isEmpty);
      tester.testTextInput.updateEditingValue(
        const TextEditingValue(
          text: '中文文',
          selection: TextSelection.collapsed(offset: 3),
        ),
      );
      await tester.pump();
      expect(
        tester.widget<EditableText>(find.byType(EditableText)).controller.text,
        isEmpty,
      );
      expect(intents, isEmpty);
      tester.testTextInput.updateEditingValue(
        const TextEditingValue(
          text: '中文',
          selection: TextSelection.collapsed(offset: 2),
          composing: TextRange(start: 0, end: 2),
        ),
      );
      await tester.pump();
      expect(intents, isEmpty);
      tester.testTextInput.updateEditingValue(
        const TextEditingValue(
          text: '中文',
          selection: TextSelection.collapsed(offset: 2),
        ),
      );
      await tester.pump();
      expect(intents.single.text, '中文');
      await tester.enterText(find.byType(TextField), 'bad\u0000');
      await tester.pump();
      expect(
        tester.widget<EditableText>(find.byType(EditableText)).controller.text,
        '中文',
      );
      expect(intents.length, 1);
    },
  );
  testWidgets('disabled removed and previous-generation actions cannot emit', (
    tester,
  ) async {
    final intents = <UiIntent>[];
    await tester.pumpWidget(view(UiDocument.decode(fixture), intents.add));
    final stale = tester
        .widget<FilledButton>(find.byType(FilledButton))
        .onPressed!;
    await tester.pumpWidget(
      view(UiDocument.decode(fixture), intents.add, identity: 'new-view'),
    );
    stale();
    expect(intents, isEmpty);
    final removed = tester
        .widget<FilledButton>(find.byType(FilledButton))
        .onPressed!;
    await tester.pumpWidget(
      view(sample(enabled: false), intents.add, identity: 'new-view'),
    );
    removed();
    expect(intents, isEmpty);
    expect(tester.widget<TextField>(find.byType(TextField)).enabled, isFalse);
    await tester.pumpWidget(const SizedBox.shrink());
    stale();
    removed();
    expect(intents, isEmpty);
    expect(tester.takeException(), isNull);
  });
  testWidgets(
    'view replacement resets local draft and exposes control semantics',
    (tester) async {
      final semantics = tester.ensureSemantics();
      try {
        await tester.pumpWidget(view(UiDocument.decode(fixture), (_) {}));
        await tester.enterText(find.byType(TextField), 'unsaved');
        await tester.pumpWidget(
          view(UiDocument.decode(fixture), (_) {}, identity: 'generation-2'),
        );
        await tester.pump();
        expect(
          tester
              .widget<EditableText>(find.byType(EditableText))
              .controller
              .text,
          '灵感🌈',
        );
        expect(find.bySemanticsLabel('应用'), findsOneWidget);
        expect(find.bySemanticsLabel(RegExp('置顶')), findsWidgets);
      } finally {
        semantics.dispose();
      }
    },
  );
  testWidgets('capture native Flutter light narrow and dark wide rendering', (
    tester,
  ) async {
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetDevicePixelRatio);
    addTearDown(tester.view.resetPhysicalSize);
    for (final dark in [false, true]) {
      tester.view.physicalSize = dark
          ? const Size(800, 520)
          : const Size(360, 560);
      await tester.pumpWidget(
        view(UiDocument.decode(fixture), (_) {}, style: theme(dark: dark)),
      );
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);
      final boundary = tester.renderObject<RenderRepaintBoundary>(
        find.byKey(const ValueKey('capture')),
      );
      await tester.runAsync(() async {
        final image = await boundary.toImage(pixelRatio: 1);
        final bytes = await image.toByteData(format: ui.ImageByteFormat.png);
        final file = File(
          '../../build/ui-renderer/${dark ? 'dark-wide' : 'light-narrow'}.png',
        );
        file.parent.createSync(recursive: true);
        file.writeAsBytesSync(bytes!.buffer.asUint8List());
        image.dispose();
      });
    }
  });
}
