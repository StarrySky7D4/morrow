import 'dart:async';
import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:morrow_studio/plugins/http_task_manager.dart';
import 'package:morrow_studio/plugins/io_task_control.dart';
import 'package:morrow_studio/plugins/plugin_library.dart';
import 'package:morrow_studio/plugins/service_control.dart';
import 'package:morrow_studio/plugins/service_run_control.dart';
import 'package:morrow_studio/plugins/service_run_manager.dart';
import 'package:morrow_studio/plugins/service_run_session.dart';
import 'package:morrow_studio/plugins/service_session.dart';

import 'service_manager_fakes.dart';
import 'http_task_manager_test.dart' show FakeEndpoints;

IoTaskSnapshot _local() => IoTaskSnapshot(
  storage: IoStoragePhase.local,
  delivery: IoDeliveryPhase.absent,
  exit: null,
);

class _RunBackend
    implements WorkbenchServiceRunControl, WorkbenchIoTaskControl {
  final starts = <ServiceRunRequest>[];
  final stops = <Uint8List>[];
  final acknowledgements = <Uint8List>[];
  int inspections = 0;
  ServiceRunSnapshot? current;
  Future<ServiceRunSnapshot> Function(ServiceRunRequest)? onStart;

  ServiceRunSnapshot snapshot(
    ServiceRunRequest request, {
    ServiceRunPhase phase = ServiceRunPhase.running,
    IoStoragePhase storage = IoStoragePhase.running,
    bool exited = false,
  }) => ServiceRunSnapshot(
    task: IoTaskSnapshot(
      key: serviceKey(900),
      submission: request.submission,
      storage: storage,
      delivery: IoDeliveryPhase.absent,
      exit: exited
          ? const IoTaskExit(
              execution: IoJobError.none,
              disconnect: IoJobError.none,
              maintenance: IoJobError.none,
            )
          : null,
    ),
    submission: request.submission,
    phase: phase,
    address: '127.0.0.1:34567',
    bind: ServiceNetworkOutcome.succeeded,
    listener: exited
        ? ServiceNetworkOutcome.succeeded
        : ServiceNetworkOutcome.pending,
    supervision: exited
        ? ServiceNetworkOutcome.succeeded
        : ServiceNetworkOutcome.pending,
  );

  @override
  Future<ServiceRunSnapshot> startServiceRun(ServiceRunRequest request) async {
    starts.add(request);
    final value =
        await (onStart?.call(request) ?? Future.value(snapshot(request)));
    current = value;
    return value;
  }

  @override
  Future<IoTaskSnapshot> ioStatus() async {
    inspections++;
    return current?.task ?? _local();
  }

  @override
  Future<IoTaskSnapshot> pollIo(Uint8List key) async {
    expect(key, current?.task.key);
    return ioStatus();
  }

  @override
  Future<ServiceRunSnapshot> serviceRunStatus({Uint8List? key}) async {
    expect(key, current?.task.key);
    return current ?? (throw StateError('No service'));
  }

  @override
  Future<IoTaskSnapshot> cancelIo(Uint8List key) async {
    expect(key, current!.task.key);
    stops.add(Uint8List.fromList(key));
    current = snapshot(
      starts.single,
      phase: ServiceRunPhase.stopping,
      storage: IoStoragePhase.stopping,
    );
    return current!.task;
  }

  @override
  Future<IoTaskSnapshot> acknowledgeIo(Uint8List key) async {
    expect(key, current!.task.key);
    expect(current!.phase, ServiceRunPhase.exited);
    expect(current!.task.storage, IoStoragePhase.reclaimed);
    expect(current!.task.exit, isNotNull);
    acknowledgements.add(Uint8List.fromList(key));
    current = null;
    return _local();
  }

  @override
  dynamic noSuchMethod(Invocation invocation) =>
      throw StateError('Unexpected control ${invocation.memberName}');
}

PluginLibraryEntry _plugin({
  bool enabled = true,
  bool available = true,
  bool approved = true,
  int digest = 3,
}) => PluginLibraryEntry(
  id: 'example.package',
  name: 'Approved service package',
  version: '1',
  digest: serviceKey(digest),
  enabled: enabled,
  builtin: false,
  available: available,
  declared: const [],
  approved: const [],
  dependencies: const [],
  handlers: const [],
  issue: '',
  ioHandlers: const ['invoke'],
  declaredIo: const ['http-listen', 'http-publish'],
  approvedIo: approved ? const ['http-listen', 'http-publish'] : const [],
);

ServicePublication _policy({
  bool tls = false,
  String address = '127.0.0.1:8080',
}) => ServicePublication(
  configId: 'service-001',
  configDigest: serviceKey(20001),
  listenAddress: address,
  tlsRequired: tls,
  method: 'POST',
  path: '/api',
  queryPath: '/history',
);

ServiceFakeBackend _metadata({
  bool configDisabled = false,
  bool publicationDisabled = false,
  bool authDisabled = false,
  bool authExpired = false,
  bool publicationExpired = false,
  ServicePublication? policy,
}) {
  final now = BigInt.from(DateTime.now().millisecondsSinceEpoch);
  return ServiceFakeBackend()
    ..configs = [serviceConfig(disabled: configDisabled)]
    ..authorities = [
      serviceAuthority(
        disabled: authDisabled,
        createdMs: now - BigInt.from(10000),
        expiresMs: authExpired ? now - BigInt.one : now + BigInt.from(3600000),
      ),
      serviceAuthority(
        identity: 10001,
        publication: policy ?? _policy(),
        disabled: publicationDisabled,
        createdMs: now - BigInt.from(10000),
        expiresMs: publicationExpired
            ? now - BigInt.one
            : now + BigInt.from(3600000),
      ),
    ];
}

Widget _host(
  _RunBackend run,
  ServiceFakeBackend metadata, {
  String locale = 'en',
  int revision = 7,
  PluginLibraryEntry? plugin,
  bool showHttp = false,
  bool catalogReady = true,
  bool showRunPanel = true,
}) => MaterialApp(
  locale: Locale(locale),
  localizationsDelegates: AppLocalizations.localizationsDelegates,
  supportedLocales: AppLocalizations.supportedLocales,
  home: Scaffold(
    body: SingleChildScrollView(
      key: const PageStorageKey('service-panel-scroll'),
      child: Column(
        children: [
          if (showRunPanel)
            ServiceRunManager(
              backend: run,
              ioBackend: run,
              metadataBackend: metadata,
              plugins: catalogReady ? [plugin ?? _plugin()] : [],
              registryRevision: catalogReady ? BigInt.from(revision) : null,
              ink: Colors.black,
              muted: Colors.grey,
              line: Colors.grey,
              radius: BorderRadius.circular(12),
            ),
          if (showHttp)
            HttpTaskManager(
              backend: run,
              endpointBackend: FakeEndpoints(),
              plugins: const [],
              registryRevision: BigInt.from(revision),
              ink: Colors.black,
              muted: Colors.grey,
              line: Colors.grey,
              radius: BorderRadius.circular(12),
              serviceSession: ServiceRunSession.forBackend(run, run),
            ),
        ],
      ),
    ),
  ),
);

Finder _find(String key) => find.byKey(ValueKey('service-run-$key'));
OutlinedButton _button(WidgetTester tester, String key) =>
    tester.widget(_find(key));
TextEditingController _field(WidgetTester tester, String key) =>
    tester.widget<TextField>(_find(key)).controller!;

Future<void> _click(WidgetTester tester, String key) async {
  await tester.ensureVisible(_find(key));
  await tester.pumpAndSettle();
  expect(_find(key).hitTestable(), findsOneWidget);
  await tester.tap(_find(key));
  await tester.pumpAndSettle();
}

Future<void> _enter(WidgetTester tester, String key, String value) async {
  await tester.ensureVisible(_find(key));
  await tester.enterText(_find(key), value);
  await tester.pumpAndSettle();
}

Future<void> _select(WidgetTester tester) async {
  final dropdown = find.descendant(
    of: find.byType(ServiceRunManager),
    matching: find.byType(DropdownButtonFormField<String>),
  );
  await tester.ensureVisible(dropdown);
  await tester.tap(dropdown);
  await tester.pumpAndSettle();
  await tester.tap(find.textContaining('Approved service package ·').last);
  await tester.pumpAndSettle();
}

Future<void> _mount(WidgetTester tester, Widget widget) async {
  await tester.pumpWidget(widget);
  await tester.pumpAndSettle();
}

void _cleanup(WidgetTester tester) {
  addTearDown(() async => tester.pumpWidget(const SizedBox.shrink()));
}

void main() {
  int choices(WidgetTester tester) => tester
      .widget<DropdownButton<String>>(
        find.descendant(
          of: find.byType(ServiceRunManager),
          matching: find.byType(DropdownButton<String>),
        ),
      )
      .items!
      .length;

  for (final sharedRefresh in [false, true]) {
    testWidgets(
      'async catalog joins ${sharedRefresh ? "shared" : "own"} metadata load',
      (tester) async {
        _cleanup(tester);
        final run = _RunBackend(), metadata = _metadata();
        final gate = Completer<ServiceConfigPage>();
        metadata.onConfigPage = (_, _) => gate.future;
        final pending = sharedRefresh
            ? ServiceSession.forBackend(metadata).refresh()
            : null;
        await _mount(tester, _host(run, metadata, catalogReady: false));
        await _mount(tester, _host(run, metadata, revision: 8));
        expect(choices(tester), 0);
        expect(_button(tester, 'start').onPressed, isNull);
        metadata.onConfigPage = null;
        gate.complete(
          ServiceConfigPage(
            configs: metadata.configs,
            snapshot: serviceKey(metadata.generation),
            next: null,
          ),
        );
        await pending;
        await tester.pumpAndSettle();
        expect(choices(tester), 1);
        expect(run.starts, isEmpty);
        expect(metadata.writes, 0);
        await _select(tester);
        await _click(tester, 'start');
        expect(run.starts.single.registryRevision, BigInt.from(8));
      },
    );
  }

  testWidgets('catalog refresh failure stays blocked without a retry loop', (
    tester,
  ) async {
    _cleanup(tester);
    final run = _RunBackend(), metadata = _metadata();
    await _mount(tester, _host(run, metadata, catalogReady: false));
    metadata.onConfigPage = (_, _) async => throw StateError('offline');
    await _mount(tester, _host(run, metadata));
    final attempts = metadata.configPages.length;
    await tester.pump(const Duration(seconds: 5));
    await tester.pumpAndSettle();
    expect(metadata.configPages.length, attempts);
    expect(choices(tester), 0);
    expect(run.starts, isEmpty);
    metadata.onConfigPage = null;
    await _click(tester, 'refresh-records');
    expect(choices(tester), 1);
    expect(metadata.writes, 0);
  });

  testWidgets('failed shared metadata read is not automatically retried', (
    tester,
  ) async {
    _cleanup(tester);
    final run = _RunBackend(), metadata = _metadata();
    final gate = Completer<ServiceConfigPage>();
    metadata.onConfigPage = (_, _) => gate.future;
    final pending = ServiceSession.forBackend(metadata).refresh();
    await _mount(tester, _host(run, metadata));
    gate.completeError(StateError('shared read failed'));
    await pending;
    await tester.pumpAndSettle();
    await tester.pump(const Duration(seconds: 3));
    expect(metadata.configPages, hasLength(1));
    expect(choices(tester), 0);
    expect(_button(tester, 'start').onPressed, isNull);
    expect(run.starts, isEmpty);
  });

  testWidgets('old metadata completion cannot refresh a replacement backend', (
    tester,
  ) async {
    _cleanup(tester);
    final oldRun = _RunBackend(), oldMetadata = _metadata();
    final gate = Completer<ServiceConfigPage>();
    oldMetadata.onConfigPage = (_, _) => gate.future;
    await _mount(tester, _host(oldRun, oldMetadata, catalogReady: false));
    final run = _RunBackend(), metadata = _metadata();
    await _mount(tester, _host(run, metadata));
    final reads = metadata.configPages.length;
    gate.complete(
      ServiceConfigPage(
        configs: oldMetadata.configs,
        snapshot: serviceKey(oldMetadata.generation),
        next: null,
      ),
    );
    await tester.pumpAndSettle();
    expect(metadata.configPages.length, reads);
    expect(choices(tester), 1);
    await _select(tester);
    await _click(tester, 'start');
    expect(oldRun.starts, isEmpty);
    expect(run.starts, hasLength(1));
    expect(tester.takeException(), isNull);
  });

  testWidgets(
    'parent scroll cannot corrupt advanced expansion on remount',
    (tester) async {
      _cleanup(tester);
      final run = _RunBackend(), metadata = _metadata();
      await _mount(tester, _host(run, metadata));
      await _click(tester, 'advanced');
      await tester.ensureVisible(_find('timeout'));
      await tester.pumpAndSettle();
      expect(_find('timeout'), findsOneWidget);
      await _mount(tester, _host(run, metadata, showRunPanel: false));
      await _mount(tester, _host(run, metadata));
      expect(_find('timeout'), findsNothing);
      await _click(tester, 'advanced');
      expect(_find('timeout'), findsOneWidget);
      expect(tester.takeException(), isNull);
      expect(run.starts, isEmpty);
    },
  );

  testWidgets(
    'explicit approved selection pins authority and submitted budgets',
    (tester) async {
      _cleanup(tester);
      final run = _RunBackend(), metadata = _metadata();
      await _mount(tester, _host(run, metadata));
      expect(_button(tester, 'start').onPressed, isNull);
      expect(run.starts, isEmpty);
      await _select(tester);
      await _enter(tester, 'lifetime', '45000');
      await _enter(tester, 'jobs', '9');
      await _enter(tester, 'bytes', '2097152');
      await _click(tester, 'advanced');
      await _enter(tester, 'calls', '2');
      await _enter(tester, 'job-bytes', '524288');
      await _enter(tester, 'total-bytes', '2097152');
      await _enter(tester, 'request-bytes', '32768');
      await _enter(tester, 'response-bytes', '16384');
      await _enter(tester, 'header-bytes', '8192');
      await _enter(tester, 'concurrent', '2');
      await _enter(tester, 'timeout', '4000');
      await _click(tester, 'start');
      final request = run.starts.single;
      expect(request.submission.length, 32);
      expect(request.submission.any((v) => v != 0), isTrue);
      expect(request.configId, 'service-001');
      expect(request.configDigest, serviceKey(20001));
      expect(request.configRevision, BigInt.one);
      expect(request.publication, serviceKey(10001));
      expect(request.publicationRevision, BigInt.one);
      expect(request.packageId, 'example.package');
      expect(request.packageDigest, serviceKey(3));
      expect(request.registryRevision, BigInt.from(7));
      expect(request.lifetimeMs, 45000);
      expect(request.maxJobs, BigInt.from(9));
      expect(request.maxBytes, BigInt.from(2097152));
      expect(request.maxCalls, 2);
      expect(request.maxJobBytes, BigInt.from(524288));
      expect(request.maxTotalBytes, BigInt.from(2097152));
      expect(request.maxRequestBytes, 32768);
      expect(request.maxResponseBytes, 16384);
      expect(request.maxHeaderBytes, 8192);
      expect(request.maxConcurrent, 2);
      expect(request.timeoutMs, 4000);
      expect(metadata.writes, 0);
      expect(_button(tester, 'start').onPressed, isNull);
      expect(_button(tester, 'stop').onPressed, isNotNull);
    },
  );

  testWidgets('invalid numeric draft and selection survive language switch', (
    tester,
  ) async {
    _cleanup(tester);
    final run = _RunBackend(), metadata = _metadata();
    await _mount(tester, _host(run, metadata));
    await _select(tester);
    await _enter(tester, 'lifetime', 'bad-number');
    await _click(tester, 'start');
    expect(run.starts, isEmpty);
    expect(_find('invalid'), findsOneWidget);
    final field = _field(tester, 'lifetime');
    field.selection = const TextSelection(baseOffset: 2, extentOffset: 5);
    await _mount(tester, _host(run, metadata, locale: 'zh'));
    expect(_field(tester, 'lifetime').text, 'bad-number');
    expect(
      _field(tester, 'lifetime').selection,
      const TextSelection(baseOffset: 2, extentOffset: 5),
    );
    expect(_button(tester, 'start').onPressed, isNotNull);
    expect(_find('invalid'), findsOneWidget);
    for (final invalid in ['0', '-1', '3600001', '999999999999999999999999']) {
      await _enter(tester, 'lifetime', invalid);
      await _click(tester, 'start');
      expect(run.starts, isEmpty);
    }
    await _enter(tester, 'lifetime', '45000');
    await _click(tester, 'start');
    expect(run.starts.single.lifetimeMs, 45000);
  });

  testWidgets('registry or package changes require new explicit selection', (
    tester,
  ) async {
    _cleanup(tester);
    final run = _RunBackend(), metadata = _metadata();
    await _mount(tester, _host(run, metadata));
    await _select(tester);
    await _enter(tester, 'jobs', '17');
    await _mount(tester, _host(run, metadata, revision: 8));
    expect(_button(tester, 'start').onPressed, isNull);
    expect(_find('stale'), findsOneWidget);
    await _click(tester, 'refresh-records');
    expect(_button(tester, 'start').onPressed, isNull);
    expect(run.starts, isEmpty);
    await _select(tester);
    await _mount(
      tester,
      _host(run, metadata, revision: 8, plugin: _plugin(digest: 4)),
    );
    expect(_button(tester, 'start').onPressed, isNull);
    await _click(tester, 'refresh-records');
    expect(_button(tester, 'start').onPressed, isNull);
    expect(_field(tester, 'jobs').text, '17');
    expect(run.starts, isEmpty);
  });

  testWidgets(
    'refreshed publication revision cannot silently replace selection',
    (tester) async {
      _cleanup(tester);
      final run = _RunBackend(), metadata = _metadata();
      await _mount(tester, _host(run, metadata));
      await _select(tester);
      metadata.authorities = [
        metadata.authorities.first,
        serviceAuthority(
          identity: 10001,
          revision: BigInt.two,
          publication: _policy(),
        ),
      ];
      metadata.generation++;
      await _click(tester, 'refresh-records');
      expect(_find('stale'), findsOneWidget);
      expect(_button(tester, 'start').onPressed, isNull);
      expect(run.starts, isEmpty);
      await _select(tester);
      await _click(tester, 'start');
      expect(run.starts.single.publicationRevision, BigInt.two);
    },
  );

  for (final condition in [
    'unapproved',
    'plugin-disabled',
    'unavailable',
    'tls',
    'nonloopback',
    'expired-auth',
    'disabled-auth',
    'expired-publication',
    'disabled-publication',
    'disabled-config',
  ]) {
    testWidgets('$condition candidate cannot start a listener', (tester) async {
      _cleanup(tester);
      final run = _RunBackend();
      final metadata = _metadata(
        configDisabled: condition == 'disabled-config',
        publicationDisabled: condition == 'disabled-publication',
        authDisabled: condition == 'disabled-auth',
        authExpired: condition == 'expired-auth',
        publicationExpired: condition == 'expired-publication',
        policy: _policy(
          tls: condition == 'tls',
          address: condition == 'nonloopback'
              ? '0.0.0.0:8080'
              : '127.0.0.1:8080',
        ),
      );
      await _mount(
        tester,
        _host(
          run,
          metadata,
          plugin: _plugin(
            approved: condition != 'unapproved',
            enabled: condition != 'plugin-disabled',
            available: condition != 'unavailable',
          ),
        ),
      );
      final choices = tester.widget<DropdownButton<String>>(
        find.byType(DropdownButton<String>),
      );
      expect(choices.items, isEmpty);
      expect(_button(tester, 'start').onPressed, isNull);
      expect(run.starts, isEmpty);
    });
  }

  testWidgets(
    'stop remains available and acknowledgement waits for actual owner return',
    (tester) async {
      _cleanup(tester);
      final run = _RunBackend(), metadata = _metadata();
      await _mount(tester, _host(run, metadata));
      await _select(tester);
      await _click(tester, 'start');
      expect(_find('address'), findsOneWidget);
      // A slow metadata refresh must not monopolize the independent controls.
      final metadataGate = Completer<ServiceConfigPage>();
      metadata.onConfigPage = (_, _) => metadataGate.future;
      await _click(tester, 'refresh-records');
      expect(_button(tester, 'stop').onPressed, isNotNull);
      expect(_button(tester, 'acknowledge').onPressed, isNull);
      await _click(tester, 'stop');
      expect(run.stops, [serviceKey(900)]);
      metadataGate.complete(
        ServiceConfigPage(
          configs: metadata.configs,
          snapshot: serviceKey(metadata.generation),
          next: null,
        ),
      );
      metadata.onConfigPage = null;
      await tester.pumpAndSettle();
      expect(_button(tester, 'acknowledge').onPressed, isNull);
      await _click(tester, 'refresh');
      expect(run.acknowledgements, isEmpty);
      run.current = run.snapshot(
        run.starts.single,
        phase: ServiceRunPhase.exited,
        storage: IoStoragePhase.recoveryRequired,
        exited: true,
      );
      await _click(tester, 'refresh');
      expect(_button(tester, 'acknowledge').onPressed, isNull);
      expect(_button(tester, 'repair').onPressed, isNotNull);
      run.current = run.snapshot(
        run.starts.single,
        phase: ServiceRunPhase.exited,
        storage: IoStoragePhase.reclaimed,
        exited: true,
      );
      await _click(tester, 'refresh');
      expect(_button(tester, 'acknowledge').onPressed, isNotNull);
      await _click(tester, 'acknowledge');
      expect(run.acknowledgements, [serviceKey(900)]);
      expect(run.starts.length, 1);
      expect(_find('identity'), findsNothing);
    },
  );

  testWidgets(
    'unmount pending and unknown start never resends original attempt',
    (tester) async {
      _cleanup(tester);
      final run = _RunBackend(), metadata = _metadata();
      final gate = Completer<ServiceRunSnapshot>();
      run.onStart = (_) => gate.future;
      await _mount(tester, _host(run, metadata));
      await _select(tester);
      await _click(tester, 'start');
      final attempt = run.starts.single;
      await _mount(tester, const SizedBox.shrink());
      await _mount(tester, _host(run, metadata));
      expect(_button(tester, 'start').onPressed, isNull);
      expect(run.starts.length, 1);
      gate.completeError(StateError('start receipt lost'));
      await tester.pumpAndSettle();
      expect(_find('attempt'), findsOneWidget);
      await _mount(tester, const SizedBox.shrink());
      await _mount(tester, _host(run, metadata, locale: 'zh'));
      await tester.pump(const Duration(seconds: 2));
      await tester.pumpAndSettle();
      expect(run.starts.length, 1);
      expect(_button(tester, 'start').onPressed, isNull);
      expect(_find('attempt'), findsOneWidget);
      final shown = tester.widget<SelectableText>(_find('attempt')).data!;
      expect(
        shown,
        attempt.submission
            .map((v) => v.toRadixString(16).padLeft(2, '0'))
            .join(),
      );
    },
  );

  testWidgets(
    'run panel does not retain a token after its sole administration viewer leaves',
    (tester) async {
      _cleanup(tester);
      final run = _RunBackend(), metadata = _metadata();
      final session = ServiceSession.forBackend(metadata);
      session.attach();
      await _mount(tester, _host(run, metadata));
      final issued = serviceIssued();
      session.issued = issued;
      final held = Uint8List.sublistView(issued.token.bytes);
      session.detach();
      await tester.pumpAndSettle();
      expect(find.byType(ServiceRunManager), findsOneWidget);
      expect(session.issued, isNull);
      expect(issued.token.isDisposed, isTrue);
      expect(held, everyElement(0));
    },
  );

  testWidgets(
    'service and HTTP panels share ownership without losing the HTTP draft',
    (tester) async {
      _cleanup(tester);
      final run = _RunBackend(), metadata = _metadata();
      await _mount(tester, _host(run, metadata, showHttp: true));
      final target = find.byKey(const ValueKey('http-task-target'));
      final body = find.byKey(const ValueKey('http-task-body'));
      await tester.ensureVisible(target);
      await tester.enterText(target, '/pending-api?mode=preview');
      await tester.pumpAndSettle();
      await tester.ensureVisible(body);
      await tester.enterText(body, 'original unsent HTTP body');
      await tester.pumpAndSettle();
      final draftTarget = tester.widget<TextField>(target).controller!;
      final draftBody = tester.widget<TextField>(body).controller!;
      await _select(tester);
      await _click(tester, 'start');
      expect(
        find.byKey(const ValueKey('http-task-service-active')),
        findsOneWidget,
      );
      for (final action in ['read', 'cancel', 'ack']) {
        expect(find.byKey(ValueKey('http-task-$action')), findsNothing);
      }
      expect(run.starts.length, 1);
      await _click(tester, 'stop');
      expect(run.stops.length, 1);
      expect(
        find.byKey(const ValueKey('http-task-service-active')),
        findsOneWidget,
      );
      run.current = run.snapshot(
        run.starts.single,
        phase: ServiceRunPhase.exited,
        storage: IoStoragePhase.reclaimed,
        exited: true,
      );
      await _click(tester, 'refresh');
      expect(
        find.byKey(const ValueKey('http-task-service-active')),
        findsOneWidget,
      );
      expect(run.acknowledgements, isEmpty);
      await _click(tester, 'acknowledge');
      expect(
        find.byKey(const ValueKey('http-task-service-active')),
        findsNothing,
      );
      expect(run.acknowledgements.length, 1);
      expect(tester.widget<TextField>(target).controller, same(draftTarget));
      expect(tester.widget<TextField>(body).controller, same(draftBody));
      expect(draftTarget.text, '/pending-api?mode=preview');
      expect(draftBody.text, 'original unsent HTTP body');
      expect(run.starts.length, 1);
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets(
    'rejected start diagnostic survives narrow bilingual remount until explicit new start',
    (tester) async {
      _cleanup(tester);
      tester.view.physicalSize = const Size(320, 640);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      final run = _RunBackend(), metadata = _metadata();
      const detail =
          'Service admission rejected: the requested execution budget exceeds the approved package declaration. 请检查运行预算后再重试。';
      run.onStart = (_) => Future.error(const ServiceRunStartFailure(detail));
      await _mount(tester, _host(run, metadata));
      await _select(tester);
      await _click(tester, 'start');
      final original = run.starts.single;
      final session = ServiceRunSession.forBackend(run, run);
      expect(session.startUnknown, isFalse);
      expect(session.history.single.request.submission, original.submission);
      expect(session.history.single.outcomeUnknown, isFalse);
      expect(session.history.single.startFailureDetail, detail);
      expect(_button(tester, 'start').onPressed, isNotNull);
      expect(
        tester.widget<Text>(_find('failure-detail')).data,
        contains(detail),
      );
      final english = tester.widget<Text>(_find('notice')).data;
      await tester.ensureVisible(_find('failure-detail'));
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);
      await _mount(tester, _host(run, metadata, locale: 'zh'));
      expect(tester.widget<Text>(_find('notice')).data, isNot(english));
      expect(
        tester.widget<Text>(_find('failure-detail')).data,
        contains(detail),
      );
      expect(run.starts.length, 1);
      await _mount(tester, const SizedBox.shrink());
      await _mount(tester, _host(run, metadata));
      await _click(tester, 'refresh');
      await _click(tester, 'refresh-records');
      expect(
        tester.widget<Text>(_find('failure-detail')).data,
        contains(detail),
      );
      expect(run.starts.length, 1);
      expect(_button(tester, 'start').onPressed, isNull);
      run.onStart = null;
      await _select(tester);
      expect(
        run.starts.length,
        1,
        reason: 'selecting a candidate never resubmits a start',
      );
      await _click(tester, 'start');
      expect(run.starts.length, 2);
      expect(
        run.starts.last.submission,
        isNot(orderedEquals(original.submission)),
      );
      expect(run.starts.last.configId, original.configId);
      expect(run.starts.last.configDigest, original.configDigest);
      expect(run.starts.last.publication, original.publication);
      expect(run.starts.last.registryRevision, original.registryRevision);
      expect(_find('failure-detail'), findsNothing);
      expect(_button(tester, 'stop').onPressed, isNotNull);
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets(
    'narrow screen and locale switch keep all advanced fields accessible',
    (tester) async {
      _cleanup(tester);
      tester.view.physicalSize = const Size(320, 640);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      final run = _RunBackend(), metadata = _metadata();
      await _mount(tester, _host(run, metadata));
      await _select(tester);
      await _click(tester, 'advanced');
      await _enter(tester, 'timeout', '12345');
      expect(tester.takeException(), isNull);
      await _mount(tester, _host(run, metadata, locale: 'zh'));
      expect(_field(tester, 'timeout').text, '12345');
      await tester.ensureVisible(_find('start'));
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);
      expect(_button(tester, 'start').onPressed, isNotNull);
      expect(run.starts, isEmpty);
    },
  );
}
