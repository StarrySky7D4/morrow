import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:morrow_studio/save_recovery.dart';
import 'package:morrow_studio/save_recovery_dialog.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/storage.dart';

class AppRecovery extends MemoryStorage implements SaveRecoveryStorage {
  final recovery = Recovery(
    const SaveRecovery(
      operation: 'old',
      digest: 'digest',
      committed: true,
      conflict: false,
    ),
  );
  @override
  Future<SaveRecovery?> inspectSaveRecovery() => recovery.inspectSaveRecovery();
  @override
  Future<void> resolveSaveRecovery(
    SaveRecovery observed, {
    required bool abandon,
  }) => recovery.resolveSaveRecovery(observed, abandon: abandon);
}

class Recovery implements SaveRecoveryStorage {
  Recovery(this.value);
  SaveRecovery? value;
  final calls = <bool>[];
  final draft = {'theme': 'newer draft'};
  @override
  Future<SaveRecovery?> inspectSaveRecovery() async => value;
  @override
  Future<void> resolveSaveRecovery(
    SaveRecovery observed, {
    required bool abandon,
  }) async {
    expect(observed, same(value));
    calls.add(abandon);
    value = null;
  }
}

void main() {
  testWidgets(
    'main window exposes restored save without replacing workspace or auto-saving',
    (t) async {
      final storage = AppRecovery();
      await t.pumpWidget(
        MorrowApp(storage: storage, initialLocale: const Locale('en')),
      );
      await t.pumpAndSettle();
      final studio = t.state(find.byType(Studio));
      await t.tap(find.text('Review save'));
      await t.pumpAndSettle();
      expect(find.byType(SaveRecoveryDialog), findsOneWidget);
      await t.tap(find.byKey(const ValueKey('save-recovery-resolve')));
      await t.pumpAndSettle();
      expect(storage.recovery.calls, [false]);
      expect(
        storage.data,
        isNull,
      ); // Resolving old outcome is not saving the new draft.
      expect(t.state(find.byType(Studio)), same(studio));
      expect(t.takeException(), isNull);
    },
  );
  Future<void> open(
    WidgetTester t,
    Recovery storage, {
    String locale = 'en',
  }) async {
    await t.pumpWidget(
      MaterialApp(
        locale: Locale(locale),
        supportedLocales: AppLocalizations.supportedLocales,
        localizationsDelegates: const [
          AppLocalizations.delegate,
          GlobalMaterialLocalizations.delegate,
          GlobalWidgetsLocalizations.delegate,
          GlobalCupertinoLocalizations.delegate,
        ],
        home: Builder(
          builder: (context) => Scaffold(
            body: TextButton(
              onPressed: () => showDialog<SaveRecoveryResult>(
                context: context,
                builder: (_) => SaveRecoveryDialog(storage: storage),
              ),
              child: const Text('open'),
            ),
          ),
        ),
      ),
    );
    await t.tap(find.text('open'));
    await t.pumpAndSettle();
  }

  testWidgets(
    'conflict requires explicit confirmation; cancel preserves pending proposal and new draft',
    (t) async {
      final storage = Recovery(
        const SaveRecovery(
          operation: 'old',
          digest: 'digest',
          committed: false,
          conflict: true,
        ),
      );
      await open(t, storage);
      expect(find.byKey(const ValueKey('save-recovery-resolve')), findsNothing);
      await t.tap(find.byKey(const ValueKey('save-recovery-abandon')));
      await t.pumpAndSettle();
      await t.tap(find.text('Cancel').last);
      await t.pumpAndSettle();
      expect(storage.calls, isEmpty);
      expect(storage.value, isNotNull);
      await t.tap(find.byKey(const ValueKey('save-recovery-abandon')));
      await t.pumpAndSettle();
      await t.tap(find.byKey(const ValueKey('save-recovery-confirm-abandon')));
      await t.pumpAndSettle();
      expect(storage.calls, [true]);
      expect(storage.draft['theme'], 'newer draft');
    },
  );
  testWidgets(
    'committed result cannot be discarded; all languages fit a narrow dialog',
    (t) async {
      t.view.devicePixelRatio = 1;
      t.view.physicalSize = const Size(390, 760);
      addTearDown(t.view.reset);
      for (final locale in [
        'zh',
        'en',
        'ru',
        'fr',
        'de',
        'es',
        'ja',
        'ko',
        'pt',
      ]) {
        await t.pumpWidget(const SizedBox());
        final storage = Recovery(
          const SaveRecovery(
            operation: 'old',
            digest: 'digest',
            committed: true,
            conflict: false,
          ),
        );
        await open(t, storage, locale: locale);
        expect(
          find.byKey(const ValueKey('save-recovery-abandon')),
          findsNothing,
        );
        expect(t.takeException(), isNull);
        await t.tap(find.byKey(const ValueKey('save-recovery-resolve')));
        await t.pumpAndSettle();
        expect(storage.calls, [false]);
        expect(storage.draft['theme'], 'newer draft');
      }
    },
  );
}
