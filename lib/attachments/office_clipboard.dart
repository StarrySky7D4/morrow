import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

const _channel = MethodChannel('morrow/office_clipboard');

bool get windowsOfficeClipboard =>
    !kIsWeb && defaultTargetPlatform == TargetPlatform.windows;

Future<int?> clipboardSequence() async {
  if (!windowsOfficeClipboard) return null;
  try {
    return await _channel.invokeMethod<int>('sequence');
  } on MissingPluginException {
    return null;
  }
}

Future<Map<Object?, Object?>> readOfficeClipboard(int? sequence) async {
  if (!windowsOfficeClipboard) return const {};
  try {
    return await _channel
            .invokeMapMethod<Object?, Object?>('read', {'sequence': sequence})
            .timeout(const Duration(seconds: 30)) ??
        const {};
  } on MissingPluginException {
    return const {};
  }
}
