import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:window_manager/window_manager.dart';
import '../test/context_menu_workflow_test.dart' as menus;
import '../test/versioned_task_panel_test.dart' as tasks;

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  setUpAll(() async {
    await windowManager.ensureInitialized();
    await windowManager.setSize(const Size(1440, 1000));
  });
  menus.main();
  tasks.main();
}
