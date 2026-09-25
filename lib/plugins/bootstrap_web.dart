import 'package:flutter/widgets.dart';
import '../main_web.dart';

Future<bool> startRustWorkbench(List<String> arguments) async {
  WidgetsFlutterBinding.ensureInitialized();
  runApp(const BrowserWorkspace());
  return true;
}
