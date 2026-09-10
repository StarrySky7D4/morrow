import 'package:morrow_studio/main.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets(
    'Android footer avoids system bars and keyboard; status icons follow theme',
    (tester) async {
      tester.view.devicePixelRatio = 1;
      tester.view.physicalSize = const Size(360, 800);
      tester.view.viewPadding = const FakeViewPadding(top: 28, bottom: 24);
      tester.view.padding = const FakeViewPadding(top: 28, bottom: 24);
      addTearDown(tester.view.reset);
      await tester.pumpWidget(const MorrowApp());
      await tester.pumpAndSettle();
      final footer = find.byKey(const ValueKey('footer-dock'));
      expect(tester.getRect(footer).bottom, closeTo(764, .1));
      expect(
        tester.getRect(find.byKey(const ValueKey('header-search'))).top,
        greaterThanOrEqualTo(28),
      );
      expect(find.byKey(const ValueKey('window-radius')), findsNothing);
      await tester.tap(find.byKey(const ValueKey('appearance-toggle')));
      await tester.pumpAndSettle();
      for (final theme in ['dark', 'white']) {
        final button = find.byKey(ValueKey('theme-$theme'));
        await tester.ensureVisible(button);
        await tester.tap(button);
        await tester.pumpAndSettle();
        final overlay = tester.widget<AnnotatedRegion<SystemUiOverlayStyle>>(
          find.byType(AnnotatedRegion<SystemUiOverlayStyle>).first,
        );
        expect(
          overlay.value.statusBarIconBrightness,
          theme == 'dark' ? Brightness.light : Brightness.dark,
        );
      }
      await tester.tap(find.byKey(const ValueKey('compact-settings-back')));
      await tester.pumpAndSettle();
      tester.view.viewInsets = const FakeViewPadding(bottom: 280);
      tester.view.padding = const FakeViewPadding(top: 28);
      await tester.pumpAndSettle();
      expect(tester.getRect(footer).bottom, closeTo(508, .1));
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox());
    },
    variant: TargetPlatformVariant.only(TargetPlatform.android),
  );
}
