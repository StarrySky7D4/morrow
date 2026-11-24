import 'dart:io';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/oauth_accounts.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';

import 'changes_cursor_product_native_test.dart' show closeOwner, install;
import 'external_plugin_native_test.dart' show entireCatalog;

void main() {
  final env = Platform.environment;
  final root = env['MORROW_OAUTH_PRODUCT_EVIDENCE'],
      host = env['MORROW_WORKBENCH_HOST'],
      supervisor = env['MORROW_WORKBENCH_SUPERVISOR'],
      builtin = env['MORROW_WORKBENCH_PACKAGE'];
  for (final language in ['rust', 'c', 'cpp']) {
    test(
      '$language Windows workbench exposes account discovery and requires an approved service across restart',
      () async {
        final dir = Directory('$root/$language');
        expect(dir.existsSync(), isFalse);
        await dir.create(recursive: true);
        Future<RustWorkbench> open() => RustWorkbench.open(
          executable: host!,
          supervisorExecutable: supervisor!,
          package: builtin!,
          directory: dir,
          managed: true,
        );
        RustWorkbench? owner;
        try {
          var desk = owner = await open();
          expect(desk.supportsOAuthAccounts, isTrue);
          final frozen = File(
            'sdk/compat/transport-v1-rc1/$language-io.mplugin',
          ).absolute.path;
          final pkg = await install(desk, frozen, []);
          var catalog = await entireCatalog(desk);
          await desk.configureExternalIo(pkg, catalog.revision, [
            'http-request',
            'credential-use',
          ]);
          Future<void> verify() async {
            catalog = await entireCatalog(desk);
            final entry = catalog.entries.singleWhere((p) => p.id == pkg.id);
            final list = await desk.accountControl(
              entry,
              catalog.revision,
              const OAuthCommand(OAuthAction.services),
            );
            expect(list.services, isEmpty);
            await expectLater(
              desk.accountControl(
                entry,
                catalog.revision,
                OAuthCommand(
                  OAuthAction.login,
                  days: 7,
                  service: Uint8List.fromList(List.filled(32, 17)),
                  key: newOAuthKey(),
                ),
              ),
              throwsA(anything),
            );
          }

          await verify();
          await closeOwner(desk);
          owner = null;
          desk = owner = await open();
          await verify();
        } finally {
          if (owner != null) await closeOwner(owner);
        }
      },
      skip:
          !Platform.isWindows ||
          [root, host, supervisor, builtin].contains(null),
      timeout: const Timeout(Duration(minutes: 2)),
    );
  }
}
