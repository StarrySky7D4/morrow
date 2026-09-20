// Shared private fixture for actual Windows service-failure tests. The service
// guest is a two-request WAT fixture; business uses the real builtin Rust guest.
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/plugin_library.dart';
import 'package:morrow_studio/plugins/service_control.dart';
import 'package:morrow_studio/plugins/service_run_control.dart';
import 'package:morrow_studio/plugins/service_run_session.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';

import 'external_plugin_native_test.dart'
    show entireCatalog, removeTestDirectory;

String _hex(List<int> value) =>
    value.map((b) => b.toRadixString(16).padLeft(2, '0')).join();

class RealServiceFixture {
  RealServiceFixture._(this.directory, this.backend);
  static bool get available =>
      Platform.isWindows &&
      [
        'MORROW_WORKBENCH_HOST',
        'MORROW_WORKBENCH_PACKAGE',
        'MORROW_SERVICE_RUN_PACKAGE',
        'MORROW_SERVICE_RUN_PACKAGER',
      ].every(Platform.environment.containsKey);
  final Directory directory;
  final RustWorkbench backend;
  late final ServiceRunSession session = ServiceRunSession.forBackend(
    backend,
    backend,
  );
  IssuedServiceAuthentication? issued;
  ServerSocket? reservation;
  late StoredServiceConfig config;
  late StoredServiceAuthority publication;
  late PluginLibraryEntry plugin;
  late BigInt registry;
  late String address;

  static Future<RealServiceFixture> open({
    bool occupyPort = false,
    Future<RustWorkbench> Function(Directory)? openBackend,
  }) async {
    final dir = await Directory.systemTemp.createTemp(
      'morrow-external-service-fault-',
    );
    RealServiceFixture? fixture;
    try {
      final backend = openBackend != null
          ? await openBackend(dir)
          : await RustWorkbench.open(
              executable: Platform.environment['MORROW_WORKBENCH_HOST']!,
              package: Platform.environment['MORROW_WORKBENCH_PACKAGE']!,
              directory: dir,
              managed: true,
            );
      fixture = RealServiceFixture._(dir, backend);
      await fixture._prepare(occupyPort: occupyPort);
      return fixture;
    } catch (_) {
      if (fixture != null) {
        await fixture.close();
      } else {
        await removeTestDirectory(dir);
      }
      rethrow;
    }
  }

  Future<void> _select(String path, {bool enable = false}) async {
    final candidate = (await backend.inspectPlugin(path)).entries.single;
    await backend.importPlugin(
      path,
      candidate.digest,
      (await backend.pluginPage()).revision,
    );
    var catalog = await entireCatalog(backend);
    plugin = catalog.entries.singleWhere((e) => e.id == candidate.id);
    await backend.configureExternalIo(plugin, catalog.revision, [
      'http-listen',
      'http-publish',
    ]);
    catalog = await entireCatalog(backend);
    plugin = catalog.entries.singleWhere((e) => e.id == candidate.id);
    if (enable) {
      await backend.configureExternal(plugin, catalog.revision, [], true);
      catalog = await entireCatalog(backend);
      plugin = catalog.entries.singleWhere((e) => e.id == candidate.id);
    }
    registry = catalog.revision;
  }

  Future<void> _prepare({required bool occupyPort}) async {
    await _select(Platform.environment['MORROW_SERVICE_RUN_PACKAGE']!);
    issued = await backend.issueServiceAuthentication(
      reference: Uint8List(0),
      expectedRevision: BigInt.zero,
      principalId: 'alice',
      lifetimeDays: 1,
    );
    ServiceConfigUpdate update({StoredServiceConfig? previous}) =>
        ServiceConfigUpdate(
          id: previous?.id ?? '',
          expectedRevision: previous?.revision ?? BigInt.zero,
          registryRevision: registry,
          packageId: plugin.id,
          packageDigest: plugin.digest,
          service: 'application.service',
          handler: 'application.serve',
          retentionMs: BigInt.from(30000),
          principals: [
            ServicePrincipal(
              id: 'alice',
              authenticationReference: issued!.authority.reference,
              scopes: [],
            ),
          ],
        );
    final initial = await backend.saveServiceConfig(update());
    final output = await Directory(
      'build/service-run-routing',
    ).create(recursive: true);
    final bound =
        '${output.absolute.path}/fault-service-${DateTime.now().microsecondsSinceEpoch}.mplugin';
    final packaged = await Process.run(
      Platform.environment['MORROW_SERVICE_RUN_PACKAGER']!,
      [bound, _hex(initial.namespace)],
    );
    expect(packaged.exitCode, 0, reason: '${packaged.stderr}');
    await _select(bound, enable: true);
    config = await backend.saveServiceConfig(update(previous: initial));
    expect(config.namespace, initial.namespace);
    reservation = await ServerSocket.bind(InternetAddress.loopbackIPv4, 0);
    address = '127.0.0.1:${reservation!.port}';
    publication = await backend.saveServicePublication(
      ServicePublicationUpdate(
        reference: config.approvalReferences.single,
        expectedRevision: BigInt.zero,
        configRevision: config.revision,
        registryRevision: registry,
        packageId: plugin.id,
        lifetimeDays: 1,
        policy: ServicePublication(
          configId: config.id,
          configDigest: config.digest,
          listenAddress: address,
          tlsRequired: false,
          method: 'POST',
          path: '/api',
          queryPath: '/history',
        ),
      ),
    );
    if (!occupyPort) {
      await reservation!.close();
      reservation = null;
    }
    await backend.saveUiLocale('zh');
    await session.refresh();
    expect(session.canStart, isTrue);
  }

  ServiceRunRequest request({
    int submission = 31,
    int lifetime = 30000,
    int timeout = 1000,
    BigInt? registryRevision,
    BigInt? configRevision,
    int jobs = 64,
    int calls = 4,
  }) => ServiceRunRequest(
    submission: Uint8List.fromList(List.filled(32, submission)),
    configId: config.id,
    configDigest: config.digest,
    configRevision: configRevision ?? config.revision,
    publication: publication.reference,
    publicationRevision: publication.revision,
    packageId: plugin.id,
    packageDigest: plugin.digest,
    registryRevision: registryRevision ?? registry,
    lifetimeMs: lifetime,
    timeoutMs: timeout,
    maxJobs: BigInt.from(jobs),
    maxBytes: BigInt.from(4 * 1024 * 1024),
    maxCalls: calls,
    maxJobBytes: BigInt.from(1024 * 1024),
    maxTotalBytes: BigInt.from(4 * 1024 * 1024),
  );

  Future<ServiceRunSnapshot> observe(ServiceRunPhase phase) async {
    final until = DateTime.now().add(const Duration(seconds: 15));
    while (true) {
      await session.refresh();
      expect(session.trusted, isTrue, reason: '${session.notice}');
      final state = session.service;
      expect(state, isNotNull);
      if (state!.phase == phase) return state;
      if (state.phase == ServiceRunPhase.exited ||
          DateTime.now().isAfter(until)) {
        fail(
          'service did not reach $phase: ${state.phase}, ${state.bind}, ${state.listener}',
        );
      }
      await Future<void>.delayed(const Duration(milliseconds: 15));
    }
  }

  Future<String> post({bool second = false}) async {
    final target = Uri.parse('http://$address');
    final socket = await Socket.connect(
      target.host,
      target.port,
      timeout: const Duration(seconds: 2),
    );
    final body = second ? 'after' : 'before';
    final request = Uint8List.fromList([
      ...utf8.encode(
        'POST /api HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer ',
      ),
      ...issued!.token.bytes,
      ...utf8.encode(
        '\r\nIdempotency-Key: application-service-$body\r\nContent-Length: ${body.length}\r\nConnection: close\r\n\r\n$body',
      ),
    ]);
    try {
      socket.add(request);
      await socket.flush();
      final bytes = await socket
          .fold<List<int>>(<int>[], (all, chunk) => all..addAll(chunk))
          .timeout(const Duration(seconds: 5));
      return utf8.decode(bytes);
    } finally {
      request.fillRange(0, request.length, 0);
      socket.destroy();
    }
  }

  Future<void> close({bool observeBeforeClose = true}) async {
    await reservation?.close();
    reservation = null;
    issued?.dispose();
    try {
      if (observeBeforeClose) {
        await session.refresh();
        if (session.canStop) await session.stop();
        if (session.service != null &&
            session.service!.phase != ServiceRunPhase.exited) {
          await observe(ServiceRunPhase.exited);
        }
      }
    } finally {
      // Native close waits for actual exit even if observation or cleanup failed.
      await backend.close();
      session.dispose();
      await removeTestDirectory(directory);
    }
  }
}
