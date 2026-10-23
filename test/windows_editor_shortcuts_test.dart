import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/windows_editor_shortcuts.dart';

void main() {
  testWidgets(
    'Windows keypad Home and End preserve navigation and selection',
    (tester) async {
      final controller = TextEditingController(text: 'first line\nsecond line');
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: WindowsEditorShortcuts(
              child: TextField(controller: controller, maxLines: 3),
            ),
          ),
        ),
      );
      await tester.tap(find.byType(TextField));
      await tester.pump();
      expect(
        HardwareKeyboard.instance.lockModesEnabled,
        isNot(contains(KeyboardLockMode.numLock)),
      );
      for (final key in [
        LogicalKeyboardKey.numpad7,
        LogicalKeyboardKey.numpad1,
      ]) {
        for (final control in [false, true]) {
          for (final shift in [false, true]) {
            controller.selection = const TextSelection.collapsed(offset: 16);
            if (control) {
              await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
            }
            if (shift) {
              await tester.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
            }
            await tester.sendKeyEvent(key);
            if (shift) {
              await tester.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
            }
            if (control) {
              await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
            }
            await tester.pump();
            final target = key == LogicalKeyboardKey.numpad1
                ? 22
                : control
                ? 0
                : 11;
            expect(controller.selection.baseOffset, shift ? 16 : target);
            expect(controller.selection.extentOffset, target);
            expect(controller.text, 'first line\nsecond line');
          }
        }
      }
      await tester.pumpWidget(const SizedBox());
      controller.dispose();
    },
    variant: TargetPlatformVariant.only(TargetPlatform.windows),
  );

  testWidgets(
    'Num Lock on leaves keypad number events unhandled by navigation',
    (tester) async {
      final controller = TextEditingController(text: 'first line\nsecond line');
      var unhandled = 0;
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: Focus(
              onKeyEvent: (_, event) {
                if (event is KeyDownEvent &&
                    [
                      LogicalKeyboardKey.numpad7,
                      LogicalKeyboardKey.numpad1,
                    ].contains(event.logicalKey)) {
                  unhandled++;
                }
                return KeyEventResult.ignored;
              },
              child: WindowsEditorShortcuts(
                child: TextField(controller: controller, maxLines: 3),
              ),
            ),
          ),
        ),
      );
      await tester.tap(find.byType(TextField));
      await tester.pump();
      controller.selection = const TextSelection.collapsed(offset: 16);
      await tester.sendKeyEvent(LogicalKeyboardKey.numLock);
      expect(
        HardwareKeyboard.instance.lockModesEnabled,
        contains(KeyboardLockMode.numLock),
      );
      for (final key in [
        LogicalKeyboardKey.numpad7,
        LogicalKeyboardKey.numpad1,
      ]) {
        await tester.sendKeyEvent(key);
        await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
        await tester.sendKeyEvent(key);
        await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
      }
      expect(unhandled, 4);
      expect(controller.selection, const TextSelection.collapsed(offset: 16));
      await tester.sendKeyEvent(LogicalKeyboardKey.numLock);
      await tester.pumpWidget(const SizedBox());
      controller.dispose();
    },
    variant: TargetPlatformVariant.only(TargetPlatform.windows),
  );
}
