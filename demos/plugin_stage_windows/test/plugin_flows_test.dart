import 'dart:io';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_stage_demo/demo_bridge.dart';
import 'package:morrow_plugin_ui/morrow_plugin_ui.dart';

void main() {
  final root = Platform.environment['MORROW_STAGE_DEMO_ROOT']!;
  String value(DemoBridge b, String id) =>
      b.document.nodes.firstWhere((n) => n.id == id).text;
  Future<void> send(
    DemoBridge b,
    String id,
    String action,
    EventKind kind, {
    String text = '',
    bool checked = false,
  }) async {
    await b.edit(
      UiIntent(
        node: id,
        action: action,
        kind: kind,
        text: text,
        checked: checked,
      ),
    );
  }

  test('Rust Unicode, normalization, empty text and field budget', () async {
    final b = await DemoBridge.open(root, 'rust');
    addTearDown(b.close);
    await send(
      b,
      'title',
      'edit',
      EventKind.editText,
      text: '  你好 🌱  \n\n 世界  ',
    );
    expect(value(b, 'result'), '你好 🌱\n世界');
    expect(value(b, 'detail'), startsWith('5 个非空白字符 · 2 行'));
    await send(b, 'option', 'compact', EventKind.setToggle, checked: true);
    expect(value(b, 'result'), '你好 🌱 世界');
    await send(b, 'apply', 'tidy', EventKind.activate);
    expect(value(b, 'title'), '你好 🌱 世界');
    await send(b, 'reset', 'clear', EventKind.activate);
    expect(value(b, 'result'), '');
    await expectLater(
      send(b, 'title', 'edit', EventKind.editText, text: 'x' * 2049),
      throwsA(anything),
    );
  });
  test(
    'C both unit modes, negative and decimal values, invalid input recovery',
    () async {
      final b = await DemoBridge.open(root, 'c');
      addTearDown(b.close);
      await send(b, 'title', 'edit', EventKind.editText, text: '-40');
      expect(value(b, 'result'), '-40.000 °F');
      await send(b, 'reverse', 'reverse', EventKind.setToggle, checked: true);
      expect(value(b, 'result'), '-40.000 °C');
      await send(b, 'mode', 'mode', EventKind.setToggle, checked: true);
      await send(b, 'title', 'edit', EventKind.editText, text: '3.5');
      expect(value(b, 'result'), '1.067 米');
      for (final invalid in ['', '.', '1e3', 'NaN', '1000001', '-1000001']) {
        await send(b, 'title', 'edit', EventKind.editText, text: invalid);
        expect(value(b, 'result'), '等待有效数值');
      }
      await send(b, 'reset', 'reset', EventKind.activate);
      expect(value(b, 'result'), '68.000 °F');
    },
  );
  test(
    'C++ sort preserves checked identity, clear/filter/reset, duplicates and Unicode',
    () async {
      final b = await DemoBridge.open(root, 'cpp');
      addTearDown(b.close);
      await send(
        b,
        'title',
        'edit',
        EventKind.editText,
        text: 'Zebra\nAlpha\nAlpha\n你好🌱',
      );
      await send(b, 'item0', 'item0', EventKind.setToggle, checked: true);
      await send(b, 'sort', 'sort', EventKind.activate);
      expect(value(b, 'result'), '○ Alpha\n○ Alpha\n✓ Zebra\n○ 你好🌱');
      await send(b, 'option', 'hide', EventKind.setToggle, checked: true);
      expect(value(b, 'result'), isNot(contains('Zebra')));
      await send(b, 'clear', 'clear', EventKind.activate);
      expect(value(b, 'title'), 'Alpha\nAlpha\n你好🌱');
      await send(b, 'title', 'edit', EventKind.editText, text: '字' * 200);
      expect(value(b, 'result'), startsWith('○ 字'));
      await send(b, 'reset', 'reset', EventKind.activate);
      expect(value(b, 'detail'), '已完成 0 / 3 · 0%');
    },
  );
  test(
    'host rejects a disabled action instead of letting it reach the guest',
    () async {
      final b = await DemoBridge.open(root, 'cpp');
      addTearDown(b.close);
      expect(
        b.document.nodes.firstWhere((n) => n.id == 'clear').enabled,
        false,
      );
      await expectLater(
        send(b, 'clear', 'clear', EventKind.activate),
        throwsA(anything),
      );
    },
  );
}
