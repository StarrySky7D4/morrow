import 'package:flutter/material.dart';
import 'package:integration_test/integration_test.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:window_manager/window_manager.dart';
import '../test/masonry_geometry_test.dart' as geometry;
import '../test/workspace_lifecycle_test.dart' as lifecycle;
import '../test/reorder_test.dart' as reorder;

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  setUpAll(() async {
    await windowManager.ensureInitialized();
    await windowManager.setSize(const Size(1440, 1100));
  });
  geometry.main();
  lifecycle.main();
  reorder.main();
}
