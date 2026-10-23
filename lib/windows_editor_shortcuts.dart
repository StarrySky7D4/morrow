import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

// Some Windows keyboards report Numpad 7/1 even with Num Lock off. Keep
// navigation in the focused EditableText's actions, including its selection
// and composing handling, instead of editing a controller from a key handler.
final _navigation = <ShortcutActivator, Intent>{
  for (final key in {
    LogicalKeyboardKey.numpad7: false,
    LogicalKeyboardKey.numpad1: true,
  }.entries)
    for (final control in [false, true])
      for (final shift in [false, true])
        SingleActivator(
          key.key,
          control: control,
          shift: shift,
          numLock: LockState.unlocked,
        ): control
            ? ExtendSelectionToDocumentBoundaryIntent(
                forward: key.value,
                collapseSelection: !shift,
              )
            : ExtendSelectionToLineBreakIntent(
                forward: key.value,
                collapseSelection: !shift,
                continuesAtWrap: true,
              ),
};

class WindowsEditorShortcuts extends StatelessWidget {
  const WindowsEditorShortcuts({super.key, required this.child});

  final Widget child;

  @override
  Widget build(BuildContext context) =>
      defaultTargetPlatform == TargetPlatform.windows
      ? Shortcuts(shortcuts: _navigation, child: child)
      : child;
}
