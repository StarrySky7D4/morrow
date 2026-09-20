import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:morrow_studio/plugins/plugin_library.dart';
import 'package:morrow_studio/plugins/service_manager.dart';
import 'package:morrow_studio/plugins/service_session.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';

import 'external_plugin_native_test.dart'
    show entireCatalog, removeTestDirectory;
import 'plugin_tools_native_test.dart' show pumpHost;

String hex(List<int> value) =>
    value.map((b) => b.toRadixString(16).padLeft(2, '0')).join();

void main() {
  final executable = Platform.environment['MORROW_WORKBENCH_HOST'];
  final builtin = Platform.environment['MORROW_WORKBENCH_PACKAGE'];
  final fixture = Platform.environment['MORROW_SERVICE_ADMIN_PACKAGE'];
  final unavailable =
      !Platform.isWindows ||
      executable == null ||
      builtin == null ||
      fixture == null;
  testWidgets(
    'service forms persist original-store config auth and publication without listening',
    (tester) async {
      Directory? directory;
      RustWorkbench? backend;
      ServerSocket? occupied;
      late PluginLibraryPage catalog;
      final capture = GlobalKey();
      final output = Platform.environment['MORROW_SERVICE_UI_CAPTURE'];
      tester.view.physicalSize = const Size(1100, 1200);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      Future<void> tap(String key) async {
        final target = find.byKey(ValueKey(key));
        await tester.ensureVisible(target);
        await tester.tap(target);
        await tester.pump();
      }

      Future<void> enter(String key, String value) async {
        final target = find.byKey(ValueKey(key));
        await tester.ensureVisible(target);
        await tester.enterText(target, value);
      }

      Widget page(Locale locale) => MaterialApp(
        locale: locale,
        theme: ThemeData(fontFamily: output == null ? null : 'ServicePreview'),
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        supportedLocales: AppLocalizations.supportedLocales,
        home: Scaffold(
          body: RepaintBoundary(
            key: capture,
            child: Material(
              color: const Color(0xfff5fafb),
              child: SingleChildScrollView(
                child: Padding(
                  padding: const EdgeInsets.all(24),
                  child: ServiceManager(
                    backend: backend!,
                    plugins: catalog.entries,
                    registryRevision: catalog.revision,
                    ink: const Color(0xff273d4a),
                    muted: const Color(0xff617785),
                    line: const Color(0xffbbd0d7),
                    radius: BorderRadius.circular(16),
                  ),
                ),
              ),
            ),
          ),
        ),
      );
      try {
        await tester.runAsync(() async {
          if (output != null) {
            await (FontLoader('ServicePreview')..addFont(
                  File(
                    'C:/Windows/Fonts/msyh.ttc',
                  ).readAsBytes().then(ByteData.sublistView),
                ))
                .load();
            await (FontLoader('MaterialIcons')..addFont(
                  File(
                    'C:/flutter/bin/cache/artifacts/material_fonts/MaterialIcons-Regular.otf',
                  ).readAsBytes().then(ByteData.sublistView),
                ))
                .load();
          }
          directory = await Directory.systemTemp.createTemp(
            'morrow-external-service-form-',
          );
          occupied = await ServerSocket.bind(InternetAddress.loopbackIPv4, 0);
          backend = await RustWorkbench.open(
            executable: executable!,
            package: builtin!,
            directory: directory!,
            managed: true,
          );
          final candidate = (await backend!.inspectPlugin(
            fixture!,
          )).entries.single;
          await backend!.importPlugin(
            fixture,
            candidate.digest,
            (await backend!.pluginPage()).revision,
          );
          catalog = await entireCatalog(backend!);
          await backend!.configureExternalIo(
            catalog.entries.singleWhere((e) => e.id == candidate.id),
            catalog.revision,
            ['http-listen', 'http-publish'],
          );
          catalog = await entireCatalog(backend!);
        });
        final session = ServiceSession.forBackend(backend!);
        await tester.pumpWidget(page(const Locale('en')));
        await pumpHost(tester, () => session.canWrite);
        await tap('service-new-auth');
        await enter('service-auth-principal', 'native-ui-client');
        await enter('service-auth-days', '1');
        await tap('service-auth-save');
        await pumpHost(
          tester,
          () => session.canWrite && session.issued != null,
        );
        final issued = session.issued!;
        final auth = issued.authority;
        expect(issued.token.bytes.any((v) => v != 0), isTrue);
        expect(find.byKey(const ValueKey('service-token')), findsOneWidget);
        await tap('service-token-clear');
        expect(issued.token.isDisposed, isTrue);
        expect(issued.token.bytes, everyElement(0));

        await tap('service-new-config');
        await enter('service-service', 'native.ui.service');
        await enter('service-retention-ms', '86400000');
        await tap('service-principal-${hex(auth.reference)}');
        await tap('service-config-save');
        await pumpHost(
          tester,
          () => session.canWrite && session.configs.isNotEmpty,
        );
        final config = session.configs.single;
        expect(config.principals.single.id, 'native-ui-client');
        expect(config.handler, 'morrow.service.admin.test.v1');
        expect(config.principals.single.scopes, isEmpty);

        await tap('service-config-publish-${config.id}');
        await enter(
          'service-publication-address',
          '127.0.0.1:${occupied!.port}',
        );
        await enter('service-publication-path', '/invoke');
        await enter('service-publication-query', '/result');
        await enter('service-publication-days', '1');
        await tap('service-publication-save');
        await pumpHost(
          tester,
          () => session.canWrite && session.authorities.any((a) => a.kind == 2),
        );
        final publication = session.authorities.singleWhere((a) => a.kind == 2);
        expect(publication.publication!.configDigest, config.digest);
        expect(publication.expiresMs, auth.expiresMs);
        expect(
          publication.publication!.listenAddress,
          '127.0.0.1:${occupied!.port}',
        );
        // Saving on an already occupied port succeeds: no listener was opened.
        await tester.pumpWidget(page(const Locale('zh')));
        await tester.pumpAndSettle();
        expect(tester.takeException(), isNull);
        if (output != null) {
          await tester.ensureVisible(
            find.byKey(const ValueKey('service-new-config')),
          );
          await tester.pumpAndSettle();
          await tester.runAsync(() async {
            final boundary =
                capture.currentContext!.findRenderObject()!
                    as RenderRepaintBoundary;
            final image = await boundary.toImage();
            try {
              final data = await image.toByteData(
                format: ui.ImageByteFormat.png,
              );
              await File(output).writeAsBytes(data!.buffer.asUint8List());
            } finally {
              image.dispose();
            }
          });
        }
        await tap('service-auth-rotate-${hex(auth.reference)}');
        await enter('service-auth-days', '2');
        await tap('service-auth-save');
        await pumpHost(
          tester,
          () => session.canWrite && session.issued != null,
        );
        expect(session.issued!.authority.reference, auth.reference);
        expect(session.issued!.authority.revision, BigInt.two);
        await tap('service-token-clear');
        await tap('service-config-edit-${config.id}');
        await enter('service-retention-ms', '172800000');
        await tap('service-config-save');
        await pumpHost(
          tester,
          () =>
              session.canWrite && session.configs.single.revision == BigInt.two,
        );
        final changed = session.configs.single;
        expect(changed.namespace, config.namespace);
        expect(
          changed.approvalReferences.single,
          config.approvalReferences.single,
        );
        expect(changed.digest, isNot(config.digest));
        await tap('service-publication-edit-${hex(publication.reference)}');
        await tap('service-publication-save');
        await pumpHost(
          tester,
          () =>
              session.canWrite &&
              session.authorities.singleWhere((a) => a.kind == 2).revision ==
                  BigInt.two,
        );
        expect(
          session.authorities
              .singleWhere((a) => a.kind == 2)
              .publication!
              .configDigest,
          changed.digest,
        );
        await tap('service-authority-disable-${hex(publication.reference)}');
        await pumpHost(
          tester,
          () =>
              session.canWrite &&
              session.authorities.singleWhere((a) => a.kind == 2).disabled,
        );
        await tap('service-authority-disable-${hex(auth.reference)}');
        await pumpHost(
          tester,
          () =>
              session.canWrite && session.authorities.every((a) => a.disabled),
        );
        await tap('service-config-disable-${config.id}');
        await pumpHost(
          tester,
          () => session.canWrite && session.configs.single.disabled,
        );
        final disabled = session.configs.single;
        await tester.pumpWidget(const SizedBox.shrink());
        await tester.runAsync(() async {
          await backend!.close();
          backend = await RustWorkbench.open(
            executable: executable!,
            package: builtin!,
            directory: directory!,
            managed: true,
          );
          expect(
            (await backend!.serviceConfigPage()).configs.single.digest,
            disabled.digest,
          );
          expect(
            (await backend!.serviceAuthorityPage()).records
                .singleWhere((a) => a.kind == 2)
                .reference,
            publication.reference,
          );
          expect(
            (await entireCatalog(
              backend!,
            )).entries.where((e) => !e.builtin).every((e) => !e.enabled),
            isTrue,
          );
          expect(
            (await backend!.serviceAuthorityPage()).records.every(
              (a) => a.disabled,
            ),
            isTrue,
          );
        });
      } finally {
        await tester.pumpWidget(const SizedBox.shrink());
        await tester.runAsync(() async {
          await backend?.close();
          await occupied?.close();
          if (directory != null) await removeTestDirectory(directory!);
        });
      }
    },
    skip: unavailable,
    timeout: const Timeout(Duration(minutes: 3)),
  );
}
