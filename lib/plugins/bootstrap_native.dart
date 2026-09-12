import 'dart:io';
import '../main_rust.dart' as application;

Future<bool> startRustWorkbench(List<String> arguments) async {
  if (!Platform.isWindows) return false;
  await application.main(arguments);
  return true;
}
