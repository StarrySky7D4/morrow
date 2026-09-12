import 'dart:io';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_plugin_ui/online.dart';
import 'package:morrow_studio/plugins/plugin_tools.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';

// These tests require the actual bundled Rust host and workbench Wasm package.
// No replacement transport, generated reply fixture, or in-Dart uppercase logic.
Future<void> pumpHost(WidgetTester tester, bool Function() complete) async {
  final deadline = DateTime.now().add(const Duration(seconds: 30));
  while (!complete()) {
    if (DateTime.now().isAfter(deadline)) {
      fail('The actual plugin host did not finish within 30 seconds');
    }
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 10)),
    );
    await tester.pump();
    final error = tester.takeException();
    if (error != null) {
      throw error;
    }
  }
  await tester.pump();
}

Future<void> closeHostUi(
  WidgetTester tester,
  PluginUiController controller,
) async {
  var finished = false;
  Object? failure;
  controller.close().then(
    (_) => finished = true,
    onError: (Object error) {
      failure = error;
      finished = true;
    },
  );
  await pumpHost(tester, () => finished);
  if (failure != null) throw failure!;
}

Widget form(PluginUiController controller) => MaterialApp(
  home: Scaffold(
    body: SizedBox(
      width: 440,
      height: 500,
      child: ManagedPluginForm(controller: controller),
    ),
  ),
);

void main() {
  final executable = Platform.environment['MORROW_WORKBENCH_HOST'];
  final package = Platform.environment['MORROW_WORKBENCH_PACKAGE'];
  final unavailable =
      !Platform.isWindows || executable == null || package == null;

  testWidgets(
    'actual Rust host transport drives PluginForm, generations and read-only previews',
    (tester) async {
      Directory? directory;
      RustWorkbench? backend;
      PluginUiController? first, second;
      try {
        await tester.runAsync(() async {
          directory = await Directory.systemTemp.createTemp(
            'morrow-native-plugin-form-',
          );
          backend = await RustWorkbench.open(
            executable: executable!,
            package: package!,
            directory: directory!,
            managed: true,
          );
          final state = await backend!.pluginState();
          expect(state.available, isTrue);
          if (!state.enabled) await backend!.configurePlugin(state, true);
          expect(await backend!.load(), isEmpty);
        });
        first = PluginUiController(backend!.createPluginUi());
        await tester.pumpWidget(form(first));
        await tester.runAsync(() => first!.open(''));
        await tester.pump();
        expect(
          first.phase,
          PluginUiPhase.ready,
          reason: first.failure?.message,
        );
        expect(find.text('文字小工具'), findsOneWidget);
        final generation = first.generation!;
        await tester.enterText(find.byType(TextField), 'abc');
        await tester.pump();
        await pumpHost(tester, () => first!.phase != PluginUiPhase.busy);
        expect(
          first.phase,
          PluginUiPhase.ready,
          reason: first.failure?.message,
        );
        expect(first.serial, BigInt.one);
        expect(first.revision, BigInt.two);
        expect(find.text('ABC'), findsOneWidget);
        expect(find.text('3 个字符 · 仅本次使用，不保存为卡片'), findsOneWidget);
        await tester.runAsync(() async {
          expect(await backend!.load(), isEmpty);
        });

        await tester.runAsync(first.close);
        await tester.pumpWidget(const SizedBox.shrink());
        second = PluginUiController(backend!.createPluginUi());
        await tester.pumpWidget(form(second));
        await tester.runAsync(() => second!.open(''));
        await tester.pump();
        expect(
          second.phase,
          PluginUiPhase.ready,
          reason: second.failure?.message,
        );
        expect(second.generation, greaterThan(generation));
        expect(second.revision, BigInt.one);
        expect(second.serial, BigInt.zero);
        expect(find.text('ABC'), findsNothing);
        // Re-closing the old transport must not close the newly opened host view.
        await tester.runAsync(first.close);
        await tester.enterText(find.byType(TextField), 'def');
        await tester.pump();
        await pumpHost(tester, () => second!.phase != PluginUiPhase.busy);
        expect(
          second.phase,
          PluginUiPhase.ready,
          reason: second.failure?.message,
        );
        expect(find.text('DEF'), findsOneWidget);
        await tester.runAsync(() async {
          expect(await backend!.load(), isEmpty);
          await second!.close();
          await backend!.close();
          backend = null;
          backend = await RustWorkbench.open(
            executable: executable!,
            package: package!,
            directory: directory!,
            managed: true,
          );
          expect(await backend!.load(), isEmpty);
        });
      } finally {
        await tester.pumpWidget(const SizedBox.shrink());
        await tester.runAsync(() async {
          if (first != null) await first.close();
          if (second != null) await second.close();
          await backend?.close();
          if (directory != null) await directory!.delete(recursive: true);
        });
        first?.dispose();
        second?.dispose();
      }
    },
    skip: unavailable,
    timeout: const Timeout(Duration(minutes: 3)),
  );

  testWidgets(
    'actual PluginTools enable/open/edit/close/reopen stays out of card storage',
    (tester) async {
      Directory? directory;
      RustWorkbench? backend;
      PluginUiController? controller;
      try {
        await tester.runAsync(() async {
          directory = await Directory.systemTemp.createTemp(
            'morrow-native-plugin-tools-',
          );
          backend = await RustWorkbench.open(
            executable: executable!,
            package: package!,
            directory: directory!,
            managed: true,
          );
          final state = await backend!.pluginState();
          expect(state.available, isTrue);
          if (state.enabled) await backend!.configurePlugin(state, false);
        });
        await tester.pumpWidget(
          MaterialApp(
            home: Scaffold(
              body: SingleChildScrollView(
                child: PluginTools(
                  backend: backend!,
                  onChanged: () {},
                  ink: Colors.black,
                  muted: Colors.black54,
                  line: Colors.black12,
                  radius: BorderRadius.circular(16),
                ),
              ),
            ),
          ),
        );
        await pumpHost(
          tester,
          () =>
              find.byKey(const ValueKey('plugin-enable')).evaluate().isNotEmpty,
        );
        await tester.tap(find.byKey(const ValueKey('plugin-enable')));
        await tester.pump();
        await pumpHost(
          tester,
          () => find
              .byKey(const ValueKey('plugin-text-tool'))
              .evaluate()
              .isNotEmpty,
        );
        await tester.tap(find.byKey(const ValueKey('plugin-text-tool')));
        await tester.pump();
        await pumpHost(
          tester,
          () => find.byType(TextField).evaluate().isNotEmpty,
        );
        controller = tester
            .widget<ManagedPluginForm>(find.byType(ManagedPluginForm))
            .controller;
        final generation = controller.generation!;
        await tester.enterText(find.byType(TextField), 'abc');
        await tester.pump();
        await pumpHost(tester, () => controller!.phase != PluginUiPhase.busy);
        expect(find.text('ABC'), findsOneWidget);
        expect(find.text('3 个字符 · 仅本次使用，不保存为卡片'), findsOneWidget);
        await tester.tap(find.byKey(const ValueKey('plugin-text-tool')));
        await tester.pump();
        await closeHostUi(tester, controller);
        expect(find.byType(TextField), findsNothing);
        await tester.tap(find.byKey(const ValueKey('plugin-text-tool')));
        await tester.pump();
        await pumpHost(
          tester,
          () => find.byType(TextField).evaluate().isNotEmpty,
        );
        controller = tester
            .widget<ManagedPluginForm>(find.byType(ManagedPluginForm))
            .controller;
        expect(controller.generation, greaterThan(generation));
        expect(find.text('ABC'), findsNothing);
        expect(find.text('0 个字符 · 仅本次使用，不保存为卡片'), findsOneWidget);
        await tester.runAsync(() async {
          expect(await backend!.load(), isEmpty);
        });
        expect(tester.takeException(), isNull);
      } finally {
        // PluginTools owns controller disposal; wait for the matching transport to close.
        await tester.pumpWidget(const SizedBox.shrink());
        if (controller != null) await closeHostUi(tester, controller);
        await tester.runAsync(() async {
          await backend?.close();
          if (directory != null) await directory!.delete(recursive: true);
        });
      }
    },
    skip: unavailable,
    timeout: const Timeout(Duration(minutes: 3)),
  );
}
