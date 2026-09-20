import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/endpoint_control.dart';
import 'package:morrow_studio/plugins/io_task_control.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';
import 'external_plugin_native_test.dart'
    show entireCatalog, removeTestDirectory;

void main() {
  final executable = Platform.environment['MORROW_WORKBENCH_HOST'];
  final builtin = Platform.environment['MORROW_WORKBENCH_PACKAGE'];
  final fixture = Platform.environment['MORROW_HTTP_FORWARD_PACKAGE'];
  test(
    'real Rust HTTP task crosses private transport and close cancels active HTTP before reopening the original library',
    () async {
      final directory = await Directory.systemTemp.createTemp(
        'morrow-external-http-task-',
      );
      final server = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
      final entered = Completer<void>(), release = Completer<void>();
      var calls = 0;
      final serving = server.listen((request) async {
        calls++;
        expect(request.method, 'POST');
        expect(await utf8.decoder.bind(request).join(), 'native body');
        if (request.uri.path == '/hold') {
          entered.complete();
          await release.future;
        }
        request.response.statusCode = 404;
        request.response.headers.add('x-repeat', 'one');
        request.response.headers.add('x-repeat', 'two');
        request.response.write('forwarded');
        await request.response.close();
      });
      RustWorkbench? backend;
      try {
        backend = await RustWorkbench.open(
          executable: executable!,
          package: builtin!,
          directory: directory,
          managed: true,
        );
        final preview = await backend.inspectPlugin(fixture!);
        final candidate = preview.entries.single;
        await backend.importPlugin(fixture, candidate.digest, preview.revision);
        var catalog = await entireCatalog(backend);
        var plugin = catalog.entries.singleWhere((e) => e.id == candidate.id);
        await backend.configureExternalIo(plugin, catalog.revision, [
          'http-request',
        ]);
        catalog = await entireCatalog(backend);
        plugin = catalog.entries.singleWhere((e) => e.id == candidate.id);
        await backend.configureExternal(plugin, catalog.revision, [], true);
        catalog = await entireCatalog(backend);
        final endpoint = await backend.saveEndpoint(
          reference: Uint8List(0),
          expectedRevision: BigInt.zero,
          registryRevision: catalog.revision,
          lifetimeDays: 1,
          policy: EndpointPolicy(
            packageId: plugin.id,
            packageDigest: plugin.digest,
            origin: 'http://127.0.0.1:${server.port}',
            profile: 2,
            methods: ['POST'],
            credentialReference: Uint8List(0),
            rootCertificate: Uint8List(0),
            maxRequestBytes: 65536,
            maxResponseBytes: 65536,
            maxHeaderBytes: 16384,
            maxConcurrent: 1,
            timeoutMs: 12000,
            maxFrameBytes: 131072,
          ),
        );
        HttpTaskRequest request(int id, String target) => HttpTaskRequest(
          submission: Uint8List(32)..[0] = id,
          endpoint: endpoint.reference,
          endpointRevision: endpoint.revision,
          packageDigest: plugin.digest,
          registryRevision: catalog.revision,
          method: 'POST',
          target: target,
          headers: [],
          body: Uint8List.fromList(utf8.encode('native body')),
          timeoutMs: 15000,
        );
        final started = await backend.startHttp(request(1, '/request'));
        expect(started.submission, request(1, '/request').submission);
        final key = started.key!;
        await expectLater(
          backend.startHttp(request(1, '/request')),
          throwsStateError,
        );
        final deadline = DateTime.now().add(const Duration(seconds: 10));
        while ((await backend.pollIo(key)).delivery.name != 'ready') {
          expect(DateTime.now().isBefore(deadline), isTrue);
          await Future<void>.delayed(const Duration(milliseconds: 10));
        }
        final read = await backend.readIo(key);
        expect(read.result!.cancelled, isFalse);
        expect(read.result!.unknown, isFalse);
        expect(read.result!.http!.httpStatus, 404);
        expect(utf8.decode(read.result!.http!.body), 'forwarded');
        expect(read.result!.calls, BigInt.one);
        while ((await backend.pollIo(key)).exit == null) {
          expect(DateTime.now().isBefore(deadline), isTrue);
          await Future<void>.delayed(const Duration(milliseconds: 10));
        }
        await backend.acknowledgeIo(key);
        await expectLater(
          backend.startHttp(request(1, '/request')),
          throwsStateError,
        );
        expect(calls, 1);
        await backend.startHttp(request(2, '/hold'));
        await entered.future.timeout(const Duration(seconds: 5));
        // The real HTTP adapter cooperatively cancels its socket on revocation;
        // it need not wait for the server to finish handling the request.
        await backend.close().timeout(const Duration(seconds: 10));
        expect(await backend.process.exitCode, 0);
        release.complete();
        expect(calls, 2);
        backend = await RustWorkbench.open(
          executable: executable,
          package: builtin,
          directory: directory,
          managed: true,
        );
        expect((await backend.ioStatus()).key, isNull);
        expect(calls, 2);
      } finally {
        if (!release.isCompleted) release.complete();
        await backend?.close();
        await serving.cancel();
        await server.close(force: true);
        await removeTestDirectory(directory);
      }
    },
    skip:
        !Platform.isWindows ||
        executable == null ||
        builtin == null ||
        fixture == null,
    timeout: const Timeout(Duration(minutes: 2)),
  );
}
