import 'dart:async';
import 'dart:typed_data';
import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_core_client/ui.dart' as core;
import 'package:morrow_core_client/src/generated/ui.capnp.dart' as wire;
import 'package:morrow_core_client/src/generated/contract_identity.dart'
    as contract;
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:morrow_plugin_ui/online.dart';
import 'package:morrow_studio/plugins/plugin_tools.dart';
import 'package:morrow_studio/plugins/workbench_tool_labels.dart';

core.UiDocumentModel fixture(String value, {String? output, String? count}) =>
    core.UiDocumentModel([
      core.UiNode(id: 'root', parent: '', kind: core.Kind.column),
      core.UiNode(
        id: 'heading',
        parent: 'root',
        kind: core.Kind.text,
        text: '文字小工具',
        tone: core.Tone.emphasis,
      ),
      core.UiNode(
        id: 'text',
        parent: 'root',
        kind: core.Kind.textInput,
        label: '输入文字',
        text: value,
        action: 'text.edit',
        maxBytes: 1024,
      ),
      core.UiNode(
        id: 'count',
        parent: 'root',
        kind: core.Kind.text,
        text: count ?? '${value.runes.length} 个字符 · 仅本次使用，不保存为卡片',
        tone: core.Tone.muted,
      ),
      core.UiNode(
        id: 'preview',
        parent: 'root',
        kind: core.Kind.text,
        text: output ?? (value.isEmpty ? '输入后查看大写转换' : value.toUpperCase()),
      ),
    ]);
core.UiNode changed(
  core.UiNode n, {
  String? id,
  String? action,
  String? text,
  int? maxBytes,
  core.Tone? tone,
}) => core.UiNode(
  id: id ?? n.id,
  parent: n.parent,
  kind: n.kind,
  label: n.label,
  text: text ?? n.text,
  action: action ?? n.action,
  enabled: n.enabled,
  checked: n.checked,
  maxBytes: maxBytes ?? n.maxBytes,
  tone: tone ?? n.tone,
);
Uint8List encode(core.UiDocumentModel document) {
  final message = MessageBuilder();
  final root = message.initRoot(wire.documentFactory);
  root.version = contract.uiProtocolVersion;
  root.schemaDigest = Uint8List.fromList(contract.uiDigest);
  final nodes = root.initNodes(document.nodes.length);
  for (var i = 0; i < nodes.length; i++) {
    final source = document.nodes[i], target = nodes[i];
    target.id = source.id;
    target.parent = source.parent;
    target.kind = wire.Kind.values[source.kind.index];
    target.label = source.label;
    target.text = source.text;
    target.action = source.action;
    target.enabled = source.enabled;
    target.checked = source.checked;
    target.maxBytes = source.maxBytes;
    target.tone = wire.Tone.values[source.tone.index];
  }
  return message.serialize();
}

Widget host(String locale, Widget child) => MaterialApp(
  locale: Locale(locale),
  supportedLocales: L10n.supportedLocales,
  localizationsDelegates: const [
    L10n.delegate,
    ...GlobalMaterialLocalizations.delegates,
  ],
  home: Scaffold(body: child),
);

class ToolTransport implements PluginUiTransport {
  final events = <core.UiEvent>[];
  final replies = <Completer<PluginUiReply>>[];
  @override
  Future<PluginUiReply> open(String seed) async => response(seed, 1, 0);
  PluginUiReply response(String value, int revision, int serial) =>
      PluginUiReply(
        view: 'tool',
        generation: BigInt.from(7),
        revision: BigInt.from(revision),
        serial: BigInt.from(serial),
        documentBytes: encode(fixture(value)),
      );
  @override
  Future<PluginUiReply> event(Uint8List bytes) {
    events.add(core.UiEvent.decode(bytes));
    final result = Completer<PluginUiReply>();
    replies.add(result);
    return result.future;
  }

  @override
  Future<void> close() async {}
}

class ToolBackend implements WorkbenchPluginControl {
  final transport = ToolTransport();
  @override
  Future<PluginManagementState> pluginState() async => PluginManagementState(
    revision: BigInt.one,
    digest: Uint8List(32),
    enabled: true,
    approved: true,
    available: true,
    writable: true,
  );
  @override
  Future<PluginManagementState> configurePlugin(
    PluginManagementState expected,
    bool enable,
  ) => throw UnsupportedError('No configuration expected');
  @override
  PluginUiTransport createPluginUi() => transport;
}

void main() {
  testWidgets(
    'Known built-in display localizes empty hints and Unicode counts without changing source nodes',
    (tester) async {
      for (final value in ['', '😀', 'ab', '输入后查看大写转换']) {
        final source = fixture(value);
        final bytes = encode(source);
        late core.UiDocumentModel translated;
        await tester.pumpWidget(
          host(
            'en',
            Builder(
              builder: (context) {
                translated = localizeWorkbenchToolDocument(context, source);
                return PluginForm(
                  document: translated,
                  viewIdentity: 'fixture',
                  onIntent: (_) {},
                );
              },
            ),
          ),
        );
        await tester.pumpAndSettle();
        expect(translated.nodes[1].text, 'Text tools');
        expect(translated.nodes[2].label, 'Enter text');
        expect(translated.nodes[2].text, value);
        expect(
          translated.nodes[3].text,
          '${value.runes.length} ${value.runes.length == 1 ? 'character' : 'characters'} · This session only; not saved as a card',
        );
        expect(
          translated.nodes[4].text,
          value.isEmpty
              ? 'Enter text to preview uppercase'
              : source.nodes[4].text,
        );
        expect(source.nodes[1].text, '文字小工具');
        expect(encode(source), bytes);
        for (var i = 0; i < source.nodes.length; i++) {
          final a = source.nodes[i], b = translated.nodes[i];
          expect(
            (
              a.id,
              a.parent,
              a.kind,
              a.action,
              a.maxBytes,
              a.enabled,
              a.checked,
              a.tone,
            ),
            (
              b.id,
              b.parent,
              b.kind,
              b.action,
              b.maxBytes,
              b.enabled,
              b.checked,
              b.tone,
            ),
          );
        }
      }
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets(
    'Unknown layouts, actions and count templates pass through unchanged',
    (tester) async {
      final original = fixture('ab');
      final variants = <core.UiDocumentModel>[];
      for (final (index, node) in [
        (1, changed(original.nodes[1], text: '其他工具')),
        (1, changed(original.nodes[1], tone: core.Tone.normal)),
        (2, changed(original.nodes[2], action: 'other.edit')),
        (2, changed(original.nodes[2], maxBytes: 2048)),
        (3, changed(original.nodes[3], text: '02 个字符 · 仅本次使用，不保存为卡片')),
        (3, changed(original.nodes[3], text: '1025 个字符 · 仅本次使用，不保存为卡片')),
        (3, changed(original.nodes[3], text: '2 个字')),
        (4, changed(original.nodes[4], id: 'other-preview')),
      ]) {
        variants.add(
          core.UiDocumentModel([
            for (var i = 0; i < original.nodes.length; i++)
              i == index ? node : original.nodes[i],
          ]),
        );
      }
      variants.add(
        core.UiDocumentModel([
          ...original.nodes,
          core.UiNode(
            id: 'extra',
            parent: 'root',
            kind: core.Kind.text,
            text: '原文',
          ),
        ]),
      );
      for (final variant in variants) {
        await tester.pumpWidget(
          host(
            'en',
            Builder(
              builder: (context) {
                expect(
                  identical(
                    localizeWorkbenchToolDocument(context, variant),
                    variant,
                  ),
                  isTrue,
                );
                return const SizedBox();
              },
            ),
          ),
        );
        await tester.pumpAndSettle();
      }
    },
  );

  testWidgets(
    'PluginTools alone opts in and locale changes preserve pending original input and output',
    (tester) async {
      final backend = ToolBackend();
      final tool = PluginTools(
        backend: backend,
        onChanged: () {},
        ink: Colors.black,
        muted: Colors.grey,
        line: Colors.grey,
        radius: BorderRadius.circular(12),
      );
      await tester.pumpWidget(host('en', tool));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Open text tool'));
      await tester.pumpAndSettle();
      final controller = tester
          .widget<ManagedPluginForm>(find.byType(ManagedPluginForm))
          .controller;
      final identity = controller.viewIdentity;
      expect(find.text('Text tools'), findsOneWidget);
      expect(find.text('Enter text to preview uppercase'), findsOneWidget);
      const value = '输入后查看大写转换';
      await tester.enterText(find.byType(TextField), value);
      await tester.pump();
      expect(backend.transport.events.single.text, value);
      expect(backend.transport.events.single.node, 'text');
      expect(backend.transport.events.single.action, 'text.edit');
      expect(controller.document!.nodes[1].text, '文字小工具');
      expect(controller.document!.nodes[3].text, '0 个字符 · 仅本次使用，不保存为卡片');
      expect(find.text('Enter text to preview uppercase'), findsOneWidget);
      final pendingBytes = backend.transport.events.single.encode();
      await tester.pumpWidget(host('zh', tool));
      await tester.pumpAndSettle();
      expect(controller.viewIdentity, identity);
      expect(backend.transport.events.single.encode(), pendingBytes);
      backend.transport.replies.single.complete(
        backend.transport.response(value, 2, 1),
      );
      await tester.pumpAndSettle();
      await tester.pumpWidget(host('en', tool));
      await tester.pumpAndSettle();
      expect(controller.phase, PluginUiPhase.ready);
      expect(controller.viewIdentity, identity);
      expect(backend.transport.events, hasLength(1));
      expect(controller.document!.nodes.last.text, value);
      expect(find.text('Enter text to preview uppercase'), findsNothing);
      expect(
        find.text(value),
        findsNWidgets(2),
      ); // Input and real conversion output.
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets(
    'Generic external form keeps even a matching built-in layout verbatim by default',
    (tester) async {
      final transport = ToolTransport();
      final controller = PluginUiController(transport);
      addTearDown(controller.dispose);
      await tester.pumpWidget(
        host('en', ManagedPluginForm(controller: controller)),
      );
      await controller.open('');
      await tester.pumpAndSettle();
      expect(find.text('文字小工具'), findsOneWidget);
      expect(find.text('输入后查看大写转换'), findsOneWidget);
      expect(find.text('Text tools'), findsNothing);
      expect(controller.document!.nodes[2].label, '输入文字');
      expect(tester.takeException(), isNull);
    },
  );
}
