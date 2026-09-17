import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:morrow_plugin_ui/online.dart';
import 'online_test.dart' as fixture;

Widget localizedHost(String locale, PluginUiController controller) =>
    MaterialApp(
      locale: Locale(locale),
      supportedLocales: L10n.supportedLocales,
      localizationsDelegates: [
        L10n.delegate,
        ...AppLocalizations.localizationsDelegates,
      ],
      home: Scaffold(body: ManagedPluginForm(controller: controller)),
    );

Future<void> openLocalized(
  WidgetTester tester,
  PluginUiController controller,
  fixture.FakeTransport transport,
) async {
  await tester.pumpWidget(localizedHost('zh', controller));
  await tester.pumpAndSettle();
  final opening = controller.open('原始');
  transport.opened.complete(fixture.reply('原始'));
  await opening;
  await tester.pumpAndSettle();
}

TextEditingValue editingValue(WidgetTester tester) =>
    tester.widget<EditableText>(find.byType(EditableText)).controller.value;

void main() {
  testWidgets(
    'Locale changes preserve active view, pending event bytes, queued draft and IME composition',
    (tester) async {
      final transport = fixture.FakeTransport();
      final controller = PluginUiController(transport);
      addTearDown(controller.dispose);
      await openLocalized(tester, controller, transport);
      await tester.enterText(find.byType(TextField), 'first');
      await tester.pump();
      await tester.enterText(find.byType(TextField), 'queued');
      await tester.pump();
      const composing = TextEditingValue(
        text: '中文组合',
        selection: TextSelection(baseOffset: 1, extentOffset: 3),
        composing: TextRange(start: 0, end: 4),
      );
      tester.testTextInput.updateEditingValue(composing);
      await tester.pump();
      final identity = controller.viewIdentity;
      final revision = controller.revision;
      final serial = controller.serial;
      final bytes = transport.requests.single.encode();
      expect(controller.phase, PluginUiPhase.busy);
      expect(find.text('正在更新预览…'), findsOneWidget);
      await tester.pumpWidget(localizedHost('en', controller));
      await tester.pumpAndSettle();
      expect(find.text('Updating preview…'), findsOneWidget);
      expect(
        find.text('应用'),
        findsOneWidget,
      ); // Guest-authored literal, not host chrome.
      expect(editingValue(tester), composing);
      expect(controller.viewIdentity, identity);
      expect(controller.revision, revision);
      expect(controller.serial, serial);
      expect(
        controller.document!.nodes
            .singleWhere((node) => node.id == 'title')
            .text,
        'queued',
      );
      expect(transport.requests, hasLength(1));
      expect(transport.requests.single.encode(), bytes);
      expect(transport.closeCalls, 0);
      transport.results[0].complete(
        fixture.reply('first', revision: 2, serial: 1),
      );
      await tester.pump();
      expect(transport.requests, hasLength(2));
      expect(transport.requests[1].text, 'queued');
      expect(transport.requests[1].serial, BigInt.two);
      expect(editingValue(tester), composing);
      await tester.pumpWidget(localizedHost('zh', controller));
      await tester.pumpAndSettle();
      expect(controller.viewIdentity, identity);
      expect(transport.requests, hasLength(2));
      transport.results[1].complete(
        fixture.reply('queued', revision: 3, serial: 2),
      );
      await tester.pumpAndSettle();
      expect(controller.phase, PluginUiPhase.ready);
      expect(controller.revision, BigInt.from(3));
      expect(editingValue(tester), composing);
      expect(tester.testTextInput.hasAnyClients, isTrue);
      expect(
        tester
            .widget<EditableText>(find.byType(EditableText))
            .focusNode
            .hasFocus,
        isTrue,
      );
      tester.testTextInput.updateEditingValue(
        composing.copyWith(composing: TextRange.empty),
      );
      await tester.pump();
      expect(editingValue(tester).composing, TextRange.empty);
      expect(transport.requests, hasLength(3));
      expect(transport.requests[2].text, '中文组合');
      transport.results[2].complete(
        fixture.reply('中文组合', revision: 4, serial: 3),
      );
      await tester.pump();
      expect(controller.phase, PluginUiPhase.ready);
      expect(controller.viewIdentity, identity);
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets(
    'Tagged host failures translate at display time without replacing the original failure',
    (tester) async {
      const expected = {
        PluginUiHostMessage.inputTooLong: (
          '输入内容已超过此界面的容量，请缩短后重试。',
          'This view has reached its input limit. Shorten the text and try again.',
        ),
        PluginUiHostMessage.connectionLost: (
          '连接已中断，请重新打开此插件界面。',
          'Connection interrupted. Reopen this plugin view.',
        ),
        PluginUiHostMessage.rejected: (
          '插件操作未被接受，请检查输入与当前权限。',
          'The plugin action was not accepted. Check the input and current permissions.',
        ),
        PluginUiHostMessage.execution: (
          '插件执行未完成，请重新打开界面后重试。',
          'Plugin execution did not finish. Reopen the view and try again.',
        ),
        PluginUiHostMessage.unavailable: (
          '插件已不可用，请检查状态并重新打开界面。',
          'The plugin is unavailable. Check its status and reopen the view.',
        ),
      };
      for (final code in PluginUiHostMessage.values) {
        final transport = fixture.FakeTransport();
        final controller = PluginUiController(transport);
        await openLocalized(tester, controller, transport);
        final identity = controller.viewIdentity;
        controller.submit(fixture.edit);
        final failure = PluginUiFailure(
          PluginUiFailureKind.rejected,
          '原始诊断不可覆盖',
          hostMessage: code,
        );
        transport.results.single.complete(
          PluginUiReply(
            view: 'view',
            generation: BigInt.from(7),
            revision: BigInt.one,
            serial: BigInt.one,
            failure: failure,
          ),
        );
        await tester.pump();
        final original = controller.failure;
        expect(find.text(expected[code]!.$1), findsOneWidget);
        await tester.pumpWidget(localizedHost('en', controller));
        await tester.pumpAndSettle();
        expect(find.text(expected[code]!.$2), findsOneWidget);
        expect(find.text('原始诊断不可覆盖'), findsNothing);
        expect(identical(controller.failure, original), isTrue);
        expect(controller.failure!.message, '原始诊断不可覆盖');
        expect(controller.viewIdentity, identity);
        expect(transport.requests, hasLength(1));
        expect(controller.revision, BigInt.one);
        expect(controller.serial, BigInt.one);
        controller.dispose();
        await tester.pumpWidget(const SizedBox());
      }
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets(
    'Plugin failure matching a host phrase remains verbatim across locales',
    (tester) async {
      final transport = fixture.FakeTransport();
      final controller = PluginUiController(transport);
      addTearDown(controller.dispose);
      await openLocalized(tester, controller, transport);
      controller.submit(fixture.edit);
      const literal = '连接已中断，请重新打开此插件界面。';
      transport.results.single.complete(
        PluginUiReply(
          view: 'view',
          generation: BigInt.from(7),
          revision: BigInt.one,
          serial: BigInt.one,
          failure: const PluginUiFailure(PluginUiFailureKind.plugin, literal),
        ),
      );
      await tester.pump();
      await tester.pumpWidget(localizedHost('en', controller));
      await tester.pumpAndSettle();
      expect(find.text(literal), findsOneWidget);
      expect(
        find.text('Connection interrupted. Reopen this plugin view.'),
        findsNothing,
      );
      expect(find.text('应用'), findsOneWidget);
      expect(controller.failure!.hostMessage, isNull);
      expect(controller.phase, PluginUiPhase.ready);
      expect(transport.requests, hasLength(1));
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets(
    'Unknown transport outcome translates but locale changes cannot restart delivery',
    (tester) async {
      final transport = fixture.FakeTransport();
      final controller = PluginUiController(transport);
      addTearDown(controller.dispose);
      await openLocalized(tester, controller, transport);
      final identity = controller.viewIdentity;
      controller.submit(fixture.edit);
      transport.results.single.completeError(
        StateError('private transport diagnostic'),
      );
      await tester.pump();
      expect(controller.phase, PluginUiPhase.interrupted);
      expect(find.text('连接已中断，请重新打开此插件界面。'), findsOneWidget);
      await tester.pumpWidget(localizedHost('en', controller));
      await tester.pumpAndSettle();
      expect(
        find.text('Connection interrupted. Reopen this plugin view.'),
        findsOneWidget,
      );
      expect(controller.viewIdentity, identity);
      expect(controller.submit(fixture.edit), PluginUiAdmission.closed);
      expect(transport.requests, hasLength(1));
      expect(controller.revision, BigInt.one);
      expect(controller.serial, BigInt.zero);
      expect(fixture.field(tester), '新标题');
      expect(tester.takeException(), isNull);
    },
  );
}
