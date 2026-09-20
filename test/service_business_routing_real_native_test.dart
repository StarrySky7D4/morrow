// Real original-owner service execution: Rust host + builtin Rust workbench
// guest, and a test-only WAT service guest matching two complete request frames.
// No SDK compatibility artifact is modified or repackaged by this test.
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/main.dart' show Idea;
import 'package:morrow_studio/plugins/io_task_models.dart';
import 'package:morrow_studio/plugins/service_control.dart';
import 'package:morrow_studio/plugins/service_run_control.dart';
import 'package:morrow_studio/plugins/service_run_session.dart';
import 'package:morrow_studio/plugins/workbench_backend.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';

import 'external_plugin_native_test.dart'
    show entireCatalog, removeTestDirectory;

String _hex(List<int> value) =>
    value.map((b) => b.toRadixString(16).padLeft(2, '0')).join();
Uint8List _identity(int value) => Uint8List.fromList(List.filled(32, value));

Future<ServiceRunSnapshot> _waitFor(
  RustWorkbench backend,
  Uint8List task,
  ServiceRunPhase phase,
) async {
  final deadline = DateTime.now().add(const Duration(seconds: 20));
  while (true) {
    final current = await backend.serviceRunStatus(key: task);
    if (current.phase == phase) return current;
    if (current.phase == ServiceRunPhase.exited ||
        DateTime.now().isAfter(deadline)) {
      fail(
        'service did not reach $phase: ${current.phase}, bind ${current.bind}, listener ${current.listener}',
      );
    }
    await Future<void>.delayed(const Duration(milliseconds: 10));
  }
}

Future<void> _post(
  String address,
  Uint8List token,
  String key,
  String body,
  String expected,
) async {
  final target = Uri.parse('http://$address');
  final socket = await Socket.connect(
    target.host,
    target.port,
    timeout: const Duration(seconds: 5),
  );
  // Keep the synthetic bearer in mutable bytes, avoiding an immutable token
  // String while exercising the actual HTTP transport and authentication.
  final request = Uint8List.fromList([
    ...utf8.encode(
      'POST /api HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer ',
    ),
    ...token,
    ...utf8.encode(
      '\r\nIdempotency-Key: $key\r\nContent-Length: ${utf8.encode(body).length}\r\nConnection: close\r\n\r\n$body',
    ),
  ]);
  try {
    socket.add(request);
    await socket.flush();
    final bytes = await socket
        .fold<List<int>>(<int>[], (all, chunk) => all..addAll(chunk))
        .timeout(const Duration(seconds: 15));
    final text = utf8.decode(bytes);
    expect(text, startsWith('HTTP/1.1 202 '));
    expect(text.split('\r\n\r\n').last, expected);
  } finally {
    request.fillRange(0, request.length, 0);
    socket.destroy();
  }
}

Future<ServiceRunSnapshot> _observeSession(
  ServiceRunSession session,
  ServiceRunPhase phase,
) async {
  final deadline = DateTime.now().add(const Duration(seconds: 20));
  while (true) {
    await session.refresh();
    expect(session.trusted, isTrue, reason: '${session.notice}');
    final current = session.service!;
    if (current.phase == phase) return current;
    if (current.phase == ServiceRunPhase.exited ||
        DateTime.now().isAfter(deadline)) {
      fail('Session did not observe $phase: ${current.phase}');
    }
    await Future<void>.delayed(const Duration(milliseconds: 10));
  }
}

void main() {
  final executable = Platform.environment['MORROW_WORKBENCH_HOST'];
  final builtin = Platform.environment['MORROW_WORKBENCH_PACKAGE'];
  final fixture = Platform.environment['MORROW_SERVICE_RUN_PACKAGE'];
  final packager = Platform.environment['MORROW_SERVICE_RUN_PACKAGER'];
  final unavailable =
      !Platform.isWindows ||
      executable == null ||
      builtin == null ||
      fixture == null ||
      packager == null;
  test(
    'real service HTTP and ordinary business calls share the original owner until explicit reclaim',
    () async {
      final directory = await Directory.systemTemp.createTemp(
        'morrow-external-service-business-',
      );
      RustWorkbench? backend;
      Uint8List? task;
      IssuedServiceAuthentication? issued;
      ServerSocket? reservation;
      Future<RustWorkbench> open() => RustWorkbench.open(
        executable: executable!,
        package: builtin!,
        directory: directory,
        managed: true,
      );
      try {
        backend = await open();
        expect(backend.writable, isTrue);
        final candidate = (await backend.inspectPlugin(
          fixture!,
        )).entries.single;
        await backend.importPlugin(
          fixture,
          candidate.digest,
          (await backend.pluginPage()).revision,
        );
        var catalog = await entireCatalog(backend);
        var entry = catalog.entries.singleWhere((e) => e.id == candidate.id);
        await backend.configureExternalIo(entry, catalog.revision, [
          'http-listen',
          'http-publish',
        ]);
        catalog = await entireCatalog(backend);
        entry = catalog.entries.singleWhere((e) => e.id == candidate.id);
        issued = await backend.issueServiceAuthentication(
          reference: Uint8List(0),
          expectedRevision: BigInt.zero,
          principalId: 'alice',
          lifetimeDays: 1,
        );
        final principals = [
          ServicePrincipal(
            id: 'alice',
            authenticationReference: issued.authority.reference,
            scopes: [],
          ),
        ];
        ServiceConfigUpdate update({StoredServiceConfig? previous}) =>
            ServiceConfigUpdate(
              id: previous?.id ?? '',
              expectedRevision: previous?.revision ?? BigInt.zero,
              registryRevision: catalog.revision,
              packageId: entry.id,
              packageDigest: entry.digest,
              service: 'application.service',
              handler: 'application.serve',
              retentionMs: BigInt.from(30000),
              principals: principals,
            );
        final initialConfig = await backend.saveServiceConfig(update());
        // The application creates the namespace. Bind only this private fixture
        // to it, then perform an explicit digest change and fresh approvals.
        final fixtureOutput = await Directory(
          'build/service-run-routing',
        ).create(recursive: true);
        final boundPackage =
            '${fixtureOutput.absolute.path}/bound-service-${DateTime.now().microsecondsSinceEpoch}.mplugin';
        final packaged = await Process.run(packager!, [
          boundPackage,
          _hex(initialConfig.namespace),
        ]);
        expect(packaged.exitCode, 0, reason: '${packaged.stderr}');
        final bound = (await backend.inspectPlugin(
          boundPackage,
        )).entries.single;
        expect(bound.id, candidate.id);
        expect(bound.digest, isNot(candidate.digest));
        await backend.importPlugin(
          boundPackage,
          bound.digest,
          (await backend.pluginPage()).revision,
        );
        catalog = await entireCatalog(backend);
        entry = catalog.entries.singleWhere((e) => e.id == candidate.id);
        await backend.configureExternalIo(entry, catalog.revision, [
          'http-listen',
          'http-publish',
        ]);
        catalog = await entireCatalog(backend);
        entry = catalog.entries.singleWhere((e) => e.id == candidate.id);
        await backend.configureExternal(entry, catalog.revision, [], true);
        catalog = await entireCatalog(backend);
        entry = catalog.entries.singleWhere((e) => e.id == candidate.id);
        expect(entry.enabled, isTrue);
        final config = await backend.saveServiceConfig(
          update(previous: initialConfig),
        );
        expect(config.namespace, initialConfig.namespace);
        reservation = await ServerSocket.bind(InternetAddress.loopbackIPv4, 0);
        final address = '127.0.0.1:${reservation.port}';
        final publication = await backend.saveServicePublication(
          ServicePublicationUpdate(
            reference: config.approvalReferences.single,
            expectedRevision: BigInt.zero,
            configRevision: config.revision,
            registryRevision: catalog.revision,
            packageId: entry.id,
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
        await reservation.close();
        reservation = null;
        await backend.saveUiLocale('zh');
        final session = ServiceRunSession.forBackend(backend, backend);
        await session.refresh();
        expect(session.canStart, isTrue);
        await session.start(
          ServiceRunRequest(
            submission: _identity(17),
            configId: config.id,
            configDigest: config.digest,
            configRevision: config.revision,
            publication: publication.reference,
            publicationRevision: publication.revision,
            packageId: entry.id,
            packageDigest: entry.digest,
            registryRevision: catalog.revision,
            lifetimeMs: 120000,
            maxJobs: BigInt.from(64),
            maxBytes: BigInt.from(4 * 1024 * 1024),
            maxCalls: 4,
            maxJobBytes: BigInt.from(1024 * 1024),
            maxTotalBytes: BigInt.from(4 * 1024 * 1024),
          ),
        );
        expect(session.notice, isNull);
        task = session.task!.key!;
        final running = await _observeSession(session, ServiceRunPhase.running);
        expect(running.address, address);
        expect(session.canStop, isTrue);
        expect(session.canAcknowledge, isFalse);
        await _post(
          address,
          issued.token.bytes,
          'application-service-before',
          'before',
          'executed-before',
        );

        // These are ordinary public business methods, with no manual command
        // envelope. Their success proves the native client's automatic routing.
        expect((await backend.ioStatus()).storage, IoStoragePhase.running);
        expect(backend.writable, isTrue);
        expect(await backend.readUiLocale(), 'zh');
        await backend.saveUiLocale('en');
        final created = await backend.apply(
          PluginAction.create,
          Idea(
            'During service',
            'created by original Rust guest',
            '灵感',
            Idea.icons[0],
            const Color(0xff8866aa),
            id: 'service-owned-card',
          ),
        );
        expect(created.id, 'service-owned-card');
        final loaded = (await backend.load()).singleWhere(
          (e) => e.id == created.id,
        );
        expect(loaded.description, 'created by original Rust guest');
        final edited = await backend.apply(
          PluginAction.edit,
          Idea(
            'Edited during service',
            'edited by original Rust guest',
            '灵感',
            Idea.icons[0],
            const Color(0xff8866aa),
            id: created.id,
          ),
        );
        expect(edited.description, 'edited by original Rust guest');
        expect(await backend.readUiLocale(), 'en');
        expect((await backend.serviceRunStatus(key: task)).task.key, task);
        await _post(
          address,
          issued.token.bytes,
          'application-service-after',
          'after',
          'executed-after',
        );

        await session.stop();
        final exited = await _observeSession(session, ServiceRunPhase.exited);
        expect(exited.task.storage, IoStoragePhase.reclaimed);
        expect(exited.task.exit!.execution, IoJobError.none);
        expect(exited.task.exit!.maintenance, IoJobError.none);
        expect(
          (await backend.load())
              .singleWhere((e) => e.id == created.id)
              .description,
          'edited by original Rust guest',
        );
        expect(await backend.readUiLocale(), 'en');
        expect(session.canAcknowledge, isTrue);
        await session.acknowledge();
        expect(session.canStart, isTrue);
        expect(session.service, isNull);
        expect(session.history.single.outcomeUnknown, isFalse);
        task = null;
        expect((await backend.ioStatus()).storage, IoStoragePhase.local);
        await backend.close();
        backend = await open();
        expect(await backend.readUiLocale(), 'en');
        expect(
          (await backend.load()).singleWhere((e) => e.id == created.id).title,
          'Edited during service',
        );
        expect((await backend.ioStatus()).storage, IoStoragePhase.local);
      } finally {
        issued?.dispose();
        await reservation?.close();
        if (backend != null && task != null) {
          try {
            await backend.cancelIo(task);
            await _waitFor(backend, task, ServiceRunPhase.exited);
          } catch (_) {
            /* close still waits for real worker exit */
          }
        }
        await backend?.close();
        await removeTestDirectory(directory);
      }
    },
    skip: unavailable,
    timeout: const Timeout(Duration(minutes: 3)),
  );
}
