import 'dart:convert';
import 'dart:io';
import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/storage.dart';
import 'package:morrow_studio/fonts/font_choice.dart';
import 'package:morrow_studio/fonts/font_repository.dart';
import 'package:morrow_studio/fonts/font_settings.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  testWidgets(
    'local font loads into the real engine and restores in a fresh process',
    (t) async {
      final stateFile = File(Platform.environment['MORROW_FONT_STATE']!);
      final importing = Platform.environment['MORROW_FONT_PHASE'] == 'import';
      final storage = MemoryStorage();
      FontChoice? imported;
      if (importing) {
        imported = await t.runAsync(
          () => FontRepository.importFile(
            XFile(Platform.environment['MORROW_FONT_FIXTURE']!),
          ),
        );
      } else {
        storage.data =
            jsonDecode(await stateFile.readAsString()) as Map<String, dynamic>;
        // New process, empty font registry: verify the saved local bytes load.
        await FontRepository.load(FontChoice.fromJson(storage.data!['uiFont']));
      }
      await t.pumpWidget(
        MorrowApp(storage: storage, initialLocale: const Locale('en')),
      );
      await t.pumpAndSettle();
      if (importing) {
        final scope = FontScope.of(t.element(find.byType(Studio)));
        await scope.onChanged(imported!);
        await t.pumpAndSettle();
        await stateFile.writeAsString(jsonEncode(storage.data));
      }
      final choice = FontChoice.fromJson(storage.data!['uiFont']);
      expect(choice.imported, isTrue);
      expect(
        Theme.of(
          t.element(find.byType(Studio)),
        ).textTheme.bodyMedium!.fontFamily,
        choice.resolvedFamily,
      );
      await t.ensureVisible(find.byKey(const ValueKey('font-settings')));
      await t.tap(find.byKey(const ValueKey('font-settings')));
      await t.pumpAndSettle();
      expect(find.text(choice.name), findsOneWidget);
      expect(t.takeException(), isNull);
      await t.pumpWidget(const SizedBox());
    },
  );
}
