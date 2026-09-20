import 'dart:async';
import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:morrow_studio/plugins/credential_manager.dart';

Uint8List ref(int value) => Uint8List.fromList(List.filled(32, value));
String hex(int value) => value.toRadixString(16).padLeft(2, '0') * 32;
StoredCredential entry(int value, {int revision = 1, bool disabled = false}) =>
    StoredCredential(
      reference: ref(value),
      revision: BigInt.from(revision),
      createdMs: BigInt.from(DateTime(2026, 1).millisecondsSinceEpoch),
      expiresMs: BigInt.from(DateTime(2099, 1).millisecondsSinceEpoch),
      disabled: disabled,
    );

class SaveCall {
  SaveCall(this.reference, this.revision, this.name, this.value, this.days);
  final Uint8List reference;
  final BigInt revision;
  final String name, value;
  final int days;
}

class FakeCredentials implements WorkbenchCredentialControl {
  List<StoredCredential> entries = [];
  final saves = <SaveCall>[];
  final disables = <StoredCredential>[];
  int reads = 0;
  Future<CredentialPage> Function(Uint8List?, Uint8List?)? page;
  Completer<StoredCredential>? saveGate;
  Object? saveError;
  @override
  Future<CredentialPage> credentialPage({
    Uint8List? after,
    Uint8List? snapshot,
  }) async {
    reads++;
    if (page != null) return page!(after, snapshot);
    return CredentialPage(entries: entries, snapshot: ref(9));
  }

  @override
  Future<StoredCredential> saveCredential({
    required Uint8List reference,
    required BigInt expectedRevision,
    required String headerName,
    required String headerValue,
    required int lifetimeDays,
  }) async {
    saves.add(
      SaveCall(
        Uint8List.fromList(reference),
        expectedRevision,
        headerName,
        headerValue,
        lifetimeDays,
      ),
    );
    if (saveError != null) throw saveError!;
    if (saveGate != null) return saveGate!.future;
    final saved = entry(
      reference.isEmpty ? 5 : reference.first,
      revision: expectedRevision.toInt() + 1,
    );
    entries = [
      ...entries.where((e) => e.reference.first != saved.reference.first),
      saved,
    ];
    return saved;
  }

  @override
  Future<StoredCredential> disableCredential(StoredCredential expected) async {
    disables.add(expected);
    final saved = entry(
      expected.reference.first,
      revision: expected.revision.toInt() + 1,
      disabled: true,
    );
    entries = entries
        .map((e) => e.reference.first == expected.reference.first ? saved : e)
        .toList();
    return saved;
  }
}

Widget page(FakeCredentials backend, {String locale = 'en'}) => MaterialApp(
  locale: Locale(locale),
  localizationsDelegates: AppLocalizations.localizationsDelegates,
  supportedLocales: AppLocalizations.supportedLocales,
  home: Scaffold(
    body: SingleChildScrollView(
      child: CredentialManager(
        backend: backend,
        ink: Colors.black,
        muted: Colors.grey,
        line: Colors.grey,
        radius: BorderRadius.circular(12),
      ),
    ),
  ),
);
Future<void> click(WidgetTester tester, String key) async {
  final target = find.byKey(ValueKey(key));
  await tester.ensureVisible(target);
  await tester.pumpAndSettle();
  await tester.tap(target);
  await tester.pumpAndSettle();
}

TextEditingController secretController(WidgetTester tester) => tester
    .widget<TextField>(find.byKey(const ValueKey('credential-secret')))
    .controller!;
void main() {
  testWidgets(
    'create is explicit, obscured, preserves secret and clears buffer before awaiting',
    (tester) async {
      final backend = FakeCredentials()
        ..saveGate = Completer<StoredCredential>();
      await tester.pumpWidget(page(backend));
      await tester.pumpAndSettle();
      expect(backend.saves, isEmpty);
      await click(tester, 'credential-new');
      final secret = tester.widget<TextField>(
        find.byKey(const ValueKey('credential-secret')),
      );
      expect(secret.obscureText, isTrue);
      expect(secret.enableSuggestions, isFalse);
      expect(secret.autocorrect, isFalse);
      await tester.enterText(
        find.byKey(const ValueKey('credential-secret')),
        ' Bearer preserved-value ',
      );
      await click(tester, 'credential-save');
      expect(secret.controller!.text, isEmpty);
      expect(backend.saves, hasLength(1));
      final saved = backend.saves.single;
      expect(saved.reference, isEmpty);
      expect(saved.revision, BigInt.zero);
      expect(saved.name, 'authorization');
      expect(saved.value, ' Bearer preserved-value ');
      expect(saved.days, 7);
      backend.saveGate!.complete(entry(5));
      await tester.pumpAndSettle();
      expect(find.byKey(ValueKey('credential-row-${hex(5)}')), findsOneWidget);
      expect(find.textContaining('preserved-value'), findsNothing);
      expect(
        find.text(
          'Credential saved. API connections still need separate approval.',
        ),
        findsOneWidget,
      );
    },
  );
  testWidgets(
    'replace needs fresh secret, preserves reference and exact CAS revision; disable is explicit',
    (tester) async {
      final backend = FakeCredentials()..entries = [entry(3, revision: 9)];
      await tester.pumpWidget(page(backend));
      await tester.pumpAndSettle();
      await click(tester, 'credential-replace-${hex(3)}');
      expect(secretController(tester).text, isEmpty);
      await click(tester, 'credential-save');
      expect(backend.saves, isEmpty);
      await tester.enterText(
        find.byKey(const ValueKey('credential-secret')),
        'replacement-only',
      );
      await tester.enterText(
        find.byKey(const ValueKey('credential-header')),
        ' X-API-Key ',
      );
      await click(tester, 'credential-save');
      expect(backend.saves.single.reference, orderedEquals(ref(3)));
      expect(backend.saves.single.revision, BigInt.from(9));
      expect(backend.saves.single.name, 'x-api-key');
      expect(backend.disables, isEmpty);
      await click(tester, 'credential-disable-${hex(3)}');
      expect(backend.disables.single.revision, BigInt.from(10));
      expect(find.text('Disabled'), findsOneWidget);
      final button = tester.widget<OutlinedButton>(
        find.byKey(ValueKey('credential-disable-${hex(3)}')),
      );
      expect(button.onPressed, isNull);
    },
  );
  testWidgets(
    'conflict or unknown error never echoes secret or retries and requires explicit refresh',
    (tester) async {
      final backend = FakeCredentials()
        ..saveError = StateError('conflict: never-display-secret');
      await tester.pumpWidget(page(backend));
      await tester.pumpAndSettle();
      await click(tester, 'credential-new');
      final controller = secretController(tester);
      await tester.enterText(
        find.byKey(const ValueKey('credential-secret')),
        'never-display-secret',
      );
      await click(tester, 'credential-save');
      expect(controller.text, isEmpty);
      expect(backend.saves, hasLength(1));
      expect(backend.reads, 1);
      expect(find.textContaining('never-display-secret'), findsNothing);
      expect(find.textContaining('could not be confirmed'), findsOneWidget);
      expect(
        tester
            .widget<OutlinedButton>(
              find.byKey(const ValueKey('credential-new')),
            )
            .onPressed,
        isNull,
      );
      await tester.pump(const Duration(seconds: 2));
      expect(backend.saves, hasLength(1));
      await click(tester, 'credential-refresh');
      expect(backend.reads, 2);
      expect(
        tester
            .widget<OutlinedButton>(
              find.byKey(const ValueKey('credential-new')),
            )
            .onPressed,
        isNotNull,
      );
    },
  );
  testWidgets(
    'closing form clears its secret and replacement starts empty with default lifetime',
    (tester) async {
      final backend = FakeCredentials()..entries = [entry(2)];
      await tester.pumpWidget(page(backend));
      await tester.pumpAndSettle();
      await click(tester, 'credential-new');
      final controller = secretController(tester);
      await tester.enterText(
        find.byKey(const ValueKey('credential-secret')),
        'discard-on-close',
      );
      await click(tester, 'credential-close');
      expect(controller.text, isEmpty);
      await click(tester, 'credential-replace-${hex(2)}');
      expect(secretController(tester).text, isEmpty);
      expect(backend.saves, isEmpty);
    },
  );
  testWidgets(
    'stale backend loads and completed saves cannot populate replacement backend',
    (tester) async {
      final old = FakeCredentials();
      final oldPage = Completer<CredentialPage>();
      old.page = (_, _) => oldPage.future;
      final next = FakeCredentials()..entries = [entry(8)];
      await tester.pumpWidget(page(old));
      await tester.pump();
      await tester.pumpWidget(page(next));
      await tester.pumpAndSettle();
      oldPage.complete(CredentialPage(entries: [entry(1)], snapshot: ref(9)));
      await tester.pumpAndSettle();
      expect(find.byKey(ValueKey('credential-row-${hex(1)}')), findsNothing);
      expect(find.byKey(ValueKey('credential-row-${hex(8)}')), findsOneWidget);
      next.saveGate = Completer<StoredCredential>();
      await click(tester, 'credential-new');
      final controller = secretController(tester);
      await tester.enterText(
        find.byKey(const ValueKey('credential-secret')),
        'old-backend-secret',
      );
      await click(tester, 'credential-save');
      final third = FakeCredentials()..entries = [entry(9)];
      await tester.pumpWidget(page(third));
      await tester.pumpAndSettle();
      expect(controller.text, isEmpty);
      next.saveGate!.complete(entry(5));
      await tester.pumpAndSettle();
      expect(find.byKey(ValueKey('credential-row-${hex(5)}')), findsNothing);
      expect(find.byKey(ValueKey('credential-row-${hex(9)}')), findsOneWidget);
      expect(third.saves, isEmpty);
      expect(tester.takeException(), isNull);
    },
  );
  testWidgets(
    'dispose clears unsaved secrets and suppresses a pending backend failure',
    (tester) async {
      final backend = FakeCredentials();
      await tester.pumpWidget(page(backend));
      await tester.pumpAndSettle();
      await click(tester, 'credential-new');
      final unsaved = secretController(tester);
      await tester.enterText(
        find.byKey(const ValueKey('credential-secret')),
        'discard-on-dispose',
      );
      await tester.pumpWidget(const SizedBox());
      expect(unsaved.text, isEmpty);
      expect(backend.saves, isEmpty);
      backend.saveGate = Completer<StoredCredential>();
      await tester.pumpWidget(page(backend));
      await tester.pumpAndSettle();
      await click(tester, 'credential-new');
      await tester.enterText(
        find.byKey(const ValueKey('credential-secret')),
        'pending-secret',
      );
      await click(tester, 'credential-save');
      await tester.pumpWidget(const SizedBox());
      backend.saveGate!.completeError(StateError('pending-secret'));
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);
    },
  );
  testWidgets(
    'filtered empty pages advance, consistent snapshot loads complete list',
    (tester) async {
      final backend = FakeCredentials();
      backend.page = (after, snapshot) async {
        if (after == null) {
          expect(snapshot, isNull);
          return CredentialPage(
            entries: const [],
            snapshot: ref(9),
            next: ref(1),
          );
        }
        expect(after, orderedEquals(ref(1)));
        expect(snapshot, orderedEquals(ref(9)));
        return CredentialPage(entries: [entry(2)], snapshot: ref(9));
      };
      await tester.pumpWidget(page(backend));
      await tester.pumpAndSettle();
      expect(backend.reads, 2);
      expect(find.byKey(ValueKey('credential-row-${hex(2)}')), findsOneWidget);
    },
  );
  testWidgets(
    'changed snapshots, looping cursor, and oversized timestamps fail closed',
    (tester) async {
      for (var mode = 0; mode < 3; mode++) {
        final backend = FakeCredentials();
        backend.page = (after, snapshot) async {
          if (mode == 2) {
            return CredentialPage(
              entries: [
                StoredCredential(
                  reference: ref(1),
                  revision: BigInt.one,
                  createdMs: BigInt.one,
                  expiresMs: (BigInt.one << 64) - BigInt.one,
                  disabled: false,
                ),
              ],
              snapshot: ref(9),
            );
          }
          if (after == null) {
            return CredentialPage(
              entries: [entry(1)],
              snapshot: ref(9),
              next: ref(2),
            );
          }
          return CredentialPage(
            entries: const [],
            snapshot: ref(mode == 0 ? 8 : 9),
            next: ref(2),
          );
        };
        await tester.pumpWidget(page(backend));
        await tester.pumpAndSettle();
        expect(
          find.textContaining('could not be read consistently'),
          findsOneWidget,
        );
        expect(find.byKey(ValueKey('credential-row-${hex(1)}')), findsNothing);
        expect(
          tester
              .widget<OutlinedButton>(
                find.byKey(const ValueKey('credential-new')),
              )
              .onPressed,
          isNull,
        );
        expect(tester.takeException(), isNull);
      }
    },
  );
  testWidgets(
    'English and Chinese forms remain usable at 320px with explicit lifetime choices',
    (tester) async {
      tester.view.physicalSize = const Size(320, 1000);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      for (final locale in ['en', 'zh']) {
        final backend = FakeCredentials()..entries = [entry(4)];
        await tester.pumpWidget(page(backend, locale: locale));
        await tester.pumpAndSettle();
        expect(
          find.text(locale == 'en' ? 'API credentials' : 'API 凭据'),
          findsOneWidget,
        );
        await click(tester, 'credential-new');
        final days = find.byType(DropdownButtonFormField<int>);
        await tester.ensureVisible(days);
        await tester.tap(days);
        await tester.pumpAndSettle();
        final one = find.text(locale == 'en' ? '1 day' : '1 天');
        await tester.scrollUntilVisible(
          one,
          -80,
          scrollable: find.byType(Scrollable).last,
        );
        await tester.tap(one);
        await tester.pumpAndSettle();
        await tester.enterText(
          find.byKey(const ValueKey('credential-secret')),
          'narrow-new-secret',
        );
        await click(tester, 'credential-save');
        expect(backend.saves.single.days, 1);
        expect(tester.takeException(), isNull);
      }
    },
  );
}
