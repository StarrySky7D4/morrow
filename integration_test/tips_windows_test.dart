import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'package:window_manager/window_manager.dart';
import 'package:morrow_studio/tip_preferences.dart';
import '../test/tip_settings_test.dart' as tips;

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  setUpAll(() async {
    await windowManager.ensureInitialized();
    await windowManager.setSize(const Size(1440, 1000));
  });
  tips.main();
  testWidgets(
    'Windows local preference plugin writes and reloads Unicode tip text',
    (t) async {
      await t.runAsync(() async {
        final store = LocalTipStore(
          libraryDirectory:
              'morrow-tips-qualification-${DateTime.now().microsecondsSinceEpoch}',
        );
        final first = TipPreferences(store);
        final reopened = TipPreferences(store);
        try {
          await first.restore();
          await first.save('footer', '本地保存 🌙\n第二条提示');
          final daily = TipItem.empty().withText('窗边休息一下 🌿');
          await first.saveItems('daily', [daily], caption: '慢慢来');
          await (await SharedPreferences.getInstance()).reload();
          await reopened.restore();
          expect(reopened.customLines('footer'), ['本地保存 🌙', '第二条提示']);
          expect(
            reopened.items('daily', ['水', '灵感', '探索']).single.id,
            daily.id,
          );
          expect(reopened.dailyCaption('默认'), '慢慢来');
          await reopened.save('footer', '');
          await (await SharedPreferences.getInstance()).reload();
          await first.restore();
          expect(first.customLines('footer'), isNull);
          expect(first.customLines('daily'), ['窗边休息一下 🌿']);
        } finally {
          first.dispose();
          reopened.dispose();
          await (await SharedPreferences.getInstance()).remove(store.key);
        }
      });
    },
  );
}
