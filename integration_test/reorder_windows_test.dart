import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'package:window_manager/window_manager.dart';
import 'package:morrow_studio/card_order_preferences.dart';
import '../test/reorder_test.dart' as reorder;
import '../test/versioned_task_panel_test.dart' as tasks;
import '../test/tip_settings_test.dart' as tips;

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  setUpAll(() async {
    await windowManager.ensureInitialized();
    await windowManager.setSize(const Size(1440, 1000));
  });
  reorder.main();
  tasks.main();
  tips.main();
  testWidgets('Windows local card order persists through plugin reload', (
    t,
  ) async {
    await t.runAsync(() async {
      final store = LocalCardOrderStore(
        'reorder-test-${DateTime.now().microsecondsSinceEpoch}',
      );
      final first = CardOrderPreferences(store),
          next = CardOrderPreferences(store);
      try {
        await first.restore();
        await first.move(
          'overview',
          '甲',
          '丙',
          true,
          all: ['甲', '乙', '丙'],
          visible: ['甲', '乙', '丙'],
        );
        await (await SharedPreferences.getInstance()).reload();
        await next.restore();
        expect(next.manual('overview'), isTrue);
        expect(next.order('overview', ['甲', '乙', '丙']), ['乙', '丙', '甲']);
      } finally {
        first.dispose();
        next.dispose();
        await (await SharedPreferences.getInstance()).remove(store.key);
      }
    });
  });
}
