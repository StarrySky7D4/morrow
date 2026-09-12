import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_acrylic/flutter_acrylic.dart' as acrylic;

class DesktopBackground {
  bool _initialized = false;
  bool _transparent = false;
  Future<void> _pending = Future.value();
  double? _frost;
  Future<void> apply({double frost = 0}) {
    if (kIsWeb || defaultTargetPlatform != TargetPlatform.windows) {
      return Future.value();
    }
    final operation = _pending.then((_) async {
      if (_transparent && _frost == frost) return;
      if (_transparent) {
        await const MethodChannel('morrow/window_shape').invokeMethod<void>(
          'setCanvasBlur',
          {'blur': frost, 'topInset': 0.0},
        );
        _frost = frost;
        return;
      }
      if (!_initialized) {
        await acrylic.Window.initialize();
        _initialized = true;
      }
      await acrylic.Window.setEffect(
        // Flutter paints every background inside its rounded canvas. A native
        // solid accent creates a second, rectangular fill behind that canvas,
        // exposed most clearly at the corners of a playing video texture.
        // Initial and zero-blur mode uses per-pixel transparency. The native
        // canvas owns its sampling source and fades it to zero when disabled.
        effect: acrylic.WindowEffect.transparent,
        color: Colors.transparent,
      );
      _transparent = true;
      if (frost > 0) {
        await const MethodChannel('morrow/window_shape').invokeMethod<void>(
          'setCanvasBlur',
          {'blur': frost, 'topInset': 0.0},
        );
      }
      _frost = frost;
    });
    _pending = operation.catchError((Object _) {});
    return operation;
  }
}
