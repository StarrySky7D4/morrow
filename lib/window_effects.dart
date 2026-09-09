import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_acrylic/flutter_acrylic.dart' as acrylic;

class DesktopBackground {
  bool _initialized = false;
  bool _transparent = false;
  Future<void> _pending = Future.value();
  Future<void> apply() {
    if (kIsWeb || defaultTargetPlatform != TargetPlatform.windows) {
      return Future.value();
    }
    final operation = _pending.then((_) async {
      if (_transparent) return;
      if (!_initialized) {
        await acrylic.Window.initialize();
        _initialized = true;
      }
      await acrylic.Window.setEffect(
        // Flutter paints every background inside its rounded canvas. A native
        // solid accent creates a second, rectangular fill behind that canvas,
        // exposed most clearly at the corners of a playing video texture.
        // Canvas materials must not change the native composition mode:
        // Aero blur can replace per-pixel desktop transparency on Windows.
        effect: acrylic.WindowEffect.transparent,
        color: Colors.transparent,
      );
      _transparent = true;
    });
    _pending = operation.catchError((Object _) {});
    return operation;
  }
}
