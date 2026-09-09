import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/storage.dart';
import 'package:morrow_studio/storage_migration_native.dart';
import 'package:shared_preferences/shared_preferences.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  test('existing snapshot and media references survive the rename', () async {
    final root = await Directory.systemTemp.createTemp('morrow-migration-');
    addTearDown(() => root.delete(recursive: true));
    final legacy = await Directory('${root.path}/legacy').create();
    final current = Directory('${root.path}/morrow');
    final media = await File(
      '${legacy.path}/photo.png',
    ).writeAsBytes([1, 2, 3]);
    final snapshot = {
      'version': 1,
      'title': '已有灵感',
      'texture': {'location': media.path, 'local': true},
    };
    final encoded = jsonEncode({
      'flutter.${LocalStorage.key}': jsonEncode(snapshot),
    });
    final source = await File(
      '${legacy.path}/shared_preferences.json',
    ).writeAsString(encoded);

    await copyLegacyPreferences(legacy: legacy, current: current);

    expect(
      await File('${current.path}/shared_preferences.json').readAsString(),
      encoded,
    );
    expect(await source.readAsString(), encoded);
    expect(await media.readAsBytes(), [1, 2, 3]);
    SharedPreferences.setMockInitialValues({
      LocalStorage.key: jsonEncode(snapshot),
    });
    expect(
      LocalStorage(await SharedPreferences.getInstance()).read(),
      snapshot,
    );
  });

  test(
    'existing Morrow data takes priority on every subsequent launch',
    () async {
      final root = await Directory.systemTemp.createTemp('morrow-migration-');
      addTearDown(() => root.delete(recursive: true));
      final legacy = await Directory('${root.path}/legacy').create();
      final current = await Directory('${root.path}/morrow').create();
      await File('${legacy.path}/shared_preferences.json').writeAsString('old');
      final target = await File(
        '${current.path}/shared_preferences.json',
      ).writeAsString('new');

      await copyLegacyPreferences(legacy: legacy, current: current);
      await copyLegacyPreferences(legacy: legacy, current: current);

      expect(await target.readAsString(), 'new');
    },
  );

  test('fresh install does not create an empty preferences snapshot', () async {
    final root = await Directory.systemTemp.createTemp('morrow-migration-');
    addTearDown(() => root.delete(recursive: true));
    final current = Directory('${root.path}/morrow');

    await copyLegacyPreferences(
      legacy: Directory('${root.path}/missing'),
      current: current,
    );

    expect(
      await File('${current.path}/shared_preferences.json').exists(),
      isFalse,
    );
  });
}
