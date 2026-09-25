import 'package:flutter/material.dart';
import 'package:integration_test/integration_test.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:window_manager/window_manager.dart';
import '../test/responsive_workflow_test.dart' as responsive;
import '../test/application_shutdown_test.dart' as shutdown;
import '../test/reorder_test.dart' as reorder;
import '../test/editor_capture_test.dart' as editor;

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  setUpAll(() async {
    await windowManager.ensureInitialized();
    await windowManager.setSize(const Size(1440, 1100));
  });
  responsive.main();
  shutdown.main();
  reorder.main();
  editor.main();
}
