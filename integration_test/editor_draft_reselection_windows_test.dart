// Real Windows engine/window and original host across separate client processes.
// Framework input does not certify physical mouse, keyboard, clipboard or IME.
import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:integration_test/integration_test.dart';
import 'package:morrow_studio/desktop_frame.dart';
import 'package:window_manager/window_manager.dart';

import '../test/editor_draft_reselection_process_test.dart' as lifecycle;

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  if (!lifecycle.available || !Platform.isWindows) {
    throw StateError(
      'Windows lifecycle qualification requires all fixture paths',
    );
  }
  if (Platform.resolvedExecutable.toLowerCase().contains('flutter_tester')) {
    throw StateError(
      'This qualification requires the native Windows app engine',
    );
  }
  lifecycle.main(
    prepareWindow: (tester) async {
      await initializeDesktopFrame();
      await windowManager.setTitle(
        'Morrow draft qualification ${lifecycle.phase}',
      );
      await windowManager.setSize(const Size(1180, 900));
      await windowManager.show();
      await tester.pumpAndSettle();
      final bounds = await windowManager.getBounds();
      final visible = await windowManager.isVisible();
      if (!visible || bounds.width < 1100 || bounds.height < 800) {
        throw StateError('Original Windows qualification window is not ready');
      }
      final root = Directory(lifecycle.rootPath!);
      if (!root.isAbsolute) {
        throw StateError('Use an absolute isolated test root');
      }
      await root.create(recursive: true);
      final proof = File('${root.path}/window-${lifecycle.phase}.json');
      if (await proof.exists()) {
        throw StateError('Fresh phase evidence required');
      }
      await proof.writeAsString(
        jsonEncode({
          'pid': pid,
          'executable': Platform.resolvedExecutable,
          'phase': lifecycle.phase,
          'visible': visible,
          'window_width': bounds.width,
          'window_height': bounds.height,
          'physical_width': tester.view.physicalSize.width,
          'physical_height': tester.view.physicalSize.height,
          'device_pixel_ratio': tester.view.devicePixelRatio,
          'input': 'framework_injection',
          'physical_input_accepted': false,
          'lifecycle_result': 'PENDING_SEPARATE_ORIGINAL_RESULT',
        }),
        flush: true,
      );
    },
  );
}
