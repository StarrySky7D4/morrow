import 'dart:typed_data';
import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_core_client/ui.dart' show UiEvent;
import 'package:morrow_core_client/src/generated/ui.capnp.dart' as wire;
import 'package:morrow_core_client/src/generated/contract_identity.dart'
    as contract;
import 'package:morrow_plugin_ui/online.dart';
import 'package:morrow_studio/plugins/plugin_library.dart';

const transform = PluginTransformHandler(
  name: 'bytes.reverse',
  inputType: 'bytes',
  outputType: 'bytes',
  maxInputBytes: 65536,
  maxOutputBytes: 65536,
);
const uiHandlers = [
  PluginTransformHandler(
    name: 'ui.form',
    inputType: 'text.utf8',
    outputType: 'morrow.ui.document.v1',
    maxInputBytes: 32,
    maxOutputBytes: 65536,
  ),
  PluginTransformHandler(
    name: 'ui.edit',
    inputType: 'morrow.ui.event.v1',
    outputType: 'morrow.ui.document.v1',
    maxInputBytes: 65536,
    maxOutputBytes: 65536,
  ),
];
PluginLibraryEntry entry(
  String id, {
  bool enabled = false,
  bool builtin = false,
  List<String> approved = const [],
  List<PluginTransformHandler> handlers = const [transform],
}) => PluginLibraryEntry(
  id: id,
  name: '插件 $id',
  version: '1.0.0',
  digest: Uint8List.fromList(List.filled(32, id.codeUnitAt(0))),
  enabled: enabled,
  builtin: builtin,
  available: true,
  declared: const ['read-content', 'edit-content'],
  approved: approved,
  dependencies: const [],
  handlers: handlers,
  issue: '',
);

Uint8List document(String text) {
  final message = MessageBuilder();
  final root = message.initRoot(wire.documentFactory);
  root.version = contract.uiProtocolVersion;
  root.schemaDigest = Uint8List.fromList(contract.uiDigest);
  final nodes = root.initNodes(2);
  nodes[0].id = 'root';
  nodes[0].kind = wire.Kind.column;
  nodes[0].enabled = true;
  nodes[1].id = 'title';
  nodes[1].parent = 'root';
  nodes[1].kind = wire.Kind.textInput;
  nodes[1].label = '表单标题';
  nodes[1].text = text;
  nodes[1].maxBytes = 32;
  nodes[1].action = 'title.edit';
  nodes[1].enabled = true;
  return message.serialize();
}

class FakeUi implements PluginUiTransport {
  FakeUi(this.log);
  final List<String> log;
  int closes = 0, closeFailures = 0;
  final events = <UiEvent>[];
  PluginUiReply reply(String text, int revision, int serial) => PluginUiReply(
    view: 'view',
    generation: BigInt.one,
    revision: BigInt.from(revision),
    serial: BigInt.from(serial),
    documentBytes: document(text),
  );
  @override
  Future<PluginUiReply> open(String seed) async {
    log.add('open');
    return reply(seed, 1, 0);
  }

  @override
  Future<PluginUiReply> event(Uint8List bytes) async {
    final event = UiEvent.decode(bytes);
    events.add(event);
    return reply(
      event.text.toUpperCase(),
      event.revision.toInt() + 1,
      event.serial.toInt(),
    );
  }

  @override
  Future<void> close() async {
    closes++;
    log.add('close');
    if (closeFailures-- > 0) throw StateError('close failed');
  }
}

class FakeBackend implements ExternalPluginControl {
  FakeBackend(this.entries);
  List<PluginLibraryEntry> entries;
  BigInt revision = BigInt.one;
  final pages = <(String, BigInt?)>[];
  final log = <String>[];
  final views = <FakeUi>[];
  int inspections = 0,
      imports = 0,
      configurations = 0,
      removals = 0,
      transforms = 0;
  bool conflict = false;
  List<String>? lastApproved;
  Uint8List? lastInput;
  @override
  Future<PluginLibraryPage> pluginPage({
    String cursor = '',
    BigInt? revision,
  }) async {
    pages.add((cursor, revision));
    if (revision != null && revision != this.revision) {
      throw StateError('changed');
    }
    final start = cursor.isEmpty ? 0 : int.parse(cursor);
    final end = (start + 2).clamp(0, entries.length).toInt();
    return PluginLibraryPage(
      revision: this.revision,
      entries: entries.sublist(start, end),
      cursor: end < entries.length ? '$end' : '',
    );
  }

  @override
  Future<PluginLibraryPage> inspectPlugin(String path) async {
    inspections++;
    return PluginLibraryPage(
      revision: revision,
      entries: [entry('new')],
      cursor: '',
    );
  }

  @override
  Future<void> importPlugin(
    String path,
    Uint8List digest,
    BigInt revision,
  ) async {
    imports++;
    expect(revision, this.revision);
    expect(digest, entry('new').digest);
    entries.add(entry('new'));
    this.revision += BigInt.one;
  }

  @override
  Future<void> configureExternal(
    PluginLibraryEntry value,
    BigInt revision,
    List<String> approved,
    bool enable,
  ) async {
    configurations++;
    log.add('configure');
    lastApproved = List.of(approved);
    expect(revision, this.revision);
    this.revision += BigInt.one;
    if (conflict) throw StateError('revision conflict');
    entries = [
      for (final item in entries)
        if (item.id == value.id)
          entry(
            item.id,
            enabled: enable,
            approved: approved,
            handlers: item.handlers,
          )
        else
          item,
    ];
  }

  @override
  Future<void> removeExternal(PluginLibraryEntry entry, BigInt revision) async {
    removals++;
    log.add('remove');
    entries.removeWhere((item) => item.id == entry.id);
    this.revision += BigInt.one;
  }

  @override
  Future<Uint8List> transformExternal(
    PluginLibraryEntry entry,
    BigInt revision,
    PluginTransformHandler handler,
    Uint8List input,
  ) async {
    transforms++;
    lastInput = Uint8List.fromList(input);
    return Uint8List.fromList([65, 255]);
  }

  @override
  PluginUiTransport createExternalPluginUi(
    PluginLibraryEntry entry,
    BigInt revision,
  ) {
    final view = FakeUi(log);
    views.add(view);
    return view;
  }
}

Widget page(FakeBackend backend, {Future<String?> Function()? picker}) =>
    MaterialApp(
      home: Scaffold(
        body: SingleChildScrollView(
          child: PluginLibrary(
            backend: backend,
            onChanged: () {},
            ink: Colors.black,
            muted: Colors.grey,
            line: Colors.grey,
            radius: BorderRadius.circular(12),
            pickPackage: picker,
          ),
        ),
      ),
    );
Future<void> click(WidgetTester tester, String key) async {
  final finder = find.byKey(ValueKey(key));
  await tester.ensureVisible(finder);
  await tester.pumpAndSettle();
  await tester.tap(finder);
  await tester.pumpAndSettle();
}

Future<void> mount(
  WidgetTester tester,
  FakeBackend backend, {
  Future<String?> Function()? picker,
}) async {
  await tester.pumpWidget(page(backend, picker: picker));
  await tester.pumpAndSettle();
}

void main() {
  testWidgets(
    'all pages retain the first revision and builtin remains separately managed',
    (tester) async {
      final backend = FakeBackend([
        entry('builtin', builtin: true),
        entry('a'),
        entry('b'),
      ]);
      await mount(tester, backend);
      expect(backend.pages, [('', null), ('2', BigInt.one)]);
      expect(find.text('插件 b'), findsOneWidget);
      await click(tester, 'plugin-entry-builtin');
      expect(
        find.byKey(const ValueKey('plugin-approve-builtin')),
        findsNothing,
      );
      expect(find.text('请使用上方工作台插件按钮管理此插件。'), findsOneWidget);
    },
  );

  testWidgets(
    'cancelled picker and cancelled preview never import; explicit import stays disabled',
    (tester) async {
      final backend = FakeBackend([]);
      var cancelPicker = true;
      await mount(
        tester,
        backend,
        picker: () async => cancelPicker ? null : 'selected.mplugin',
      );
      await click(tester, 'plugin-pick');
      expect(backend.inspections, 0);
      expect(backend.imports, 0);
      cancelPicker = false;
      await click(tester, 'plugin-pick');
      expect(find.byKey(const ValueKey('plugin-preview')), findsOneWidget);
      expect(backend.imports, 0);
      await click(tester, 'plugin-cancel-import');
      expect(backend.imports, 0);
      await click(tester, 'plugin-pick');
      await click(tester, 'plugin-import');
      expect(backend.imports, 1);
      expect(backend.configurations, 0);
      expect(backend.entries.single.enabled, isFalse);
      expect(find.text('已导入，尚未启用。请选择需要允许的权限。'), findsOneWidget);
    },
  );

  testWidgets('checkboxes grant nothing until explicit approval and enable', (
    tester,
  ) async {
    final backend = FakeBackend([entry('a')]);
    await mount(tester, backend);
    await click(tester, 'plugin-entry-a');
    await click(tester, 'plugin-cap-a-read-content');
    expect(backend.configurations, 0);
    await click(tester, 'plugin-approve-a');
    expect(backend.configurations, 1);
    expect(backend.lastApproved, ['read-content']);
    expect(backend.entries.single.enabled, isTrue);
  });

  testWidgets(
    'conflicted mutation refreshes state but never automatically repeats write',
    (tester) async {
      final backend = FakeBackend([entry('a')])..conflict = true;
      await mount(tester, backend);
      await click(tester, 'plugin-entry-a');
      await click(tester, 'plugin-approve-a');
      expect(backend.configurations, 1);
      expect(backend.pages.length, 2);
      expect(find.textContaining('操作未确认'), findsOneWidget);
      await tester.pump(const Duration(seconds: 1));
      expect(backend.configurations, 1);
      backend.conflict = false;
      await click(tester, 'plugin-approve-a');
      expect(backend.configurations, 2);
      expect(backend.entries.single.enabled, isTrue);
    },
  );

  testWidgets(
    'actual text input sends selected transform and binary result remains readable',
    (tester) async {
      final backend = FakeBackend([entry('a', enabled: true)]);
      await mount(tester, backend);
      await click(tester, 'plugin-entry-a');
      await click(tester, 'plugin-transform-a');
      await tester.enterText(find.byKey(const ValueKey('plugin-input')), 'abc');
      await click(tester, 'plugin-run');
      expect(backend.transforms, 1);
      expect(backend.lastInput, [97, 98, 99]);
      expect(find.textContaining('二进制内容：41 ff'), findsOneWidget);
      expect(backend.configurations, 0);
      expect(backend.imports, 0);
    },
  );

  testWidgets(
    'managed form sends encoded events and disable closes before policy mutation',
    (tester) async {
      final backend = FakeBackend([
        entry('a', enabled: true, handlers: uiHandlers),
      ]);
      await mount(tester, backend);
      await click(tester, 'plugin-entry-a');
      await click(tester, 'plugin-ui-a');
      expect(find.byType(ManagedPluginForm), findsOneWidget);
      final field = find.descendant(
        of: find.byType(ManagedPluginForm),
        matching: find.byType(TextField),
      );
      await tester.enterText(field, 'abc');
      await tester.pumpAndSettle();
      expect(backend.views.single.events.single.text, 'abc');
      expect(tester.widget<TextField>(field).controller!.text, 'ABC');
      await click(tester, 'plugin-disable-a');
      expect(backend.log, ['open', 'close', 'configure']);
      expect(backend.views.single.closes, 1);
      expect(find.byType(ManagedPluginForm), findsNothing);
    },
  );

  testWidgets(
    'only one external form remains open and uninstall preserves explicit close ordering',
    (tester) async {
      final backend = FakeBackend([
        entry('a', enabled: true, handlers: uiHandlers),
        entry('b', enabled: true, handlers: uiHandlers),
      ]);
      await mount(tester, backend);
      await click(tester, 'plugin-entry-a');
      await click(tester, 'plugin-ui-a');
      await click(tester, 'plugin-entry-b');
      await click(tester, 'plugin-ui-b');
      expect(backend.views.length, 2);
      expect(backend.views.first.closes, 1);
      expect(find.byType(ManagedPluginForm), findsOneWidget);
      await click(tester, 'plugin-remove-b');
      expect(backend.views.last.closes, 1);
      expect(backend.removals, 1);
      expect(backend.log.sublist(backend.log.length - 2), ['close', 'remove']);
      expect(find.text('已卸载，已有内容仍保留。'), findsOneWidget);
    },
  );

  testWidgets(
    'close failure is visible and blocks mutation until an explicit refresh retries close',
    (tester) async {
      final backend = FakeBackend([
        entry('a', enabled: true, handlers: uiHandlers),
      ]);
      await mount(tester, backend);
      await click(tester, 'plugin-entry-a');
      await click(tester, 'plugin-ui-a');
      backend.views.single.closeFailures = 1;
      await click(tester, 'plugin-disable-a');
      expect(backend.configurations, 0);
      expect(backend.views.single.closes, 1);
      expect(find.textContaining('操作未确认'), findsOneWidget);
      await click(tester, 'plugin-refresh');
      expect(backend.views.single.closes, 2);
      await click(tester, 'plugin-disable-a');
      expect(backend.configurations, 1);
    },
  );
}
