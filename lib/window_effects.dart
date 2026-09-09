import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_acrylic/flutter_acrylic.dart' as acrylic;

class DesktopBackground {
  bool _initialized = false;
  Future<void> _pending = Future.value();
  Future<void> apply({
    required bool transparent,
    required bool dark,
    required Color color,
  }) {
    if (kIsWeb || defaultTargetPlatform != TargetPlatform.windows) {
      return Future.value();
    }
    final operation = _pending.then((_) async {
      if (!_initialized) {
        await acrylic.Window.initialize();
        _initialized = true;
      }
      await acrylic.Window.setEffect(
        effect: transparent
            ? acrylic.WindowEffect.transparent
            : acrylic.WindowEffect.solid,
        color: transparent ? Colors.transparent : color,
        dark: dark,
      );
    });
    _pending = operation.catchError((Object _) {});
    return operation;
  }
}
