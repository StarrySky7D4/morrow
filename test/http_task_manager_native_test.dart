import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:ui' as ui;
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:morrow_studio/plugins/endpoint_control.dart';
import 'package:morrow_studio/plugins/http_task_manager.dart';
import 'package:morrow_studio/plugins/plugin_library.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';
import 'external_plugin_native_test.dart'
    show entireCatalog, removeTestDirectory;
import 'plugin_tools_native_test.dart' show pumpHost;

void main() {
  final exe = Platform.environment['MORROW_WORKBENCH_HOST'];
  final bundle = Platform.environment['MORROW_WORKBENCH_PACKAGE'];
  final pluginFile = Platform.environment['MORROW_HTTP_FORWARD_PACKAGE'];
  testWidgets(
    'actual task form submits once through Rust guest and protected credential, survives settings unmount and confirms reclaimed owner',
    (tester) async {
      Directory? directory;
      RustWorkbench? backend;
      HttpServer? server;
      StreamSubscription<HttpRequest>? serving;
      PluginLibraryPage? catalog;
      final requests = <String>[];
      Object? serverError;
      final boundaryKey = GlobalKey();
      final capture = Platform.environment['MORROW_HTTP_UI_CAPTURE'];
      try {
        await tester.binding.setSurfaceSize(const Size(900, 1400));
        await tester.runAsync(() async {
          if (capture != null) {
            final font = FontLoader('UiPreview')
              ..addFont(
                File(
                  'C:/Windows/Fonts/segoeui.ttf',
                ).readAsBytes().then(ByteData.sublistView),
              );
            await font.load();
            final icons = FontLoader('MaterialIcons')
              ..addFont(
                File(
                  'C:/flutter/bin/cache/artifacts/material_fonts/MaterialIcons-Regular.otf',
                ).readAsBytes().then(ByteData.sublistView),
              );
            await icons.load();
          }
          directory = await Directory.systemTemp.createTemp(
            'morrow-external-http-ui-',
          );
          server = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
          serving = server!.listen((request) async {
            try {
              requests.add(request.uri.toString());
              expect(request.method, 'POST');
              expect(
                request.headers.value('authorization'),
                'Bearer ui-test-secret',
              );
              expect(
                await utf8.decoder.bind(request).join(),
                'from the real form',
              );
              request.response.statusCode = 422;
              request.response.headers.add('x-repeat', 'first');
              request.response.headers.add('x-repeat', 'second');
              request.response.write('A real response from the local API');
              await request.response.close();
            } catch (error) {
              serverError = error;
            }
          });
          backend = await RustWorkbench.open(
            executable: exe!,
            package: bundle!,
            directory: directory!,
            managed: true,
          );
          final preview = await backend!.inspectPlugin(pluginFile!);
          final candidate = preview.entries.single;
          expect(candidate.ioHandlers, ['morrow.http.forward.v1']);
          await backend!.importPlugin(
            pluginFile,
            candidate.digest,
            preview.revision,
          );
          catalog = await entireCatalog(backend!);
          var plugin = catalog!.entries.singleWhere(
            (e) => e.id == candidate.id,
          );
          await backend!.configureExternalIo(plugin, catalog!.revision, [
            'http-request',
            'credential-use',
          ]);
          catalog = await entireCatalog(backend!);
          plugin = catalog!.entries.singleWhere((e) => e.id == candidate.id);
          await backend!.configureExternal(plugin, catalog!.revision, [], true);
          catalog = await entireCatalog(backend!);
          final credential = await backend!.saveCredential(
            reference: Uint8List(0),
            expectedRevision: BigInt.zero,
            headerName: 'authorization',
            headerValue: 'Bearer ui-test-secret',
            lifetimeDays: 3,
          );
          await backend!.saveEndpoint(
            reference: Uint8List(0),
            expectedRevision: BigInt.zero,
            registryRevision: catalog!.revision,
            lifetimeDays: 1,
            policy: EndpointPolicy(
              packageId: plugin.id,
              packageDigest: plugin.digest,
              origin: 'http://127.0.0.1:${server!.port}',
              profile: 2,
              methods: ['POST'],
              credentialReference: credential.reference,
              rootCertificate: Uint8List(0),
              maxRequestBytes: 65536,
              maxResponseBytes: 65536,
              maxHeaderBytes: 16384,
              maxConcurrent: 1,
              timeoutMs: 10000,
              maxFrameBytes: 131072,
            ),
          );
        });
        Widget panel() => MaterialApp(
          locale: const Locale('en'),
          localizationsDelegates: AppLocalizations.localizationsDelegates,
          supportedLocales: AppLocalizations.supportedLocales,
          theme: ThemeData(fontFamily: capture == null ? null : 'UiPreview'),
          home: Scaffold(
            backgroundColor: const Color(0xfff6f4ee),
            body: RepaintBoundary(
              key: boundaryKey,
              child: ColoredBox(
                color: const Color(0xfff6f4ee),
                child: SingleChildScrollView(
                  child: Padding(
                    padding: const EdgeInsets.all(20),
                    child: HttpTaskManager(
                      backend: backend!,
                      endpointBackend: backend!,
                      plugins: catalog!.entries,
                      registryRevision: catalog!.revision,
                      ink: const Color(0xff242424),
                      muted: const Color(0xff66645f),
                      line: const Color(0xffccc8be),
                      radius: BorderRadius.circular(14),
                    ),
                  ),
                ),
              ),
            ),
          ),
        );
        Finder key(String name) => find.byKey(ValueKey('http-task-$name'));
        bool enabled(String name) =>
            key(name).evaluate().isNotEmpty &&
            tester.widget<OutlinedButton>(key(name)).onPressed != null;
        Future<void> tap(String name) async {
          await tester.ensureVisible(key(name));
          await tester.tap(key(name));
          await tester.pump();
        }

        await tester.pumpWidget(panel());
        await pumpHost(tester, () => enabled('start'));
        await tester.enterText(key('target'), '/submit?mode=preview');
        await tester.enterText(key('body'), 'from the real form');
        await tester.enterText(key('headers'), 'X-Client: Morrow');
        await pumpHost(tester, () => enabled('start'));
        await tap('start');
        // Advance the widget's read-only poll timer while the real child process runs.
        final limit = DateTime.now().add(const Duration(seconds: 20));
        while (!enabled('read')) {
          expect(DateTime.now().isBefore(limit), isTrue);
          await tester.runAsync(
            () => Future<void>.delayed(const Duration(milliseconds: 20)),
          );
          // Avoid repeatedly landing on the next poll's busy edge and starving
          // the explicit read until the real worker's delivery deadline expires.
          await tester.pump(const Duration(milliseconds: 20));
        }
        await tap('read');
        await pumpHost(tester, () => key('result').evaluate().isNotEmpty);
        expect(serverError, isNull);
        expect(key('result-body'), findsOneWidget);
        expect(
          find.textContaining('A real response from the local API'),
          findsWidgets,
        );
        expect(find.textContaining('422'), findsWidgets);
        expect(find.textContaining('ui-test-secret'), findsNothing);
        expect(serverError, isNull);
        expect(requests, ['/submit?mode=preview']);
        await tester.pumpWidget(const SizedBox.shrink());
        await tester.pumpWidget(panel());
        await tester.pump();
        expect(
          find.textContaining('A real response from the local API'),
          findsWidgets,
        );
        expect(requests, hasLength(1));
        while (!enabled('ack')) {
          expect(DateTime.now().isBefore(limit), isTrue);
          await tester.runAsync(
            () => Future<void>.delayed(const Duration(milliseconds: 20)),
          );
          await tester.pump(const Duration(milliseconds: 20));
        }
        if (capture != null) {
          await pumpHost(
            tester,
            () => enabled('ack') && enabled('endpoints-refresh'),
          );
          await tester.ensureVisible(key('result'));
          await tester.pump(const Duration(milliseconds: 300));
          final render =
              boundaryKey.currentContext!.findRenderObject()!
                  as RenderRepaintBoundary;
          await tester.runAsync(() async {
            final image = await render.toImage();
            final png = await image.toByteData(format: ui.ImageByteFormat.png);
            await File(capture).writeAsBytes(png!.buffer.asUint8List());
            image.dispose();
          });
        }
        await pumpHost(tester, () => enabled('ack'));
        await tap('ack');
        await pumpHost(tester, () => enabled('start'));
        expect(requests, hasLength(1));
        await tester.runAsync(() async {
          expect((await backend!.ioStatus()).key, isNull);
        });
      } finally {
        await tester.pumpWidget(const SizedBox.shrink());
        await tester.binding.setSurfaceSize(null);
        await tester.runAsync(() async {
          await backend?.close();
          await serving?.cancel();
          await server?.close(force: true);
          if (directory != null) await removeTestDirectory(directory!);
        });
      }
    },
    skip:
        !Platform.isWindows ||
        exe == null ||
        bundle == null ||
        pluginFile == null,
    timeout: const Timeout(Duration(minutes: 3)),
  );
}
