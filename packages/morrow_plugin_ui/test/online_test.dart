import 'dart:async';
import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_core_client/ui.dart' show UiEvent;
import 'package:morrow_core_client/src/generated/ui.capnp.dart' as wire;
import 'package:morrow_core_client/src/generated/contract_identity.dart'
    as contract;
import 'package:morrow_plugin_ui/online.dart';

Uint8List document(String text) {
  final message = MessageBuilder();
  final r = message.initRoot(wire.documentFactory);
  r.version = contract.uiProtocolVersion;
  r.schemaDigest = Uint8List.fromList(contract.uiDigest);
  final n = r.initNodes(3);
  n[0].id = 'root';
  n[0].kind = wire.Kind.column;
  n[0].enabled = true;
  n[1].id = 'title';
  n[1].parent = 'root';
  n[1].kind = wire.Kind.textInput;
  n[1].label = '标题';
  n[1].text = text;
  n[1].maxBytes = 32;
  n[1].action = 'title.edit';
  n[1].enabled = true;
  n[2].id = 'apply';
  n[2].parent = 'root';
  n[2].kind = wire.Kind.button;
  n[2].label = '应用';
  n[2].action = 'apply';
  n[2].enabled = true;
  return message.serialize();
}

PluginUiReply reply(
  String text, {
  int revision = 1,
  int serial = 0,
  int generation = 7,
}) => PluginUiReply(
  view: 'view',
  generation: BigInt.from(generation),
  revision: BigInt.from(revision),
  serial: BigInt.from(serial),
  documentBytes: document(text),
);

class FakeTransport implements PluginUiTransport {
  final opened = Completer<PluginUiReply>();
  final requests = <UiEvent>[];
  final results = <Completer<PluginUiReply>>[];
  int closeCalls = 0;
  @override
  Future<PluginUiReply> open(String seed) => opened.future;
  @override
  Future<PluginUiReply> event(Uint8List bytes) {
    requests.add(UiEvent.decode(bytes));
    final c = Completer<PluginUiReply>();
    results.add(c);
    return c.future;
  }

  @override
  Future<void> close() async {
    closeCalls++;
  }
}

const edit = UiIntent(
  node: 'title',
  action: 'title.edit',
  kind: EventKind.editText,
  text: '新标题',
);
Widget host(PluginUiController controller) => MaterialApp(
  home: Scaffold(body: ManagedPluginForm(controller: controller)),
);
Future<void> open(
  WidgetTester tester,
  PluginUiController controller,
  FakeTransport transport, {
  int generation = 7,
}) async {
  await tester.pumpWidget(host(controller));
  final opening = controller.open('初始');
  transport.opened.complete(reply('初始', generation: generation));
  await opening;
  await tester.pump();
}

String field(WidgetTester tester) =>
    tester.widget<EditableText>(find.byType(EditableText)).controller.text;
void main() {
  testWidgets(
    'real PluginForm input emits encoded events and renders async decoded replies',
    (tester) async {
      final t = FakeTransport();
      final c = PluginUiController(t);
      addTearDown(c.dispose);
      await open(tester, c, t);
      await tester.enterText(find.byType(TextField), '第一');
      await tester.pump();
      expect(t.requests.single.text, '第一');
      expect(t.requests.single.serial, BigInt.one);
      expect(t.requests.single.revision, BigInt.one);
      expect(
        tester.widget<FilledButton>(find.byType(FilledButton)).onPressed,
        isNull,
      );
      await tester.enterText(find.byType(TextField), '第二');
      await tester.pump();
      await tester.enterText(find.byType(TextField), '第三');
      await tester.pump();
      expect(t.requests, hasLength(1));
      expect(field(tester), '第三');
      t.results[0].complete(reply('第一', revision: 2, serial: 1));
      await tester.pump();
      expect(field(tester), '第三');
      expect(t.requests, hasLength(2));
      expect(t.requests[1].text, '第三');
      expect(t.requests[1].serial, BigInt.two);
      expect(t.requests[1].revision, BigInt.two);
      t.results[1].complete(reply('主机校正', revision: 3, serial: 2));
      await tester.pump();
      expect(field(tester), '主机校正');
      expect(c.phase, PluginUiPhase.ready);
      expect(c.revision, BigInt.from(3));
      expect(
        tester.widget<FilledButton>(find.byType(FilledButton)).onPressed,
        isNotNull,
      );
      expect(tester.takeException(), isNull);
    },
  );
  testWidgets('reply preserves IME composition and only committed text sends', (
    tester,
  ) async {
    final t = FakeTransport();
    final c = PluginUiController(t);
    addTearDown(c.dispose);
    await open(tester, c, t);
    await tester.enterText(find.byType(TextField), 'first');
    await tester.pump();
    tester.testTextInput.updateEditingValue(
      const TextEditingValue(
        text: '中文',
        selection: TextSelection.collapsed(offset: 2),
        composing: TextRange(start: 0, end: 2),
      ),
    );
    await tester.pump();
    expect(t.requests, hasLength(1));
    t.results[0].complete(reply('规范标题', revision: 2, serial: 1));
    await tester.pump();
    expect(field(tester), '中文');
    expect(
      tester
          .widget<EditableText>(find.byType(EditableText))
          .controller
          .value
          .composing,
      const TextRange(start: 0, end: 2),
    );
    tester.testTextInput.updateEditingValue(
      const TextEditingValue(
        text: '中文',
        selection: TextSelection.collapsed(offset: 2),
      ),
    );
    await tester.pump();
    expect(t.requests, hasLength(2));
    expect(t.requests[1].text, '中文');
    expect(t.requests[1].revision, BigInt.two);
    t.results[1].complete(reply('中文', revision: 3, serial: 2));
    await tester.pump();
    expect(field(tester), '中文');
  });
  testWidgets(
    'accepted failure keeps document and drafts, clears queued delivery, no retry',
    (tester) async {
      final t = FakeTransport();
      final c = PluginUiController(t);
      addTearDown(c.dispose);
      await open(tester, c, t);
      await tester.enterText(find.byType(TextField), 'failed');
      await tester.pump();
      await tester.enterText(find.byType(TextField), 'unsent');
      await tester.pump();
      t.results[0].complete(
        PluginUiReply(
          view: 'view',
          generation: BigInt.from(7),
          revision: BigInt.one,
          serial: BigInt.one,
          failure: const PluginUiFailure(
            PluginUiFailureKind.plugin,
            '暂时无法应用此输入',
          ),
        ),
      );
      await tester.pump();
      expect(t.requests, hasLength(1));
      expect(c.revision, BigInt.one);
      expect(c.serial, BigInt.one);
      expect(field(tester), 'unsent');
      expect(find.text('暂时无法应用此输入'), findsOneWidget);
      await tester.enterText(find.byType(TextField), 'recovered');
      await tester.pump();
      expect(t.requests[1].serial, BigInt.two);
      expect(t.requests[1].revision, BigInt.one);
      t.results[1].complete(reply('recovered', revision: 2, serial: 2));
      await tester.pump();
      expect(c.failure, isNull);
    },
  );
  testWidgets(
    'controller replacement rejects stale callback and old completion',
    (tester) async {
      final t = FakeTransport();
      final c = PluginUiController(t);
      await open(tester, c, t);
      final oldAction = tester
          .widget<FilledButton>(find.byType(FilledButton))
          .onPressed!;
      c.submit(edit);
      final nextT = FakeTransport();
      final next = PluginUiController(nextT);
      addTearDown(next.dispose);
      c.dispose();
      await open(tester, next, nextT, generation: 8);
      oldAction();
      t.results.single.complete(
        reply('old late result', revision: 2, serial: 1),
      );
      await tester.pump();
      expect(nextT.requests, isEmpty);
      expect(field(tester), '初始');
      expect(t.requests, hasLength(1));
      expect(t.closeCalls, 1);
      expect(tester.takeException(), isNull);
    },
  );
  testWidgets(
    'unknown outcome and malformed/wrong identity reply stop delivery',
    (tester) async {
      for (final failure in [0, 1, 2]) {
        final t = FakeTransport();
        final c = PluginUiController(t);
        await open(tester, c, t);
        c.submit(edit);
        if (failure == 0) {
          t.results.single.completeError(StateError('transport disconnected'));
        } else if (failure == 1) {
          t.results.single.complete(
            reply('wrong', revision: 2, serial: 1, generation: 8),
          );
        } else {
          t.results.single.complete(
            PluginUiReply(
              view: 'view',
              generation: BigInt.from(7),
              revision: BigInt.two,
              serial: BigInt.one,
              documentBytes: Uint8List.fromList([1, 2]),
            ),
          );
        }
        await tester.pump();
        expect(c.phase, PluginUiPhase.interrupted);
        expect(c.revision, BigInt.one);
        expect(c.submit(edit), PluginUiAdmission.closed);
        expect(t.requests, hasLength(1));
        c.dispose();
        await tester.pumpWidget(const SizedBox.shrink());
      }
    },
  );
  testWidgets(
    'stale revision and busy discrete intents never enter transport',
    (tester) async {
      final t = FakeTransport();
      final c = PluginUiController(t);
      addTearDown(c.dispose);
      await open(tester, c, t);
      expect(
        c.submit(edit, expectedRevision: BigInt.zero),
        PluginUiAdmission.rejected,
      );
      expect(c.submit(edit), PluginUiAdmission.sent);
      expect(
        c.submit(
          const UiIntent(
            node: 'apply',
            action: 'apply',
            kind: EventKind.activate,
          ),
        ),
        PluginUiAdmission.busy,
      );
      expect(t.requests, hasLength(1));
      t.results[0].complete(reply('新标题', revision: 2, serial: 1));
      await tester.pump();
    },
  );
}
