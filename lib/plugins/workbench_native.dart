import 'editor_session.dart';
import 'plugin_tools.dart';
import 'plugin_library.dart';
import 'credential_manager.dart';
import 'endpoint_control.dart';
import 'service_control.dart';
import 'service_codec_native.dart';
import 'service_run_control.dart';
import 'service_run_codec_native.dart';
import 'io_task_control.dart';
import 'io_task_codec_native.dart';
import 'host_request.dart';
import 'package:morrow_plugin_ui/online.dart';
import 'studio_native.dart';
import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:math' show Random;
import 'dart:typed_data';
import 'package:crypto/crypto.dart';
import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:flutter/material.dart' show Color;
import '../main.dart' show Idea;
import '../attachments/attachment.dart';
import '../media/texture_source.dart';
import 'workbench_backend.dart';
import 'generated/host.capnp.dart' as host;
import 'generated/workbench.capnp.dart' as wire;
import 'generated/identity.dart' as contract;

part 'service_business_routing_native.dart';

class RustWorkbench
    implements
        WorkbenchBackend,
        WorkbenchProtectionBackup,
        WorkbenchPluginControl,
        ExternalPluginControl,
        WorkbenchCredentialControl,
        WorkbenchEndpointControl,
        WorkbenchServiceControl,
        WorkbenchServiceRunControl,
        WorkbenchIoTaskControl,
        WorkbenchEditorSupport {
  RustWorkbench._(this.process, this.cache) {
    process.stdout.listen(_receive, onError: _fail, onDone: _ended);
    _stderrDone = process.stderr.listen((bytes) {
      final remaining = 4096 - _stderr.length;
      if (remaining > 0) _stderr.addAll(bytes.take(remaining));
    }).asFuture<void>();
  }
  final _stderr = <int>[];
  late final Future<void> _stderrDone;
  String? maintenanceWarning;
  Future<void> _ended() async {
    try {
      await _stderrDone;
    } catch (_) {}
    final code = await process.exitCode;
    final detail = utf8.decode(_stderr, allowMalformed: true).trim();
    _fail(StateError(detail.isEmpty ? '内容服务已退出 ($code)' : detail));
  }

  final Process process;
  final Directory cache;
  final _revisions = <String, int>{};
  final _assets = <String, IdeaAttachment>{};
  final _importAliases = <String, Set<String>>{};
  final _buffer = <int>[];
  Completer<Uint8List>? _response;
  Object? _failure;
  Future<void> _queue = Future.value();
  Future<void> _businessQueue = Future.value();
  ServiceRunSnapshot? _serviceRoute;
  bool _serviceStopping = false;
  bool _serviceAdmissionUncertain = false;
  Future<void>? _closingProcess;
  int _sequence = 0;
  @override
  late final RustStudioPlugin studio = RustStudioPlugin(this);
  @override
  bool writable = false;
  static Future<RustWorkbench> open({
    required String executable,
    required String package,
    required Directory directory,
    bool managed = false,
  }) async {
    await directory.create(recursive: true);
    final cache = Directory('${directory.path}/preview-cache');
    await cache.create(recursive: true);
    final process = await Process.start(executable, [
      if (managed) '--managed',
      managed ? directory.path : '${directory.path}/workbench.db',
      package,
    ]);
    final result = RustWorkbench._(process, cache);
    try {
      await result._call(host.Action.page, configure: (r) => r.limit = 1);
      return result;
    } catch (_) {
      await result.close();
      rethrow;
    }
  }

  PluginManagementState _pluginState(host.ResponseReader r) =>
      PluginManagementState(
        revision: BigInt.from(r.revision).toUnsigned(64),
        digest: Uint8List.fromList(r.sha256 ?? []),
        enabled: r.pluginEnabled,
        approved: r.pluginApproved,
        available: r.pluginAvailable,
        writable: !r.readOnly,
      );
  @override
  Future<PluginManagementState> pluginState() async =>
      _pluginState(await _call(host.Action.pluginState));
  @override
  Future<PluginManagementState> configurePlugin(
    PluginManagementState expected,
    bool enable,
  ) async => _pluginState(
    await _call(
      host.Action.pluginConfigure,
      configure: (r) {
        r.revision = expected.revision.toSigned(64).toInt();
        r.sha256 = expected.digest;
        r.limit = enable ? 1 : 0;
      },
    ),
  );
  @override
  PluginUiTransport createPluginUi() => _WorkbenchUiTransport(this);

  PluginLibraryPage _libraryPage(host.ResponseReader response) {
    final rows = response.plugins;
    if (rows != null && rows.length > 2) {
      throw const FormatException('插件目录超出分页范围');
    }
    List<String> texts(Iterable<String?>? values) => [
      for (final value in values ?? <String?>[])
        if (value != null) value else throw const FormatException('插件资料不完整'),
    ];
    return PluginLibraryPage(
      revision: BigInt.from(response.revision).toUnsigned(64),
      cursor: response.cursor ?? '',
      entries: [
        for (final row in rows ?? <host.PluginEntryReader>[])
          PluginLibraryEntry(
            id: row.packageId ?? '',
            name: row.name ?? '',
            version: row.packageVersion ?? '',
            digest: Uint8List.fromList(row.digest ?? []),
            enabled: row.enabled,
            builtin: row.builtin,
            available: row.available,
            declared: texts(row.declared),
            approved: texts(row.approved),
            declaredIo: texts(row.declaredIo),
            approvedIo: texts(row.approvedIo),
            ioHandlers: texts(row.ioHandlers),
            dependencies: texts(row.dependencies),
            issue: row.issue ?? '',
            handlers: [
              for (final handler
                  in row.handlers ?? <host.PluginHandlerReader>[])
                PluginTransformHandler(
                  name: handler.name ?? '',
                  inputType: handler.inputType ?? '',
                  outputType: handler.outputType ?? '',
                  maxInputBytes: handler.maxInputBytes,
                  maxOutputBytes: handler.maxOutputBytes,
                ),
            ],
          ),
      ],
    );
  }

  @override
  Future<PluginLibraryPage> pluginPage({
    String cursor = '',
    BigInt? revision,
  }) async => _libraryPage(
    await _call(
      host.Action.pluginCatalog,
      configure: (r) {
        r.cursor = cursor;
        r.catalogRevisionBound = revision != null;
        if (revision != null) r.revision = revision.toSigned(64).toInt();
      },
    ),
  );

  @override
  Future<PluginLibraryPage> inspectPlugin(String path) async => _libraryPage(
    await _call(
      host.Action.pluginInspect,
      configure: (r) => r.selectedPath = path,
    ),
  );

  @override
  Future<void> importPlugin(
    String path,
    Uint8List digest,
    BigInt revision,
  ) async {
    await _call(
      host.Action.pluginImport,
      configure: (r) {
        r.selectedPath = path;
        r.sha256 = digest;
        r.revision = revision.toSigned(64).toInt();
      },
    );
  }

  void _externalSelection(
    host.RequestBuilder r,
    PluginLibraryEntry entry,
    BigInt revision,
  ) {
    r.id = entry.id;
    r.sha256 = entry.digest;
    r.revision = revision.toSigned(64).toInt();
  }

  @override
  Future<void> configureExternal(
    PluginLibraryEntry entry,
    BigInt revision,
    List<String> approved,
    bool enable,
  ) async {
    await _call(
      host.Action.pluginApprove,
      configure: (r) {
        _externalSelection(r, entry, revision);
        r.limit = enable ? 1 : 0;
        final decisions = r.initApprovedCapabilities(approved.length);
        for (var index = 0; index < approved.length; index++) {
          decisions[index] = approved[index];
        }
      },
    );
  }

  IoTaskSnapshot _ioSnapshot(host.ResponseReader response, {Uint8List? key}) {
    final snapshot = HttpTaskCodec.snapshot(response.ioState);
    if (key != null && !_same(snapshot.key, key)) {
      throw const FormatException('IO task response identity changed');
    }
    return snapshot;
  }

  @override
  Future<IoTaskSnapshot> startHttp(HttpTaskRequest request) async {
    HttpTaskCodec.validateRequest(request);
    final response = await _call(
      host.Action.httpStart,
      configure: (r) => HttpTaskCodec.writeRequest(request, r.initHttpStart()),
    );
    final snapshot = _ioSnapshot(response);
    if (snapshot.key == null ||
        !_same(snapshot.submission, request.submission)) {
      throw const FormatException('HTTP submission acknowledgement changed');
    }
    return snapshot;
  }

  @override
  Future<IoTaskSnapshot> ioStatus() async =>
      _ioSnapshot(await _call(host.Action.ioStatus));

  Future<IoTaskSnapshot> _ioAction(host.Action action, Uint8List key) async {
    final expected = HttpTaskCodec.identity(key);
    final response = await _call(action, configure: (r) => r.ioKey = expected);
    if (action == host.Action.ioAcknowledge) {
      final snapshot = _ioSnapshot(response);
      if (snapshot.key != null || snapshot.storage != IoStoragePhase.local) {
        throw const FormatException('IO task acknowledgement incomplete');
      }
      return snapshot;
    }
    return _ioSnapshot(response, key: expected);
  }

  @override
  Future<IoTaskSnapshot> pollIo(Uint8List key) =>
      _ioAction(host.Action.ioPoll, key);
  @override
  Future<IoTaskSnapshot> cancelIo(Uint8List key) =>
      _ioAction(host.Action.ioCancel, key);
  @override
  Future<IoTaskSnapshot> repairIo(Uint8List key) =>
      _ioAction(host.Action.ioRepair, key);
  @override
  Future<IoTaskSnapshot> acknowledgeIo(Uint8List key) =>
      _ioAction(host.Action.ioAcknowledge, key);
  @override
  Future<IoTaskRead> readIo(Uint8List key) async {
    final expected = HttpTaskCodec.identity(key);
    final response = await _call(
      host.Action.ioRead,
      configure: (r) => r.ioKey = expected,
    );
    return IoTaskRead(
      snapshot: _ioSnapshot(response, key: expected),
      result: HttpTaskCodec.result(response.ioResult),
    );
  }

  @override
  Future<ServiceConfigPage> serviceConfigPage({
    String? after,
    Uint8List? snapshot,
  }) {
    final bound = snapshot == null ? null : Uint8List.fromList(snapshot);
    return _callDecoded<ServiceConfigPage>(
      host.Action.serviceConfigPage,
      configure: (r) =>
          ServiceCodec.writeConfigPage(r, after: after, snapshot: bound),
      decode: ServiceCodec.configPage,
      clearReply: true,
    );
  }

  @override
  Future<StoredServiceConfig> saveServiceConfig(ServiceConfigUpdate update) =>
      _callDecoded<StoredServiceConfig>(
        host.Action.serviceConfigSave,
        configure: (r) =>
            ServiceCodec.writeConfig(update, r.initServiceConfig()),
        decode: ServiceCodec.configResult,
        clearReply: true,
      );

  @override
  Future<StoredServiceConfig> disableServiceConfig(
    String id,
    BigInt expectedRevision,
  ) => _callDecoded<StoredServiceConfig>(
    host.Action.serviceConfigDisable,
    configure: (r) => ServiceCodec.writeConfigDisable(id, expectedRevision, r),
    decode: ServiceCodec.configResult,
    clearReply: true,
  );

  @override
  Future<ServiceAuthorityPage> serviceAuthorityPage({
    Uint8List? after,
    Uint8List? snapshot,
  }) {
    final cursor = after == null ? null : Uint8List.fromList(after);
    final bound = snapshot == null ? null : Uint8List.fromList(snapshot);
    return _callDecoded<ServiceAuthorityPage>(
      host.Action.serviceAuthorityPage,
      configure: (r) =>
          ServiceCodec.writeAuthorityPage(r, after: cursor, snapshot: bound),
      decode: ServiceCodec.authorityPage,
      clearReply: true,
    );
  }

  @override
  Future<IssuedServiceAuthentication> issueServiceAuthentication({
    required Uint8List reference,
    required BigInt expectedRevision,
    required String principalId,
    required int lifetimeDays,
  }) {
    final owned = Uint8List.fromList(reference);
    return _callDecoded<IssuedServiceAuthentication>(
      host.Action.serviceAuthenticationIssue,
      configure: (r) => ServiceCodec.writeAuthentication(
        r,
        reference: owned,
        expectedRevision: expectedRevision,
        principalId: principalId,
        lifetimeDays: lifetimeDays,
      ),
      decode: ServiceCodec.issuedAuthentication,
      clearReply: true,
    );
  }

  @override
  Future<StoredServiceAuthority> disableServiceAuthority(
    Uint8List reference,
    BigInt expectedRevision,
  ) {
    final owned = Uint8List.fromList(reference);
    return _callDecoded<StoredServiceAuthority>(
      host.Action.serviceAuthorityDisable,
      configure: (r) =>
          ServiceCodec.writeAuthorityDisable(owned, expectedRevision, r),
      decode: ServiceCodec.authorityResult,
      clearReply: true,
    );
  }

  @override
  Future<StoredServiceAuthority> saveServicePublication(
    ServicePublicationUpdate update,
  ) => _callDecoded<StoredServiceAuthority>(
    host.Action.servicePublicationSave,
    configure: (r) =>
        ServiceCodec.writePublication(update, r.initServicePublication()),
    decode: ServiceCodec.publicationResult,
    clearReply: true,
  );

  EndpointPolicy _endpointPolicy(host.EndpointPolicyReader row) {
    final methods = row.methods;
    if (methods == null || methods.length > 16) {
      throw const FormatException('Invalid endpoint methods');
    }
    final value = EndpointPolicy(
      packageId: row.packageId ?? '',
      packageDigest: Uint8List.fromList(row.packageDigest ?? []),
      origin: row.origin ?? '',
      profile: row.profile,
      methods: [
        for (final method in methods)
          method ?? (throw const FormatException('Missing endpoint method')),
      ],
      credentialReference: Uint8List.fromList(row.credentialReference ?? []),
      rootCertificate: Uint8List.fromList(row.rootCertificate ?? []),
      maxRequestBytes: row.maxRequestBytes,
      maxResponseBytes: row.maxResponseBytes,
      maxHeaderBytes: row.maxHeaderBytes,
      maxConcurrent: row.maxConcurrent,
      timeoutMs: row.timeoutMs,
      maxFrameBytes: row.maxFrameBytes,
    );
    _checkEndpointPolicy(value);
    return value;
  }

  void _checkEndpointPolicy(EndpointPolicy value, {bool forSave = false}) {
    const methods = {
      'GET',
      'HEAD',
      'POST',
      'PUT',
      'PATCH',
      'DELETE',
      'OPTIONS',
    };
    bool bounded(int n, int max) => n > 0 && n <= max;
    if (value.packageId.isEmpty ||
        value.packageId.length > 256 ||
        value.packageDigest.length != 32 ||
        value.packageDigest.every((v) => v == 0) ||
        value.origin.isEmpty ||
        value.origin.length > 2048 ||
        value.profile < 1 ||
        value.profile > 3 ||
        value.methods.isEmpty ||
        value.methods.length > 16 ||
        !value.methods.every((v) => RegExp(r'^[A-Z-]{1,32}$').hasMatch(v)) ||
        (forSave && !value.methods.every(methods.contains)) ||
        (value.credentialReference.isNotEmpty &&
            (value.credentialReference.length != 32 ||
                value.credentialReference.every((v) => v == 0))) ||
        value.rootCertificate.length > 32768 ||
        !bounded(value.maxRequestBytes, 65536) ||
        !bounded(value.maxResponseBytes, 65536) ||
        !bounded(value.maxHeaderBytes, 16384) ||
        !bounded(value.maxConcurrent, 128) ||
        !bounded(value.timeoutMs, 30000) ||
        !bounded(value.maxFrameBytes, 131072)) {
      throw const FormatException('Invalid endpoint policy');
    }
    for (var i = 1; i < value.methods.length; i++) {
      if (value.methods[i - 1].compareTo(value.methods[i]) >= 0) {
        throw const FormatException('Invalid endpoint method ordering');
      }
    }
  }

  StoredEndpoint _endpoint(host.EndpointInfoReader row) {
    final reference = Uint8List.fromList(row.reference ?? []);
    final revision = BigInt.from(row.revision).toUnsigned(64);
    final created = BigInt.from(row.createdMs).toUnsigned(64);
    final expires = BigInt.from(row.expiresMs).toUnsigned(64);
    _credentialIdentity(reference, revision);
    if (created <= BigInt.zero ||
        expires <= created ||
        expires - created > BigInt.from(30 * 86400000)) {
      throw const FormatException('Invalid endpoint lifetime');
    }
    return StoredEndpoint(
      reference: reference,
      revision: revision,
      createdMs: created,
      expiresMs: expires,
      disabled: row.disabled,
      policy: _endpointPolicy(
        row.policy ?? (throw const FormatException('Missing endpoint policy')),
      ),
    );
  }

  @override
  Future<EndpointPage> endpointPage({
    Uint8List? after,
    Uint8List? snapshot,
  }) async {
    if ((after != null && after.length != 32) ||
        (snapshot != null && snapshot.length != 32)) {
      throw const FormatException('Invalid endpoint cursor');
    }
    final result = await _call(
      host.Action.endpointPage,
      configure: (r) {
        if (after != null) r.endpointCursor = after;
        if (snapshot != null) r.endpointSnapshot = snapshot;
      },
    );
    final entries = result.endpoints;
    final bound = Uint8List.fromList(result.endpointSnapshot ?? []);
    final next = Uint8List.fromList(result.endpointCursor ?? []);
    if (entries == null ||
        entries.length > 2 ||
        bound.length != 32 ||
        (next.isNotEmpty && next.length != 32)) {
      throw const FormatException('Invalid endpoint page');
    }
    return EndpointPage(
      entries: [for (final row in entries) _endpoint(row)],
      snapshot: bound,
      next: next.isEmpty ? null : next,
    );
  }

  StoredEndpoint _endpointResult(host.ResponseReader result) {
    final entries = result.endpoints;
    if (entries == null || entries.length != 1) {
      throw const FormatException('Endpoint update result unavailable');
    }
    return _endpoint(entries[0]);
  }

  @override
  Future<StoredEndpoint> saveEndpoint({
    required Uint8List reference,
    required BigInt expectedRevision,
    required BigInt registryRevision,
    required int lifetimeDays,
    required EndpointPolicy policy,
  }) async {
    _credentialIdentity(reference, expectedRevision, create: true);
    _checkEndpointPolicy(policy, forSave: true);
    if (registryRevision < BigInt.zero ||
        registryRevision > (BigInt.one << 63) - BigInt.one ||
        lifetimeDays < 1 ||
        lifetimeDays > 30) {
      throw const FormatException('Invalid endpoint approval');
    }
    return _endpointResult(
      await _call(
        host.Action.endpointSave,
        configure: (r) {
          r.endpointReference = reference;
          r.revision = expectedRevision.toSigned(64).toInt();
          r.endpointRegistryRevision = registryRevision.toSigned(64).toInt();
          r.endpointDays = lifetimeDays;
          final p = r.initEndpointPolicy();
          p.packageId = policy.packageId;
          p.packageDigest = policy.packageDigest;
          p.origin = policy.origin;
          p.profile = policy.profile;
          p.credentialReference = policy.credentialReference;
          p.rootCertificate = policy.rootCertificate;
          p.maxRequestBytes = policy.maxRequestBytes;
          p.maxResponseBytes = policy.maxResponseBytes;
          p.maxHeaderBytes = policy.maxHeaderBytes;
          p.maxConcurrent = policy.maxConcurrent;
          p.timeoutMs = policy.timeoutMs;
          p.maxFrameBytes = policy.maxFrameBytes;
          final methods = p.initMethods(policy.methods.length);
          for (var i = 0; i < policy.methods.length; i++) {
            methods[i] = policy.methods[i];
          }
        },
      ),
    );
  }

  @override
  Future<StoredEndpoint> disableEndpoint(StoredEndpoint expected) async {
    _credentialIdentity(expected.reference, expected.revision);
    return _endpointResult(
      await _call(
        host.Action.endpointDisable,
        configure: (r) {
          r.endpointReference = expected.reference;
          r.revision = expected.revision.toSigned(64).toInt();
        },
      ),
    );
  }

  StoredCredential _credential(host.CredentialInfoReader row) {
    final reference = Uint8List.fromList(row.reference ?? []);
    final revision = BigInt.from(row.revision).toUnsigned(64);
    final created = BigInt.from(row.createdMs).toUnsigned(64);
    final expires = BigInt.from(row.expiresMs).toUnsigned(64);
    if (reference.length != 32 ||
        reference.every((b) => b == 0) ||
        revision <= BigInt.zero ||
        revision > (BigInt.one << 63) - BigInt.one ||
        created <= BigInt.zero ||
        expires <= created ||
        expires - created > BigInt.from(30 * 86400000)) {
      throw const FormatException('Invalid credential metadata');
    }
    return StoredCredential(
      reference: reference,
      revision: revision,
      createdMs: created,
      expiresMs: expires,
      disabled: row.disabled,
    );
  }

  @override
  Future<CredentialPage> credentialPage({
    Uint8List? after,
    Uint8List? snapshot,
  }) async {
    final result = await _call(
      host.Action.credentialPage,
      configure: (r) {
        if (after != null) r.credentialCursor = after;
        if (snapshot != null) r.credentialSnapshot = snapshot;
      },
    );
    final rows = result.credentials;
    final bound = Uint8List.fromList(result.credentialSnapshot ?? []);
    final cursor = Uint8List.fromList(result.credentialCursor ?? []);
    if (rows == null ||
        rows.length > 16 ||
        bound.length != 32 ||
        (cursor.isNotEmpty && cursor.length != 32)) {
      throw const FormatException('Invalid credential page');
    }
    return CredentialPage(
      entries: [for (final row in rows) _credential(row)],
      snapshot: bound,
      next: cursor.isEmpty ? null : cursor,
    );
  }

  StoredCredential _credentialResult(host.ResponseReader result) {
    final rows = result.credentials;
    if (rows == null || rows.length != 1) {
      throw const FormatException('Credential update result unavailable');
    }
    return _credential(rows[0]);
  }

  void _credentialIdentity(
    Uint8List reference,
    BigInt revision, {
    bool create = false,
  }) {
    if (revision < BigInt.zero ||
        revision > (BigInt.one << 63) - BigInt.one ||
        (reference.isEmpty
            ? !create || revision != BigInt.zero
            : reference.length != 32 ||
                  reference.every((b) => b == 0) ||
                  revision == BigInt.zero)) {
      throw const FormatException('Invalid credential update identity');
    }
  }

  @override
  Future<StoredCredential> saveCredential({
    required Uint8List reference,
    required BigInt expectedRevision,
    required String headerName,
    required String headerValue,
    required int lifetimeDays,
  }) async {
    _credentialIdentity(reference, expectedRevision, create: true);
    if (lifetimeDays < 1 ||
        lifetimeDays > 30 ||
        headerName.isEmpty ||
        headerName.length > 128 ||
        headerValue.isEmpty ||
        headerValue.length > 8192 ||
        !headerName.codeUnits.every((v) => v >= 0x21 && v <= 0x7e) ||
        !headerValue.codeUnits.every((v) => v >= 0x20 && v <= 0x7e)) {
      throw const FormatException('Invalid credential input');
    }
    return _credentialResult(
      await _call(
        host.Action.credentialSave,
        configure: (r) {
          r.credentialReference = reference;
          r.revision = expectedRevision.toSigned(64).toInt();
          r.credentialHeader = headerName;
          r.credentialSecret = headerValue;
          r.credentialDays = lifetimeDays;
        },
      ),
    );
  }

  @override
  Future<StoredCredential> disableCredential(StoredCredential expected) async {
    _credentialIdentity(expected.reference, expected.revision);
    return _credentialResult(
      await _call(
        host.Action.credentialDisable,
        configure: (r) {
          r.credentialReference = expected.reference;
          r.revision = expected.revision.toSigned(64).toInt();
        },
      ),
    );
  }

  @override
  Future<void> configureExternalIo(
    PluginLibraryEntry entry,
    BigInt revision,
    List<String> approved,
  ) async {
    await _call(
      host.Action.pluginApproveIo,
      configure: (r) {
        _externalSelection(r, entry, revision);
        final decisions = r.initApprovedIoCapabilities(approved.length);
        for (var index = 0; index < approved.length; index++) {
          decisions[index] = approved[index];
        }
      },
    );
  }

  @override
  Future<void> removeExternal(PluginLibraryEntry entry, BigInt revision) async {
    await _call(
      host.Action.pluginRemove,
      configure: (r) => _externalSelection(r, entry, revision),
    );
  }

  @override
  Future<Uint8List> transformExternal(
    PluginLibraryEntry entry,
    BigInt revision,
    PluginTransformHandler handler,
    Uint8List input,
  ) async {
    final response = await _call(
      host.Action.pluginTransform,
      configure: (r) {
        _externalSelection(r, entry, revision);
        r.handler = handler.name;
        r.inputType = handler.inputType;
        r.outputType = handler.outputType;
        r.payload = input;
      },
    );
    return Uint8List.fromList(response.payload ?? []);
  }

  @override
  PluginUiTransport createExternalPluginUi(
    PluginLibraryEntry entry,
    BigInt revision,
  ) => _WorkbenchUiTransport(this, entry: entry, expectedRevision: revision);

  @override
  Future<void> backupProtection(String destination) async {
    await _call(
      host.Action.backupProtection,
      configure: (r) => r.selectedPath = destination,
    );
  }

  @override
  Future<void> backupSnapshot(String destination) async {
    await _call(
      host.Action.backupSnapshot,
      requestTimeout: const Duration(minutes: 30),
      configure: (r) => r.selectedPath = destination,
    );
  }

  static Future<void> restoreSnapshot({
    required String executable,
    required String archive,
    required Directory destination,
  }) async {
    final result = await Process.run(executable, [
      '--restore-snapshot',
      archive,
      destination.path,
    ]);
    if (result.exitCode != 0) throw StateError(result.stderr.toString().trim());
  }

  static Future<void> activateLibrary({
    required String executable,
    required Directory root,
    required Directory selected,
  }) async {
    final result = await Process.run(executable, [
      '--activate-library',
      root.path,
      selected.path,
    ]);
    if (result.exitCode != 0) throw StateError(result.stderr.toString().trim());
  }

  /// The host verifies and restores protected bytes. Dart never loads secret material.
  static Future<void> restoreKey({
    required String executable,
    required Directory directory,
    required String selected,
    bool managed = false,
  }) async {
    final result = await Process.run(executable, [
      managed ? '--restore-active-key' : '--restore-key',
      managed ? directory.path : '${directory.path}/workbench.db',
      selected,
    ]);
    if (result.exitCode != 0) {
      final detail = result.stderr.toString().trim();
      throw StateError(detail.isEmpty ? '恢复未完成，请重新打开内容库核对。' : detail);
    }
  }

  @override
  Future<ServiceRunSnapshot> startServiceRun(ServiceRunRequest request) async {
    ServiceRunValidation.request(request);
    try {
      return await _callDecoded<ServiceRunSnapshot>(
        host.Action.serviceRunStart,
        configure: (r) =>
            ServiceRunCodec.writeRequest(request, r.initServiceRun()),
        decode: (r) =>
            ServiceRunCodec.snapshot(r, expectedSubmission: request.submission),
        clearReply: true,
        updatePresentation: false,
      );
    } on _HostResponseError catch (error) {
      throw ServiceRunStartFailure(error.message);
    }
  }

  @override
  Future<ServiceRunSnapshot> serviceRunStatus({Uint8List? key}) {
    final owned = key == null ? null : ServiceRunValidation.identity(key);
    return _callDecoded<ServiceRunSnapshot>(
      host.Action.serviceRunStatus,
      configure: (r) {
        if (owned != null) r.ioKey = owned;
      },
      decode: (r) => ServiceRunCodec.snapshot(r, expectedKey: owned),
      clearReply: true,
      updatePresentation: false,
    );
  }

  @override
  Future<OwnerCommandSnapshot> submitServiceCommand(
    Uint8List task,
    Uint8List submission,
    Uint8List frame,
  ) async {
    final taskId = ServiceRunValidation.identity(task);
    final attempt = ServiceRunValidation.identity(submission);
    ServiceRunValidation.command(frame);
    // The caller owns its original frame. Protect this asynchronous queue copy
    // until host_request has serialized and wiped its own command envelope.
    final input = Uint8List.fromList(frame);
    try {
      return await _callDecoded<OwnerCommandSnapshot>(
        host.Action.commandSubmit,
        configure: (r) {
          r.ioKey = taskId;
          r.commandSubmission = attempt;
          r.payload = input;
        },
        decode: (r) => ServiceRunCodec.command(r, expectedSubmission: attempt),
        clearReply: true,
        updatePresentation: false,
      );
    } finally {
      input.fillRange(0, input.length, 0);
    }
  }

  Future<OwnerCommandSnapshot> _serviceCommandAction(
    host.Action action,
    Uint8List task,
    Uint8List key,
  ) {
    final taskId = ServiceRunValidation.identity(task);
    final commandId = ServiceRunValidation.identity(key);
    return _callDecoded<OwnerCommandSnapshot>(
      action,
      configure: (r) {
        r.ioKey = taskId;
        r.commandKey = commandId;
      },
      decode: (r) => ServiceRunCodec.command(r, expectedKey: commandId),
      clearReply: true,
      updatePresentation: false,
    );
  }

  @override
  Future<OwnerCommandSnapshot> serviceCommandStatus(
    Uint8List task,
    Uint8List key,
  ) => _serviceCommandAction(host.Action.commandStatus, task, key);

  @override
  Future<OwnerCommandSnapshot> serviceCommandBySubmission(
    Uint8List task,
    Uint8List submission,
  ) {
    final taskId = ServiceRunValidation.identity(task);
    final attempt = ServiceRunValidation.identity(submission);
    return _callDecoded<OwnerCommandSnapshot>(
      host.Action.commandStatus,
      configure: (r) {
        r.ioKey = taskId;
        r.commandSubmission = attempt;
      },
      decode: (r) => ServiceRunCodec.command(r, expectedSubmission: attempt),
      clearReply: true,
      updatePresentation: false,
    );
  }

  @override
  Future<OwnerCommandSnapshot> cancelServiceCommand(
    Uint8List task,
    Uint8List key,
  ) => _serviceCommandAction(host.Action.commandCancel, task, key);

  @override
  Future<OwnerCommandRead> readServiceCommand(Uint8List task, Uint8List key) {
    final taskId = ServiceRunValidation.identity(task);
    final commandId = ServiceRunValidation.identity(key);
    return _callDecoded<OwnerCommandRead>(
      host.Action.commandRead,
      configure: (r) {
        r.ioKey = taskId;
        r.commandKey = commandId;
      },
      decode: (r) => ServiceRunCodec.read(r, expectedKey: commandId),
      clearReply: true,
      updatePresentation: false,
    );
  }

  void _fail(Object error) {
    _failure ??= error;
    _buffer.fillRange(0, _buffer.length, 0);
    _buffer.clear();
    final pending = _response;
    _response = null;
    if (pending != null && !pending.isCompleted) pending.completeError(error);
  }

  void _receive(List<int> bytes) {
    try {
      if (_failure != null) return;
      if (_buffer.length + bytes.length > 256 * 1024 + 4) {
        _fail(const FormatException('内容服务消息过大'));
        return;
      }
      _buffer.addAll(bytes);
      if (_buffer.length < 4) return;
      final size = ByteData.sublistView(
        Uint8List.fromList(_buffer.take(4).toList()),
      ).getUint32(0, Endian.little);
      if (size == 0 || size > 256 * 1024 || _buffer.length > 256 * 1024 + 4) {
        _fail(const FormatException('内容服务消息过大'));
        return;
      }
      if (_buffer.length < size + 4) return;
      if (_buffer.length != size + 4 || _response == null) {
        _fail(const FormatException('内容服务响应顺序错误'));
        return;
      }
      final reply = Uint8List(size)..setRange(0, size, _buffer, 4);
      _buffer.fillRange(0, _buffer.length, 0);
      _buffer.clear();
      final pending = _response!;
      _response = null;
      pending.complete(reply);
    } finally {
      // Process stdout has one subscriber. Erase writable delivered chunks after
      // copying them; immutable runtime-owned chunks cannot be guaranteed wiped.
      try {
        bytes.fillRange(0, bytes.length, 0);
      } on UnsupportedError {
        // The aggregation buffer and owned decoded frame are still cleared.
      }
    }
  }

  static MessageReader readMessage(
    Uint8List bytes, {
    int maxBytes = 128 * 1024,
  }) {
    if (bytes.length < 8 || bytes.length > maxBytes) {
      throw const FormatException('消息长度');
    }
    final view = ByteData.sublistView(bytes);
    final count = view.getUint32(0, Endian.little) + 1;
    if (count > 512) throw const FormatException('分段数量');
    var total = ((count + 2) ~/ 2) * 8;
    if (total > bytes.length) throw const FormatException('分段头');
    for (var i = 0; i < count; i++) {
      total += view.getUint32(4 + i * 4, Endian.little) * 8;
      if (total > bytes.length) throw const FormatException('分段长度');
    }
    if (total != bytes.length) throw const FormatException('多余消息');
    return MessageReader.deserialize(
      bytes,
      MessageReaderOptions(
        traversalLimitInWords: maxBytes ~/ 8,
        nestingLimit: 20,
        maxSegments: 512,
      ),
    );
  }

  static bool _same(List<int>? a, List<int> b) =>
      a != null &&
      a.length == b.length &&
      List.generate(b.length, (i) => a[i] == b[i]).every((v) => v);
  Future<host.ResponseReader> _call(
    host.Action action, {
    void Function(host.RequestBuilder)? configure,
    Duration requestTimeout = const Duration(seconds: 60),
  }) => _callDecoded<host.ResponseReader>(
    action,
    configure: configure,
    requestTimeout: requestTimeout,
    decode: (reply) => reply,
  );

  /// Decode owned service models before clearing the private reply frame.
  /// Native readers borrow that frame; a decoder must not return reader views.
  Future<T> _callDecoded<T>(
    host.Action action, {
    void Function(host.RequestBuilder)? configure,
    Duration requestTimeout = const Duration(seconds: 60),
    required T Function(host.ResponseReader) decode,
    bool clearReply = false,
    bool updatePresentation = true,
  }) {
    if (_isScheduler(action)) {
      return _exchangeDecoded(
        action,
        configure: configure,
        requestTimeout: requestTimeout,
        decode: decode,
        clearReply: clearReply,
        updatePresentation: updatePresentation,
      );
    }
    final completion = Completer<T>();
    _businessQueue = _businessQueue.then((_) async {
      try {
        completion.complete(
          await _exchangeDecoded(
            action,
            configure: configure,
            requestTimeout: requestTimeout,
            decode: decode,
            clearReply: clearReply,
            updatePresentation: updatePresentation,
            routeBusiness: true,
          ),
        );
      } catch (error, stack) {
        completion.completeError(error, stack);
      }
    });
    return completion.future;
  }

  Future<T> _exchangeDecoded<T>(
    host.Action action, {
    void Function(host.RequestBuilder)? configure,
    Duration requestTimeout = const Duration(seconds: 60),
    required T Function(host.ResponseReader) decode,
    bool clearReply = false,
    bool updatePresentation = true,
    bool routeBusiness = false,
    Duration Function()? remainingBudget,
  }) {
    // Preserve the original fenced-transport diagnosis while EOF cleanup runs.
    if (_failure != null) return Future.error(_failure!);
    if (_closingProcess != null) {
      return Future.error(StateError('Content service is closing'));
    }
    final completion = Completer<T>();
    _queue = _queue.then((_) async {
      Uint8List? receivedBytes;
      Uint8List? schedulerKey;
      Completer<Uint8List>? pendingReply;
      Never transportTimeout() {
        final error = TimeoutException('内容服务响应超时');
        _fail(error);
        // EOF requests original-owner cleanup; a timer never proves exit.
        unawaited(close().catchError((Object _) {}));
        throw error;
      }

      try {
        if (_failure != null) throw _failure!;
        remainingBudget?.call();
        // Choose only when this actual pipe slot is reached. A preceding
        // service-start receipt has already updated the observed owner here.
        if (routeBusiness && _serviceAdmissionUncertain) {
          throw StateError(
            'Service admission outcome is unknown; inspect its current status',
          );
        }
        if (routeBusiness && _serviceRoute != null) {
          final service = _serviceRoute!;
          unawaited(
            _throughService(
              action,
              service,
              configure: configure,
              requestTimeout: requestTimeout,
              decode: decode,
              clearReply: clearReply,
              updatePresentation: updatePresentation,
            ).then<void>(
              completion.complete,
              onError: completion.completeError,
            ),
          );
          return;
        }
        late Completer<Uint8List> response;
        await sendHostRequest(
          action,
          configure: (r) {
            configure?.call(r);
            if (_isScheduler(action)) schedulerKey = r.asReader().ioKey;
          },
          send: (payload) async {
            final header = ByteData(4)
              ..setUint32(0, payload.length, Endian.little);
            response = _response = pendingReply = Completer<Uint8List>();
            process.stdin.add(header.buffer.asUint8List());
            process.stdin.add(payload);
            await process.stdin.flush().timeout(
              remainingBudget?.call() ?? requestTimeout,
              onTimeout: transportTimeout,
            );
          },
        );
        final bytes = receivedBytes = await response.future.timeout(
          remainingBudget?.call() ?? requestTimeout,
          onTimeout: transportTimeout,
        );
        remainingBudget?.call();
        final reply = readMessage(
          bytes,
          maxBytes: ServiceRunCodec.responseMaxBytes(action),
        ).getRoot(host.responseFactory);
        if (reply.version != 1 || !_same(reply.digest, contract.hostDigest)) {
          throw const FormatException('内容服务版本不匹配');
        }
        final maskServiceObservation =
            _isScheduler(action) && _serviceRoute != null;
        _checkBusinessReply(
          action,
          reply,
          updatePresentation: updatePresentation && !maskServiceObservation,
        );
        final decoded = decode(reply);
        _observeScheduler(action, reply, expectedTask: schedulerKey);
        if (maskServiceObservation &&
            _serviceRoute == null &&
            updatePresentation) {
          _checkBusinessReply(action, reply, updatePresentation: true);
        }
        completion.complete(decoded);
      } catch (e, stack) {
        if (action == host.Action.serviceRunStart &&
            pendingReply != null &&
            e is! _HostResponseError) {
          _serviceAdmissionUncertain = true;
        }
        if (pendingReply != null && receivedBytes == null) {
          // A failed write/flush cannot prove whether the host accepted this
          // request. Fence the channel so a late reply can never satisfy a
          // different request, including a one-time authentication receipt.
          _fail(StateError('Content service transport outcome is unknown'));
          unawaited(close().catchError((Object _) {}));
        }
        completion.completeError(e, stack);
      } finally {
        if (receivedBytes case final bytes?) {
          if (clearReply) bytes.fillRange(0, bytes.length, 0);
        } else if (pendingReply case final pending?) {
          // A write/flush failure can race a completed reply. Also erase that
          // owned frame when it arrives, without retrying the request.
          unawaited(
            pending.future.then<void>((bytes) {
              if (clearReply) bytes.fillRange(0, bytes.length, 0);
            }, onError: (Object _, StackTrace _) {}),
          );
        }
      }
    });
    return completion.future;
  }

  wire.ResponseReader _payload(Uint8List? bytes) {
    if (bytes == null) throw const FormatException('缺少内容响应');
    final r = readMessage(bytes).getRoot(wire.responseFactory);
    if (r.version != 1 || !_same(r.digest, contract.workbenchDigest)) {
      throw const FormatException('工作台插件版本不匹配');
    }
    return r;
  }

  Future<Idea> _idea(
    host.ResponseReader reply, {
    bool trackRevision = true,
  }) async {
    final r = _payload(reply.payload).idea!;
    final id = r.id!;
    final attachments = <IdeaAttachment>[];
    for (final a in r.assets ?? <wire.AssetReader>[]) {
      final key = '$id/${a.id}';
      var item = _assets[key];
      if (item != null &&
          item.source.local &&
          !await File(item.source.location).exists()) {
        _assets.remove(key);
        item = null;
      }
      if (item == null) {
        final extension = (a.name ?? '')
            .split('.')
            .last
            .toLowerCase()
            .replaceAll(RegExp('[^a-z0-9]'), '');
        final path = '${cache.path}/${id}_${a.id}.$extension';
        // A fresh host-verified extraction is used for each session. Existing
        // cache names are avoided; user exports are separate platform actions.
        final file = File(
          '$path.${DateTime.now().microsecondsSinceEpoch}.$extension',
        );
        await _call(
          host.Action.exportFile,
          configure: (out) {
            out.id = id;
            out.attachment = a.id;
            out.selectedPath = file.path;
          },
        );
        item = IdeaAttachment(
          source: TextureSource(
            location: file.path,
            name: a.name!,
            kind: TextureKind.values.byName(a.kind!),
            local: true,
          ),
          size: a.bytes,
          pluginId: a.id,
        );
        _assets[key] = item;
      }
      attachments.add(item);
    }
    final idea = Idea(
      r.title ?? '',
      r.description ?? '',
      r.category!,
      Idea.icons[r.icon],
      Color(r.color),
      id: id,
      stage: r.stage,
      favorite: r.favorite,
      todos: [...?r.todos].whereType<String>().toList(),
      completed: {...?r.completed}.whereType<String>().toSet(),
      hypothesis: r.hypothesis ?? '',
      conclusion: r.conclusion ?? '',
      attachments: attachments,
    );
    if (trackRevision) _revisions[id] = reply.revision;
    return idea;
  }

  Future<List<Idea>> load() async {
    final ideas = <Idea>[];
    final revisions = <String, int>{};
    var cursor = '';
    do {
      final page = await _call(
        host.Action.page,
        configure: (r) {
          r.cursor = cursor;
          r.limit = 128;
        },
      );
      cursor = page.cursor ?? '';
      for (final id in page.ids ?? <String?>[]) {
        final record = await _call(
          host.Action.read,
          configure: (r) => r.id = id,
        );
        final data = _payload(record.payload).idea!;
        revisions[id!] = record.revision;
        if (!data.deleted) ideas.add(await _idea(record, trackRevision: false));
      }
    } while (cursor.isNotEmpty);
    _revisions.addAll(revisions);
    return ideas.reversed.toList();
  }

  void _writeIdea(
    wire.IdeaBuilder b,
    Idea idea,
    List<IdeaAttachment> attachments, {
    bool includeImportAliases = false,
  }) {
    b.id = idea.id;
    b.title = idea.title;
    var description = idea.description;
    for (var i = 0; i < attachments.length; i++) {
      final old = idea.attachments[i].source;
      final target = 'attachment:${attachments[i].pluginId}';
      final names = {
        old.location,
        old.name,
        Uri.encodeComponent(old.location),
        Uri.encodeComponent(old.name),
        if (includeImportAliases)
          ...?_importAliases['${idea.id}/${attachments[i].pluginId}'],
      }.toList()..sort((a, b) => b.length.compareTo(a.length));
      for (final name in names) {
        description = description.replaceAll(
          RegExp('attachment:${RegExp.escape(name)}(?=[\\s)>]|\u0024)'),
          target,
        );
      }
    }
    b.description = description;
    b.category = idea.category;
    b.stage = idea.stage;
    b.hypothesis = idea.hypothesis;
    b.conclusion = idea.conclusion;
    b.favorite = idea.favorite;
    b.icon = Idea.icons.indexOf(idea.icon).clamp(0, 3);
    b.color = idea.color.toARGB32();
    final todos = b.initTodos(idea.todos.length);
    for (var i = 0; i < idea.todos.length; i++) {
      todos[i] = idea.todos[i];
    }
    final completed = idea.completed.toList();
    final done = b.initCompleted(completed.length);
    for (var i = 0; i < completed.length; i++) {
      done[i] = completed[i];
    }
    final assets = b.initAssets(attachments.length);
    for (var i = 0; i < attachments.length; i++) {
      final a = attachments[i];
      final out = assets[i];
      out.id = a.pluginId!;
      out.name = a.source.name;
      out.kind = a.source.kind.name;
      out.bytes = a.size;
    }
  }

  Future<IdeaAttachment> _importAttachment(
    String target,
    IdeaAttachment a,
  ) async {
    if (a.pluginId != null) return a;
    final r = await _call(
      host.Action.importFile,
      configure: (r) {
        r.id = target;
        r.selectedPath = a.source.location;
        r.name = a.source.name;
        r.kind = a.source.kind.name;
      },
    );
    final asset = _payload(r.payload).idea!.assets![0];
    final item = IdeaAttachment(
      source: a.source,
      size: asset.bytes,
      pluginId: asset.id,
    );
    _importAliases['$target/${asset.id}'] = {
      a.source.location,
      a.source.name,
      Uri.encodeComponent(a.source.location),
      Uri.encodeComponent(a.source.name),
    };
    // Keep aliases as text only; they never own the read cache.
    // A selected editor file is a staging source, not a durable read cache.
    // Materialize confirmed content through _idea's host export instead.
    return item;
  }

  @override
  Future<List<Idea>> refreshEditorContent() => load();

  @override
  Future<WorkbenchEditorSession> openEditor(
    String target, {
    required bool create,
  }) async {
    final revision = create ? 0 : _revisions[target] ?? 0;
    if (!create && revision == 0) throw StateError('请先重新读取要编辑的卡片。');
    final response = await _call(
      host.Action.openCaptureScope,
      configure: (r) {
        r.id = target;
        r.revision = revision;
      },
    );
    final scope = response.captureScope ?? '';
    if (scope.isEmpty) throw const FormatException('缺少编辑器会话标识');
    return _NativeEditorSession(this, target, revision, scope, create);
  }

  Future<host.ResponseReader> _captureUpload(
    String correlation,
    Uint8List value,
    host.Action finish,
  ) async {
    final bytes = Uint8List.fromList(value);
    if (bytes.isEmpty || bytes.length > 4 * 1024 * 1024) {
      throw const FormatException('编辑器记录超过 4 MiB，请减少本次粘贴内容。');
    }
    final begin = await _call(
      host.Action.beginCaptureUpload,
      configure: (r) {
        r.operation = correlation;
        r.totalLength = bytes.length;
        r.sha256 = Uint8List.fromList(sha256.convert(bytes).bytes);
      },
    );
    final token = begin.transfer ?? '';
    if (token.isEmpty) throw const FormatException('缺少编辑器传输标识');
    try {
      for (var offset = 0; offset < bytes.length;) {
        final end = (offset + _partBytes).clamp(0, bytes.length);
        final reply = await _call(
          host.Action.appendCaptureUpload,
          configure: (r) {
            r.transfer = token;
            r.offset = offset;
            r.payload = Uint8List.sublistView(bytes, offset, end);
          },
        );
        if (reply.transfer != token || reply.offset != end) {
          throw const FormatException('编辑器上传确认不匹配');
        }
        offset = end;
      }
      return await _call(finish, configure: (r) => r.transfer = token);
    } catch (_) {
      try {
        await _call(
          host.Action.abortCaptureUpload,
          configure: (r) => r.transfer = token,
        );
      } catch (_) {
        /* Connection failure drops the host's bounded staging buffers. */
      }
      rethrow;
    }
  }

  @override
  Future<Idea> apply(
    PluginAction action,
    Idea idea, {
    String text = '',
    bool flag = false,
  }) async {
    if (!writable) throw StateError('工作台插件不可用，已有内容仍可查看和导出。');
    final attachments = <IdeaAttachment>[];
    if (action == PluginAction.create || action == PluginAction.edit) {
      for (final a in idea.attachments) {
        attachments.add(await _importAttachment(idea.id, a));
      }
    }
    final builder = MessageBuilder();
    final r = builder.initRoot(wire.requestFactory);
    r.version = 1;
    r.digest = Uint8List.fromList(contract.workbenchDigest);
    r.action = wire.Action.values.byName(action.name);
    r.text = text;
    r.flag = flag;
    if (action == PluginAction.create || action == PluginAction.edit) {
      _writeIdea(
        r.initProposed(),
        idea,
        attachments,
        includeImportAliases: true,
      );
    }
    final bytes = builder.serialize();
    if (bytes.length > 65536) {
      throw const FormatException('当前插件消息容量不足，请缩短正文或改为附件。');
    }
    final result = await _call(
      host.Action.mutate,
      configure: (r) {
        r.id = idea.id;
        r.operation =
            'ui-${DateTime.now().microsecondsSinceEpoch}-${_sequence++}';
        r.revision = action == PluginAction.create
            ? 0
            : _revisions[idea.id] ?? 0;
        r.payload = bytes;
      },
    );
    return _idea(result);
  }

  @override
  Future<List<String>> query(
    String section,
    String filter,
    String text,
    String sort, {
    String? operation,
  }) async {
    final builder = MessageBuilder();
    final r = builder.initRoot(wire.requestFactory);
    r.version = 1;
    r.digest = Uint8List.fromList(contract.workbenchDigest);
    r.action = wire.Action.query;
    r.section = section;
    r.filter = filter;
    r.text = text;
    r.sort = sort;
    final result = await _call(
      host.Action.query,
      configure: (r) {
        r.operation = operation ?? newQueryOperationId();
        r.payload = builder.serialize();
      },
    );
    return [...?result.ids].whereType<String>().toList();
  }

  Future<Uint8List> service(Uint8List bytes) async => (await _call(
    host.Action.service,
    configure: (r) => r.payload = bytes,
  )).payload!;
  Future<Uint8List> capture(Uint8List bytes) async =>
      (await captureResult(bytes)).payload;
  Future<({Uint8List payload, String? ticket})> captureResult(
    Uint8List bytes, {
    String? scope,
    String? parent,
  }) async {
    final reply = await _call(
      host.Action.capture,
      configure: (r) {
        r.payload = bytes;
        r.captureScope = scope ?? '';
        r.captureParent = parent ?? '';
      },
    );
    final payload = reply.payload;
    final ticket = reply.captureTicket;
    if (payload == null ||
        (scope != null && (ticket == null || ticket.isEmpty))) {
      throw const FormatException('内容转换缺少结果或票据');
    }
    return (
      payload: payload,
      ticket: ticket == null || ticket.isEmpty ? null : ticket,
    );
  }

  static const maxPreferencesBytes = 4 * 1024 * 1024;
  static const _partBytes = 32768;
  int? _uiLocaleRevision;
  String? _uiLocale;
  (String, int, String)? _pendingUiLocale;
  Future<void> _uiLocaleQueue = Future.value();
  Future<String> readUiLocale() async {
    final reply = await _call(host.Action.readUiLocale);
    final locale = utf8.decode(reply.payload ?? []);
    if (!const {'system', 'zh', 'en'}.contains(locale)) {
      throw const FormatException('Unsupported UI locale');
    }
    _uiLocaleRevision = reply.revision;
    _uiLocale = locale;
    return locale;
  }

  Future<void> _confirmUiLocale((String, int, String) request) async {
    final reply = await _call(
      host.Action.saveUiLocale,
      configure: (r) {
        r.operation = request.$1;
        r.revision = request.$2;
        r.payload = Uint8List.fromList(utf8.encode(request.$3));
      },
    );
    if (utf8.decode(reply.payload ?? []) != request.$3 ||
        reply.revision != request.$2 + 1) {
      throw const FormatException('Language preference receipt mismatch');
    }
    _uiLocale = request.$3;
    _uiLocaleRevision = reply.revision;
    _pendingUiLocale = null;
  }

  Future<void> saveUiLocale(String locale) {
    final result = _uiLocaleQueue.then((_) async {
      if (!const {'system', 'zh', 'en'}.contains(locale)) {
        throw const FormatException('Unsupported UI locale');
      }
      if (_uiLocaleRevision == null) await readUiLocale();
      // A later explicit save first confirms the exact earlier operation. It
      // never replaces an uncertain operation's locale or expected revision.
      // There is no timer-based or autonomous retry.
      if (_pendingUiLocale case final request?) {
        await _confirmUiLocale(request);
      }
      if (_uiLocale == locale) return;
      final request = _pendingUiLocale = (
        newQueryOperationId(),
        _uiLocaleRevision!,
        locale,
      );
      await _confirmUiLocale(request);
    });
    _uiLocaleQueue = result.catchError((Object _) {});
    return result;
  }

  Future<void> _preferencesQueue = Future.value();
  Future<T> _preferencesJob<T>(Future<T> Function() job) {
    final result = Completer<T>();
    _preferencesQueue = _preferencesQueue.then((_) async {
      try {
        result.complete(await job());
      } catch (error, stack) {
        result.completeError(error, stack);
      }
    });
    return result.future;
  }

  Future<void> _abortPreferences(String token) async {
    try {
      await _call(
        host.Action.abortPreferences,
        configure: (r) => r.transfer = token,
      );
    } catch (_) {
      /* A closed connection drops its bounded staging buffers. */
    }
  }

  Future<Uint8List?> readPreferences() => _preferencesJob(_readPreferences);
  Future<Uint8List?> _readPreferences() async {
    var response = await _call(host.Action.readPreferences);
    if (response.totalLength == 0 && (response.payload?.isEmpty ?? true)) {
      return null;
    }
    final token = response.transfer ?? '';
    final total = response.totalLength;
    final digest = response.sha256;
    try {
      if (token.isEmpty ||
          total <= 0 ||
          total > maxPreferencesBytes ||
          digest?.length != 32) {
        throw const FormatException('配置快照无效');
      }
      final result = BytesBuilder(copy: false);
      var offset = 0;
      while (true) {
        final part = response.payload;
        if (response.transfer != token ||
            response.totalLength != total ||
            response.offset != offset ||
            !_same(response.sha256, digest!) ||
            part == null ||
            part.isEmpty ||
            part.length > _partBytes ||
            part.length > total - offset) {
          throw const FormatException('配置片段不匹配');
        }
        result.add(part);
        offset += part.length;
        if (offset == total) break;
        response = await _call(
          host.Action.readPreferencesPart,
          configure: (r) {
            r.transfer = token;
            r.offset = offset;
          },
        );
      }
      final bytes = result.takeBytes();
      if (!_same(sha256.convert(bytes).bytes, digest)) {
        throw const FormatException('配置校验失败');
      }
      return bytes;
    } catch (_) {
      await _abortPreferences(token);
      rethrow;
    }
  }

  Future<Uint8List> savePreferences(Uint8List value) {
    // The caller may reuse its buffer while queued: freeze this proposal now.
    final bytes = Uint8List.fromList(value);
    return _preferencesJob(() async {
      if (bytes.isEmpty || bytes.length > maxPreferencesBytes) {
        throw const FormatException('配置超过 4 MiB，原设置保留');
      }
      final begin = await _call(
        host.Action.beginPreferences,
        configure: (r) {
          r.operation =
              'prefs-${DateTime.now().microsecondsSinceEpoch}-${_sequence++}';
          r.totalLength = bytes.length;
          r.sha256 = Uint8List.fromList(sha256.convert(bytes).bytes);
        },
      );
      final token = begin.transfer ?? '';
      if (token.isEmpty) throw const FormatException('缺少配置传输标识');
      try {
        for (var offset = 0; offset < bytes.length;) {
          final end = (offset + _partBytes).clamp(0, bytes.length);
          final reply = await _call(
            host.Action.appendPreferences,
            configure: (r) {
              r.transfer = token;
              r.offset = offset;
              r.payload = Uint8List.sublistView(bytes, offset, end);
            },
          );
          if (reply.transfer != token || reply.offset != end) {
            throw const FormatException('配置上传确认不匹配');
          }
          offset = end;
        }
        final committed = await _call(
          host.Action.finishPreferences,
          configure: (r) => r.transfer = token,
        );
        final stored = await _readPreferences();
        if (stored == null ||
            committed.totalLength != stored.length ||
            !_same(committed.sha256, sha256.convert(stored).bytes)) {
          throw const FormatException('配置提交后快照不匹配，请重新打开工作台确认');
        }
        return stored;
      } catch (_) {
        await _abortPreferences(token);
        rethrow;
      }
    });
  }

  Future<void> close() => _closingProcess ??= _closeProcess();

  Future<void> _closeProcess() async {
    Object? failure;
    StackTrace? failureStack;
    try {
      // Finish requests already accepted by the serialized channel before EOF.
      await _queue;
    } catch (error, stack) {
      failure = error;
      failureStack = stack;
    }
    try {
      await process.stdin.close();
    } catch (error, stack) {
      failure ??= error;
      failureStack ??= stack;
    }
    // EOF asks the CLI to stop and join its actual worker. A stop request is not
    // proof of exit, and forcibly killing it after five seconds loses the owner.
    final code = await process.exitCode;
    try {
      await _stderrDone;
    } catch (error, stack) {
      failure ??= error;
      failureStack ??= stack;
    }
    if (code != 0) {
      throw StateError('Content service shutdown failed ($code)');
    }
    if (failure != null) {
      Error.throwWithStackTrace(failure, failureStack!);
    }
  }
}

class _WorkbenchUiTransport implements PluginUiTransport {
  _WorkbenchUiTransport(this.backend, {this.entry, this.expectedRevision});
  final RustWorkbench backend;
  final PluginLibraryEntry? entry;
  final BigInt? expectedRevision;
  BigInt? _generation;
  Future<PluginUiReply>? _opening;
  Future<void>? _closing;
  bool _closed = false;
  PluginUiReply _reply(host.ResponseReader r) {
    final generation = BigInt.from(r.uiGeneration).toUnsigned(64);
    _generation ??= generation;
    if (_generation != generation) throw const FormatException('表单代次不匹配');
    final failure = switch (r.uiCode) {
      0 => null,
      1 => PluginUiFailureKind.rejected,
      2 => PluginUiFailureKind.plugin,
      3 => PluginUiFailureKind.execution,
      4 => PluginUiFailureKind.unavailable,
      _ => throw const FormatException('未知表单状态'),
    };
    return PluginUiReply(
      view: r.uiView ?? '',
      generation: generation,
      revision: BigInt.from(r.revision).toUnsigned(64),
      serial: BigInt.from(r.uiSerial).toUnsigned(64),
      documentBytes: failure == null ? r.payload : null,
      failure: failure == null
          ? null
          : PluginUiFailure(
              failure,
              r.uiFailure ?? '',
              hostMessage: switch (failure) {
                PluginUiFailureKind.rejected => PluginUiHostMessage.rejected,
                PluginUiFailureKind.execution => PluginUiHostMessage.execution,
                PluginUiFailureKind.unavailable =>
                  PluginUiHostMessage.unavailable,
                _ => null,
              },
            ),
    );
  }

  @override
  Future<PluginUiReply> open(String seed) {
    if (_closed || _opening != null) throw StateError('表单已打开或关闭');
    return _opening = backend
        ._call(
          entry == null ? host.Action.uiOpen : host.Action.externalUiOpen,
          configure: (r) {
            r.name = seed;
            if (entry != null) {
              backend._externalSelection(r, entry!, expectedRevision!);
            }
          },
        )
        .then(_reply);
  }

  @override
  Future<PluginUiReply> event(Uint8List bytes) async {
    if (_closed || _generation == null) throw StateError('表单不可用');
    return _reply(
      await backend._call(
        entry == null ? host.Action.uiEvent : host.Action.externalUiEvent,
        configure: (r) {
          if (entry != null) r.id = entry!.id;
          r.offset = _generation!.toSigned(64).toInt();
          r.payload = bytes;
        },
      ),
    );
  }

  @override
  Future<void> close() {
    final current = _closing;
    if (current != null) return current;
    late Future<void> observed;
    observed = _close().then<void>(
      (_) {},
      onError: (Object error, StackTrace stack) {
        if (identical(_closing, observed)) _closing = null;
        Error.throwWithStackTrace(error, stack);
      },
    );
    return _closing = observed;
  }

  Future<void> _close() async {
    _closed = true;
    try {
      await _opening;
    } catch (_) {
      return;
    }
    if (_generation != null) {
      await backend._call(
        entry == null ? host.Action.uiClose : host.Action.externalUiClose,
        configure: (r) {
          if (entry != null) r.id = entry!.id;
          r.offset = _generation!.toSigned(64).toInt();
        },
      );
    }
  }
}

class _NativeEditorSession implements WorkbenchEditorSession {
  _NativeEditorSession(
    this.owner,
    this.targetId,
    this.revision,
    this.scope,
    this.create,
  );
  final RustWorkbench owner;
  @override
  final String targetId;
  final int revision;
  final String scope;
  final bool create;
  @override
  late final RustStudioPlugin studio = RustStudioPlugin(
    owner,
    captureScope: scope,
  );
  bool _closed = false;
  Future<void>? _closing;
  String? _operation, _fingerprint;
  Idea? _draft;
  EditorFields? _fields;
  final _attachments = <IdeaAttachment>[];
  Uint8List? _pendingBytes;
  Future<Idea>? _inFlight;

  @override
  Future<void> recordPaste(PasteInsertion insertion) async {
    if (_closed || _operation != null) throw StateError('此编辑会话已结束或正在等待提交确认。');
    final message = MessageBuilder();
    final upload = message.initRoot(host.pasteUploadFactory);
    upload.scope = scope;
    final event = upload.initEvent();
    event.id = insertion.id;
    event.field = insertion.field;
    event.before = insertion.before;
    event.startUtf16 = insertion.startUtf16;
    event.endUtf16 = insertion.endUtf16;
    event.after = insertion.after;
    final parts = event.initParts(insertion.parts.length);
    for (var i = 0; i < insertion.parts.length; i++) {
      final part = insertion.parts[i];
      parts[i].ticket = part.ticket;
      parts[i].literal = part.literal;
      parts[i].selection = part.selection;
    }
    await owner._captureUpload(
      insertion.id,
      message.serialize(),
      host.Action.finishPaste,
    );
  }

  @override
  Future<Idea> save(Idea draft, EditorFields fields) {
    if (_closed) return Future.error(StateError('编辑器会话已关闭。'));
    final fingerprint = jsonEncode([
      draft.toJson(),
      fields.title,
      fields.description,
      fields.hypothesis,
      fields.conclusion,
      fields.todos,
    ]);
    if (_fingerprint != null && _fingerprint != fingerprint) {
      return Future.error(StateError('上次提交尚未确认，请先重试原提交，不能更换其内容。'));
    }
    if (draft.id != targetId) return Future.error(StateError('编辑器目标不匹配。'));
    _fingerprint ??= fingerprint;
    _operation ??=
        'editor-${DateTime.now().microsecondsSinceEpoch}-${owner._sequence++}';
    _fields ??= fields;
    _draft ??= Idea(
      draft.title,
      draft.description,
      draft.category,
      draft.icon,
      draft.color,
      id: draft.id,
      favorite: draft.favorite,
      time: draft.time,
      stage: draft.stage,
      hypothesis: draft.hypothesis,
      conclusion: draft.conclusion,
      attachments: List.unmodifiable(draft.attachments),
      todos: List.unmodifiable(draft.todos),
      completed: Set.unmodifiable(draft.completed),
    );
    return _inFlight ??= _save().whenComplete(() => _inFlight = null);
  }

  Future<Idea> _save() async {
    final draft = _draft!;
    final fields = _fields!;
    if (_pendingBytes == null) {
      try {
        while (_attachments.length < draft.attachments.length) {
          _attachments.add(
            await owner._importAttachment(
              targetId,
              draft.attachments[_attachments.length],
            ),
          );
        }
        final request = MessageBuilder();
        final r = request.initRoot(wire.requestFactory);
        r.version = 1;
        r.digest = Uint8List.fromList(contract.workbenchDigest);
        r.action = create ? wire.Action.create : wire.Action.edit;
        owner._writeIdea(r.initProposed(), draft, _attachments);
        final payload = request.serialize();
        if (payload.length > 65536) {
          throw const FormatException('当前插件消息容量不足，请缩短正文或改为附件。');
        }
        final message = MessageBuilder();
        final save = message.initRoot(host.capturedSaveFactory);
        save.scope = scope;
        save.operation = _operation;
        save.target = targetId;
        save.revision = revision;
        save.payload = payload;
        final snapshot = save.initSnapshot();
        snapshot.title = fields.title;
        snapshot.description = fields.description;
        snapshot.hypothesis = fields.hypothesis;
        snapshot.conclusion = fields.conclusion;
        snapshot.todos = fields.todos;
        final aliases = snapshot.initAliases(_attachments.length);
        for (var i = 0; i < _attachments.length; i++) {
          aliases[i].id = _attachments[i].pluginId;
          aliases[i].location = draft.attachments[i].source.location;
          aliases[i].name = draft.attachments[i].source.name;
        }
        _pendingBytes = message.serialize();
        if (_pendingBytes!.length > 4 * 1024 * 1024) {
          throw const FormatException('编辑器记录超过 4 MiB，请减少本次粘贴内容。');
        }
      } catch (error) {
        _operation = null;
        _fingerprint = null;
        _draft = null;
        _fields = null;
        _pendingBytes = null;
        _attachments.clear();
        throw EditorPreparationException(error);
      }
    }
    final reply = await owner._captureUpload(
      _operation!,
      _pendingBytes!,
      host.Action.finishCapturedSave,
    );
    // Decoding/export can fail after a successful commit. Keep the exact operation and
    // payload so another save asks for its historical receipt, never a new mutation.
    return owner._idea(reply);
  }

  @override
  Future<void> close() {
    if (_closing != null) return _closing!;
    _closed = true;
    return _closing = owner
        ._call(
          host.Action.closeCaptureScope,
          configure: (r) => r.captureScope = scope,
        )
        .then((_) {});
  }
}
