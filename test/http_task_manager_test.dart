import 'dart:async';
import 'dart:convert';
import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:morrow_studio/plugins/endpoint_control.dart';
import 'package:morrow_studio/plugins/http_task_manager.dart';
import 'package:morrow_studio/plugins/io_task_control.dart';
import 'package:morrow_studio/plugins/plugin_library.dart';

Uint8List identity(int value) => Uint8List.fromList(List.filled(32, value));
String hex(int value) => value.toRadixString(16).padLeft(2, '0') * 32;
const cleanExit = IoTaskExit(
  execution: IoJobError.none,
  disconnect: IoJobError.none,
  maintenance: IoJobError.none,
);
IoTaskSnapshot snapshot({
  IoStoragePhase storage = IoStoragePhase.running,
  IoDeliveryPhase delivery = IoDeliveryPhase.pending,
  Uint8List? key,
  Uint8List? submission,
  IoTaskExit? exit,
}) => IoTaskSnapshot(
  key: storage == IoStoragePhase.local ? null : key ?? identity(42),
  submission: submission,
  storage: storage,
  delivery: delivery,
  exit: exit,
);
IoTaskResult result({
  int httpStatus = 404,
  Uint8List? body,
  bool unknown = false,
  bool cancelled = false,
  List<HttpTaskHeader>? headers,
}) => IoTaskResult(
  cancelled: cancelled,
  unknown: unknown,
  calls: BigInt.one,
  chargedBytes: BigInt.from(256),
  executionFault: IoExecutionFault.none,
  exitCode: 0,
  http: unknown
      ? null
      : HttpTaskOutcome(
          status: IoHttpStatus.completed,
          httpStatus: httpStatus,
          headers:
              headers ??
              [
                HttpTaskHeader(
                  name: 'x-tag',
                  value: Uint8List.fromList(utf8.encode('first')),
                ),
                HttpTaskHeader(
                  name: 'x-tag',
                  value: Uint8List.fromList(utf8.encode('second')),
                ),
              ],
          body: body ?? Uint8List.fromList([0, 1, 255]),
        ),
);
PluginLibraryEntry plugin({
  String id = 'test.http-ui',
  bool enabled = true,
  bool available = true,
  List<String> approved = const ['http-request'],
  List<String> profiles = const ['morrow.http.forward.v1'],
}) => PluginLibraryEntry(
  id: id,
  name: 'HTTP test package',
  version: '1',
  digest: identity(1),
  enabled: enabled,
  builtin: false,
  available: available,
  declared: [],
  approved: [],
  dependencies: [],
  handlers: [],
  issue: '',
  declaredIo: const ['http-request'],
  approvedIo: approved,
  ioHandlers: profiles,
);
StoredEndpoint endpoint({
  bool disabled = false,
  int reference = 2,
  String packageId = 'test.http-ui',
  int digest = 1,
  int profile = 1,
  List<String> methods = const ['GET', 'POST'],
}) => StoredEndpoint(
  reference: identity(reference),
  revision: BigInt.from(3),
  createdMs: BigInt.from(DateTime(2026, 1).millisecondsSinceEpoch),
  expiresMs: BigInt.from(DateTime(2099, 1).millisecondsSinceEpoch),
  disabled: disabled,
  policy: EndpointPolicy(
    packageId: packageId,
    packageDigest: identity(digest),
    origin: 'https://api.example.com',
    profile: profile,
    methods: methods,
    credentialReference: Uint8List(0),
    rootCertificate: Uint8List(0),
    maxRequestBytes: 65536,
    maxResponseBytes: 65536,
    maxHeaderBytes: 16384,
    maxConcurrent: 1,
    timeoutMs: 30000,
    maxFrameBytes: 131072,
  ),
);

class FakeEndpoints implements WorkbenchEndpointControl {
  List<StoredEndpoint> entries = [endpoint()];
  int reads = 0, writes = 0;
  Future<EndpointPage> Function()? page;
  @override
  Future<EndpointPage> endpointPage({
    Uint8List? after,
    Uint8List? snapshot,
  }) async {
    reads++;
    if (page != null) return page!();
    return EndpointPage(entries: entries, snapshot: identity(5));
  }

  @override
  Future<StoredEndpoint> saveEndpoint({
    required Uint8List reference,
    required BigInt expectedRevision,
    required BigInt registryRevision,
    required int lifetimeDays,
    required EndpointPolicy policy,
  }) async {
    writes++;
    throw StateError('HTTP request UI must never approve an endpoint');
  }

  @override
  Future<StoredEndpoint> disableEndpoint(StoredEndpoint expected) async {
    writes++;
    throw StateError('HTTP request UI must never mutate endpoint approval');
  }
}

class FakeTasks implements WorkbenchIoTaskControl {
  IoTaskSnapshot state = snapshot(
    storage: IoStoragePhase.local,
    delivery: IoDeliveryPhase.absent,
  );
  final starts = <HttpTaskRequest>[];
  final polls = <Uint8List>[],
      reads = <Uint8List>[],
      cancels = <Uint8List>[],
      repairs = <Uint8List>[],
      acknowledgements = <Uint8List>[];
  int statuses = 0;
  Future<IoTaskSnapshot> Function(HttpTaskRequest)? start;
  Future<IoTaskSnapshot> Function()? status;
  Future<IoTaskSnapshot> Function(Uint8List)? poll, cancel, repair, acknowledge;
  Future<IoTaskRead> Function(Uint8List)? read;
  IoTaskResult nextResult = result();
  @override
  Future<IoTaskSnapshot> startHttp(HttpTaskRequest request) async {
    starts.add(request);
    if (start != null) return start!(request);
    return state = snapshot(submission: request.submission);
  }

  @override
  Future<IoTaskSnapshot> ioStatus() async {
    statuses++;
    return status == null ? state : status!();
  }

  @override
  Future<IoTaskSnapshot> pollIo(Uint8List key) async {
    polls.add(Uint8List.fromList(key));
    return poll == null ? state : poll!(key);
  }

  @override
  Future<IoTaskRead> readIo(Uint8List key) async {
    reads.add(Uint8List.fromList(key));
    if (read != null) return read!(key);
    state = snapshot(
      key: key,
      submission: state.submission,
      storage: IoStoragePhase.reclaimed,
      delivery: IoDeliveryPhase.consumed,
      exit: cleanExit,
    );
    return IoTaskRead(snapshot: state, result: nextResult);
  }

  @override
  Future<IoTaskSnapshot> cancelIo(Uint8List key) async {
    cancels.add(Uint8List.fromList(key));
    if (cancel != null) return cancel!(key);
    return state = snapshot(
      key: key,
      submission: state.submission,
      storage: IoStoragePhase.stopping,
    );
  }

  @override
  Future<IoTaskSnapshot> repairIo(Uint8List key) async {
    repairs.add(Uint8List.fromList(key));
    if (repair != null) return repair!(key);
    return state = snapshot(
      key: key,
      submission: state.submission,
      storage: IoStoragePhase.reclaimed,
      delivery: IoDeliveryPhase.consumed,
      exit: cleanExit,
    );
  }

  @override
  Future<IoTaskSnapshot> acknowledgeIo(Uint8List key) async {
    acknowledgements.add(Uint8List.fromList(key));
    if (acknowledge != null) return acknowledge!(key);
    return state = snapshot(
      storage: IoStoragePhase.local,
      delivery: IoDeliveryPhase.absent,
    );
  }
}

Widget page(
  FakeTasks tasks,
  FakeEndpoints endpoints, {
  List<PluginLibraryEntry>? plugins,
  BigInt? revision,
  bool confirmed = true,
  String locale = 'en',
  VoidCallback? onChanged,
}) => MaterialApp(
  locale: Locale(locale),
  localizationsDelegates: AppLocalizations.localizationsDelegates,
  supportedLocales: AppLocalizations.supportedLocales,
  home: Scaffold(
    body: SingleChildScrollView(
      child: HttpTaskManager(
        backend: tasks,
        endpointBackend: endpoints,
        plugins: plugins ?? [plugin()],
        registryRevision: confirmed ? revision ?? BigInt.from(7) : null,
        ink: Colors.black,
        muted: Colors.grey,
        line: Colors.grey,
        radius: BorderRadius.circular(12),
        onChanged: onChanged,
      ),
    ),
  ),
);
Future<void> settle(WidgetTester tester) async {
  await tester.pump();
  await tester.pump(const Duration(milliseconds: 40));
  await tester.pump();
}

Future<void> click(WidgetTester tester, String key) async {
  final finder = find.byKey(ValueKey('http-task-$key'));
  await tester.ensureVisible(finder);
  await tester.tap(finder);
  await settle(tester);
}

Future<void> enter(WidgetTester tester, String key, String value) async {
  final finder = find.byKey(ValueKey('http-task-$key'));
  await tester.ensureVisible(finder);
  await tester.enterText(finder, value);
  await settle(tester);
}

Future<void> choose(WidgetTester tester, String key, String value) async {
  final widget = tester.widget(find.byKey(ValueKey('http-task-$key')));
  if (widget is DropdownButtonFormField<String>) {
    widget.onChanged!(value);
  } else {
    (widget as DropdownButton<String>).onChanged!(value);
  }
  await settle(tester);
}

bool enabled(WidgetTester tester, String key) {
  final widgets = tester
      .widgetList(find.byKey(ValueKey('http-task-$key')))
      .toList();
  if (widgets.isEmpty) return false;
  final widget = widgets.single;
  if (widget is ButtonStyleButton) return widget.onPressed != null;
  if (widget is IconButton) return widget.onPressed != null;
  throw StateError('unsupported action widget ${widget.runtimeType}');
}

Future<void> mount(
  WidgetTester tester,
  FakeTasks tasks,
  FakeEndpoints endpoints, {
  String locale = 'en',
  List<PluginLibraryEntry>? plugins,
  bool confirmed = true,
}) async {
  await tester.pumpWidget(
    page(
      tasks,
      endpoints,
      locale: locale,
      plugins: plugins,
      confirmed: confirmed,
    ),
  );
  await settle(tester);
}

Future<void> disposePage(WidgetTester tester) async {
  await tester.pumpWidget(const SizedBox.shrink());
  await settle(tester);
}

void main() {
  testWidgets(
    'only explicit submit sends every field and never changes approval',
    (tester) async {
      final tasks = FakeTasks(), endpoints = FakeEndpoints();
      final revision = (BigInt.one << 53) + BigInt.one;
      await tester.pumpWidget(page(tasks, endpoints, revision: revision));
      await settle(tester);
      expect(tasks.starts, isEmpty);
      expect(endpoints.writes, 0);
      await choose(tester, 'endpoint', hex(2));
      await choose(tester, 'method', 'POST');
      await enter(tester, 'target', '/v1/test?mode=exact');
      await enter(tester, 'headers', 'x-tag: first\nx-tag: second');
      await choose(tester, 'body-format', 'base64');
      await enter(tester, 'body', 'AAH/');
      await enter(tester, 'timeout', '2345');
      expect(tasks.starts, isEmpty);
      await click(tester, 'start');
      expect(tasks.starts, hasLength(1));
      final sent = tasks.starts.single;
      expect(sent.endpoint, orderedEquals(identity(2)));
      expect(sent.endpointRevision, BigInt.from(3));
      expect(sent.packageDigest, orderedEquals(identity(1)));
      expect(sent.registryRevision, revision);
      expect(sent.method, 'POST');
      expect(sent.target, '/v1/test?mode=exact');
      expect(sent.timeoutMs, 2345);
      expect(sent.body, orderedEquals([0, 1, 255]));
      expect(sent.headers.map((h) => h.name), ['x-tag', 'x-tag']);
      expect(sent.headers.map((h) => utf8.decode(h.value)), [
        'first',
        'second',
      ]);
      expect(sent.submission, hasLength(32));
      expect(sent.submission.any((v) => v != 0), isTrue);
      expect(endpoints.writes, 0);
      expect(enabled(tester, 'start'), isFalse);
      await disposePage(tester);
    },
  );

  testWidgets(
    'incompatible disabled or unapproved plugins never become implicitly enabled',
    (tester) async {
      for (final candidate in [
        plugin(enabled: false),
        plugin(available: false),
        plugin(approved: []),
        plugin(profiles: []),
      ]) {
        final tasks = FakeTasks(), endpoints = FakeEndpoints();
        await mount(tester, tasks, endpoints, plugins: [candidate]);
        expect(enabled(tester, 'start'), isFalse);
        expect(tasks.starts, isEmpty);
        expect(endpoints.writes, 0);
        await disposePage(tester);
      }
    },
  );

  testWidgets(
    'historical disabled TRACE metadata does not poison an executable POST option',
    (tester) async {
      final tasks = FakeTasks(),
          endpoints = FakeEndpoints()
            ..entries = [
              endpoint(reference: 1, disabled: true, methods: ['TRACE']),
              endpoint(methods: ['POST']),
            ];
      await mount(tester, tasks, endpoints);
      final choices = tester.widget<DropdownButton<String>>(
        find.descendant(
          of: find.byKey(const ValueKey('http-task-endpoint')),
          matching: find.byType(DropdownButton<String>),
        ),
      );
      expect(choices.items!.map((v) => v.value), [hex(2)]);
      expect(enabled(tester, 'start'), isTrue);
      await click(tester, 'start');
      expect(tasks.starts, hasLength(1));
      expect(tasks.starts.single.method, 'POST');
      expect(tasks.starts.single.endpoint, orderedEquals(identity(2)));
      expect(endpoints.writes, 0);
      await disposePage(tester);
    },
  );

  testWidgets(
    'unknown profile disabled package or changed digest only excludes that endpoint',
    (tester) async {
      for (final historical in [
        endpoint(reference: 1, profile: 99),
        endpoint(reference: 1, packageId: 'test.disabled'),
        endpoint(reference: 1, digest: 9),
      ]) {
        final tasks = FakeTasks(),
            endpoints = FakeEndpoints()
              ..entries = [
                historical,
                endpoint(methods: ['POST']),
              ];
        await mount(
          tester,
          tasks,
          endpoints,
          plugins: [
            plugin(),
            plugin(id: 'test.disabled', enabled: false),
          ],
        );
        final choices = tester.widget<DropdownButton<String>>(
          find.descendant(
            of: find.byKey(const ValueKey('http-task-endpoint')),
            matching: find.byType(DropdownButton<String>),
          ),
        );
        expect(choices.items!.map((v) => v.value), [hex(2)]);
        expect(enabled(tester, 'start'), isTrue);
        await click(tester, 'start');
        expect(tasks.starts.single.method, 'POST');
        expect(tasks.starts.single.endpoint, orderedEquals(identity(2)));
        expect(endpoints.writes, 0);
        await disposePage(tester);
      }
    },
  );

  testWidgets(
    'status-confirmed running after a lost start reply notifies availability once',
    (tester) async {
      final tasks = FakeTasks(), endpoints = FakeEndpoints();
      var changes = 0;
      tasks.start = (request) async {
        tasks.state = snapshot(submission: request.submission);
        throw StateError('lost start reply');
      };
      await tester.pumpWidget(
        page(tasks, endpoints, onChanged: () => changes++),
      );
      await settle(tester);
      expect(changes, 0);
      await click(tester, 'start');
      expect(changes, 0);
      await click(tester, 'status-refresh');
      expect(changes, 1);
      await click(tester, 'poll');
      await click(tester, 'poll');
      await click(tester, 'status-refresh');
      await tester.pump(const Duration(milliseconds: 1100));
      await settle(tester);
      expect(changes, 1);
      expect(tasks.starts, hasLength(1));
      await disposePage(tester);
      await tester.pumpWidget(
        page(tasks, endpoints, onChanged: () => changes++),
      );
      await settle(tester);
      expect(changes, 1);
      expect(tasks.starts, hasLength(1));
      expect(tasks.reads, isEmpty);
      await disposePage(tester);
    },
  );

  testWidgets(
    'ready delivery is not exit and 404 binary response preserves duplicate headers',
    (tester) async {
      final tasks = FakeTasks()
        ..state = snapshot(
          delivery: IoDeliveryPhase.ready,
          submission: identity(77),
        );
      final endpoints = FakeEndpoints();
      await mount(tester, tasks, endpoints);
      expect(enabled(tester, 'read'), isTrue);
      expect(enabled(tester, 'ack'), isFalse);
      expect(tasks.reads, isEmpty);
      expect(tasks.acknowledgements, isEmpty);
      await click(tester, 'read');
      expect(tasks.reads, hasLength(1));
      expect(tasks.reads.single, orderedEquals(identity(42)));
      expect(find.textContaining('404'), findsWidgets);
      expect(find.byKey(const ValueKey('http-task-result')), findsOneWidget);
      final l = L10n.of(tester.element(find.byType(HttpTaskManager)));
      expect(
        find.text(l.pluginsHttpTaskHttpResult(404, l.pluginsHttpTaskCompleted)),
        findsOneWidget,
      );
      expect(
        find.text(l.pluginsHttpTaskExecution(0, l.pluginsHttpTaskOk)),
        findsOneWidget,
      );
      expect(find.text(l.pluginsHttpTaskRemoteError), findsOneWidget);
      expect(find.textContaining('first'), findsWidgets);
      expect(find.textContaining('second'), findsWidgets);
      expect(find.textContaining('AAH/'), findsWidgets);
      expect(enabled(tester, 'ack'), isTrue);
      expect(tasks.starts, isEmpty);
      expect(tasks.cancels, isEmpty);
      await click(tester, 'ack');
      expect(tasks.acknowledgements, hasLength(1));
      expect(tasks.acknowledgements.single, orderedEquals(identity(42)));
      await disposePage(tester);
    },
  );

  testWidgets(
    'pending task remains controllable when endpoints are busy and catalog is unconfirmed',
    (tester) async {
      final tasks = FakeTasks()..state = snapshot(submission: identity(77));
      final endpoints = FakeEndpoints()
        ..page = () => Future.error(StateError('library busy'));
      await mount(tester, tasks, endpoints, confirmed: false, plugins: []);
      expect(enabled(tester, 'start'), isFalse);
      expect(enabled(tester, 'cancel'), isTrue);
      await click(tester, 'poll');
      expect(tasks.polls, isNotEmpty);
      await click(tester, 'cancel');
      expect(tasks.cancels, hasLength(1));
      expect(tasks.cancels.single, orderedEquals(identity(42)));
      expect(tasks.state.storage, IoStoragePhase.stopping);
      expect(enabled(tester, 'ack'), isFalse);
      expect(tasks.starts, isEmpty);
      expect(endpoints.writes, 0);
      await disposePage(tester);
    },
  );

  testWidgets(
    'recovery and acknowledgement require the returned original task',
    (tester) async {
      final tasks = FakeTasks()
        ..state = snapshot(
          storage: IoStoragePhase.recoveryRequired,
          delivery: IoDeliveryPhase.consumed,
          submission: identity(77),
          exit: const IoTaskExit(
            execution: IoJobError.none,
            disconnect: IoJobError.none,
            maintenance: IoJobError.unavailable,
          ),
        );
      await mount(tester, tasks, FakeEndpoints(), confirmed: false);
      expect(enabled(tester, 'repair'), isTrue);
      expect(enabled(tester, 'ack'), isFalse);
      await click(tester, 'repair');
      expect(tasks.repairs, hasLength(1));
      expect(tasks.repairs.single, orderedEquals(identity(42)));
      expect(enabled(tester, 'ack'), isTrue);
      await click(tester, 'ack');
      expect(tasks.acknowledgements, hasLength(1));
      expect(tasks.starts, isEmpty);
      await disposePage(tester);
    },
  );

  testWidgets(
    'unknown start checks status and never sends a second attempt including remount',
    (tester) async {
      final tasks = FakeTasks(), endpoints = FakeEndpoints();
      tasks.start = (request) async {
        tasks.state = snapshot(submission: request.submission);
        throw StateError('start reply lost');
      };
      await mount(tester, tasks, endpoints);
      await choose(tester, 'endpoint', hex(2));
      await click(tester, 'start');
      expect(tasks.starts, hasLength(1));
      await click(tester, 'status-refresh');
      expect(tasks.starts, hasLength(1));
      expect(tasks.statuses, greaterThan(1));
      expect(enabled(tester, 'start'), isFalse);
      expect(enabled(tester, 'cancel'), isTrue);
      await disposePage(tester);
      await mount(tester, tasks, endpoints);
      expect(tasks.starts, hasLength(1));
      expect(tasks.reads, isEmpty);
      expect(tasks.acknowledgements, isEmpty);
      expect(tasks.cancels, isEmpty);
      expect(enabled(tester, 'start'), isFalse);
      await disposePage(tester);
    },
  );

  testWidgets(
    'unknown read is never replayed even if status still says ready after remount',
    (tester) async {
      final tasks = FakeTasks()
        ..state = snapshot(
          delivery: IoDeliveryPhase.ready,
          submission: identity(77),
        );
      tasks.read = (_) async =>
          throw StateError('read response lost after consumption');
      final endpoints = FakeEndpoints();
      await mount(tester, tasks, endpoints);
      await click(tester, 'read');
      expect(tasks.reads, hasLength(1));
      expect(enabled(tester, 'read'), isFalse);
      await click(tester, 'status-refresh');
      expect(tasks.reads, hasLength(1));
      expect(enabled(tester, 'read'), isFalse);
      await disposePage(tester);
      await mount(tester, tasks, endpoints);
      expect(enabled(tester, 'read'), isFalse);
      expect(tasks.reads, hasLength(1));
      expect(tasks.starts, isEmpty);
      expect(tasks.cancels, isEmpty);
      expect(tasks.acknowledgements, isEmpty);
      await disposePage(tester);
    },
  );

  testWidgets(
    'consumed response survives settings close without implicit read or acknowledgement',
    (tester) async {
      final tasks = FakeTasks()
        ..state = snapshot(
          delivery: IoDeliveryPhase.ready,
          submission: identity(77),
        );
      tasks.nextResult = result(
        httpStatus: 200,
        body: Uint8List.fromList(utf8.encode('persisted-result-sentinel')),
      );
      final endpoints = FakeEndpoints();
      await mount(tester, tasks, endpoints);
      await click(tester, 'read');
      expect(find.textContaining('persisted-result-sentinel'), findsWidgets);
      final statusesBeforeRemount = tasks.statuses;
      await disposePage(tester);
      await mount(tester, tasks, endpoints);
      expect(tasks.statuses, greaterThan(statusesBeforeRemount));
      expect(find.textContaining('persisted-result-sentinel'), findsWidgets);
      expect(tasks.reads, hasLength(1));
      expect(tasks.starts, isEmpty);
      expect(tasks.acknowledgements, isEmpty);
      expect(tasks.cancels, isEmpty);
      await disposePage(tester);
    },
  );

  testWidgets(
    'unknown external outcome is explicitly retained instead of presented as rollback or HTTP completion',
    (tester) async {
      final tasks = FakeTasks()
        ..state = snapshot(
          delivery: IoDeliveryPhase.ready,
          submission: identity(77),
        );
      tasks.nextResult = result(unknown: true);
      await mount(tester, tasks, FakeEndpoints());
      await click(tester, 'read');
      final l = L10n.of(tester.element(find.byType(HttpTaskManager)));
      expect(find.text(l.pluginsHttpTaskOutcomeUnknown), findsOneWidget);
      expect(find.byKey(const ValueKey('http-task-result-body')), findsNothing);
      expect(find.textContaining(l.pluginsHttpTaskCompleted), findsNothing);
      expect(tasks.starts, isEmpty);
      expect(tasks.reads, hasLength(1));
      await disposePage(tester);
    },
  );

  testWidgets(
    'unknown start with a freshly empty host needs explicit local observation closure before another attempt',
    (tester) async {
      final tasks = FakeTasks(), endpoints = FakeEndpoints();
      tasks.start = (request) async {
        tasks.state = snapshot(
          storage: IoStoragePhase.local,
          delivery: IoDeliveryPhase.absent,
          submission: request.submission,
        );
        throw StateError('start acknowledgement unknown');
      };
      await mount(tester, tasks, endpoints);
      await click(tester, 'start');
      final first = tasks.starts.single.submission;
      expect(enabled(tester, 'start'), isFalse);
      await click(tester, 'status-refresh');
      expect(enabled(tester, 'start'), isFalse);
      expect(enabled(tester, 'abandon-attempt'), isTrue);
      await click(tester, 'abandon-attempt');
      expect(tasks.starts, hasLength(1));
      expect(tasks.reads, isEmpty);
      expect(tasks.cancels, isEmpty);
      expect(tasks.repairs, isEmpty);
      expect(tasks.acknowledgements, isEmpty);
      expect(enabled(tester, 'start'), isTrue);
      final l = L10n.of(tester.element(find.byType(HttpTaskManager)));
      expect(find.text(l.pluginsHttpTaskArchivedUnknown), findsWidgets);
      final archived = tester.widget<Text>(
        find.byKey(const ValueKey('http-task-history-0-submission')),
      );
      expect(
        archived.data,
        contains(first.map((v) => v.toRadixString(16).padLeft(2, '0')).join()),
      );
      await click(tester, 'start');
      expect(tasks.starts, hasLength(2));
      expect(tasks.starts.last.submission, isNot(orderedEquals(first)));
      await disposePage(tester);
    },
  );

  testWidgets(
    'same backend observing a new task never attributes the previous consumed result to it',
    (tester) async {
      final tasks = FakeTasks()
        ..state = snapshot(
          delivery: IoDeliveryPhase.ready,
          submission: identity(71),
        );
      tasks.nextResult = result(
        httpStatus: 200,
        body: Uint8List.fromList(utf8.encode('previous-task-result')),
      );
      final endpoints = FakeEndpoints();
      await mount(tester, tasks, endpoints);
      await click(tester, 'read');
      tasks.state = snapshot(
        key: identity(43),
        delivery: IoDeliveryPhase.ready,
        submission: identity(72),
      );
      await click(tester, 'status-refresh');
      expect(enabled(tester, 'read'), isTrue);
      expect(find.byKey(const ValueKey('http-task-result')), findsNothing);
      tasks.nextResult = result(
        httpStatus: 201,
        body: Uint8List.fromList(utf8.encode('new-task-result')),
      );
      await click(tester, 'read');
      expect(tasks.reads.last, orderedEquals(identity(43)));
      final current = tester.widget<Text>(
        find.byKey(const ValueKey('http-task-result-text')),
      );
      expect(current.data, 'new-task-result');
      await disposePage(tester);
    },
  );

  testWidgets(
    'late endpoint load and finally cannot authorize a replacement page still loading',
    (tester) async {
      final tasks = FakeTasks();
      final a = FakeEndpoints(), b = FakeEndpoints();
      final gateA = Completer<EndpointPage>(),
          gateB = Completer<EndpointPage>();
      a.page = () => gateA.future;
      b.page = () => gateB.future;
      await mount(tester, tasks, a);
      await mount(tester, tasks, b);
      gateA.complete(
        EndpointPage(entries: [endpoint()], snapshot: identity(5)),
      );
      await settle(tester);
      expect(enabled(tester, 'start'), isFalse);
      expect(enabled(tester, 'endpoints-refresh'), isFalse);
      gateB.complete(
        EndpointPage(entries: [endpoint()], snapshot: identity(6)),
      );
      await settle(tester);
      expect(enabled(tester, 'start'), isTrue);
      expect(tasks.starts, isEmpty);
      expect(a.writes + b.writes, 0);
      await disposePage(tester);
    },
  );

  testWidgets(
    'late read and its finally cannot overwrite or unlock another backend session',
    (tester) async {
      final a = FakeTasks()
        ..state = snapshot(
          delivery: IoDeliveryPhase.ready,
          submission: identity(71),
        );
      final b = FakeTasks()
        ..state = snapshot(
          key: identity(43),
          delivery: IoDeliveryPhase.ready,
          submission: identity(72),
        );
      final gateA = Completer<IoTaskRead>(), gateB = Completer<IoTaskRead>();
      a.read = (_) => gateA.future;
      b.read = (_) => gateB.future;
      final endpoints = FakeEndpoints();
      await mount(tester, a, endpoints);
      await click(tester, 'read');
      await mount(tester, b, endpoints);
      await click(tester, 'read');
      expect(a.reads, hasLength(1));
      expect(b.reads, hasLength(1));
      a.state = snapshot(
        storage: IoStoragePhase.reclaimed,
        delivery: IoDeliveryPhase.consumed,
        submission: identity(71),
        exit: cleanExit,
      );
      gateA.complete(
        IoTaskRead(
          snapshot: a.state,
          result: result(
            httpStatus: 200,
            body: Uint8List.fromList(utf8.encode('late-result-A')),
          ),
        ),
      );
      await settle(tester);
      expect(find.textContaining('late-result-A'), findsNothing);
      expect(enabled(tester, 'read'), isFalse);
      expect(enabled(tester, 'ack'), isFalse);
      b.state = snapshot(
        key: identity(43),
        storage: IoStoragePhase.reclaimed,
        delivery: IoDeliveryPhase.consumed,
        submission: identity(72),
        exit: cleanExit,
      );
      gateB.complete(
        IoTaskRead(
          snapshot: b.state,
          result: result(
            httpStatus: 200,
            body: Uint8List.fromList(utf8.encode('current-result-B')),
          ),
        ),
      );
      await settle(tester);
      expect(find.textContaining('current-result-B'), findsWidgets);
      expect(find.textContaining('late-result-A'), findsNothing);
      await mount(tester, a, endpoints);
      expect(find.textContaining('late-result-A'), findsWidgets);
      expect(find.textContaining('current-result-B'), findsNothing);
      expect(a.reads, hasLength(1));
      expect(b.reads, hasLength(1));
      await disposePage(tester);
    },
  );

  for (final action in ['poll', 'cancel']) {
    testWidgets(
      'late $action from A never changes B and A restoration retains only its own task',
      (tester) async {
        final a = FakeTasks()..state = snapshot(submission: identity(71));
        final b = FakeTasks()
          ..state = snapshot(
            key: identity(43),
            delivery: IoDeliveryPhase.ready,
            submission: identity(72),
          );
        final gate = Completer<IoTaskSnapshot>();
        if (action == 'poll') {
          a.poll = (_) => gate.future;
        } else {
          a.cancel = (_) => gate.future;
        }
        final endpoints = FakeEndpoints();
        await mount(tester, a, endpoints);
        await click(tester, action);
        await mount(tester, b, endpoints);
        expect(enabled(tester, 'read'), isTrue);
        expect(enabled(tester, 'ack'), isFalse);
        a.state = snapshot(
          storage: IoStoragePhase.reclaimed,
          delivery: IoDeliveryPhase.consumed,
          submission: identity(71),
          exit: cleanExit,
        );
        gate.complete(a.state);
        await settle(tester);
        expect(enabled(tester, 'read'), isTrue);
        expect(enabled(tester, 'ack'), isFalse);
        await click(tester, 'read');
        expect(b.reads.single, orderedEquals(identity(43)));
        expect(a.reads, isEmpty);
        await mount(tester, a, endpoints);
        expect(enabled(tester, 'ack'), isTrue);
        await click(tester, 'ack');
        expect(a.acknowledgements.single, orderedEquals(identity(42)));
        expect(b.acknowledgements, isEmpty);
        await disposePage(tester);
      },
    );
  }

  testWidgets(
    'pending start survives close and late acknowledgement belongs to its original session',
    (tester) async {
      final a = FakeTasks(),
          b = FakeTasks()
            ..state = snapshot(key: identity(43), submission: identity(72));
      final gate = Completer<IoTaskSnapshot>();
      a.start = (_) => gate.future;
      final endpoints = FakeEndpoints();
      await mount(tester, a, endpoints);
      await choose(tester, 'endpoint', hex(2));
      await click(tester, 'start');
      expect(a.starts, hasLength(1));
      await disposePage(tester);
      await mount(tester, b, endpoints);
      a.state = snapshot(submission: a.starts.single.submission);
      gate.complete(a.state);
      await settle(tester);
      await click(tester, 'cancel');
      expect(b.cancels.single, orderedEquals(identity(43)));
      expect(a.cancels, isEmpty);
      await mount(tester, a, endpoints);
      expect(enabled(tester, 'start'), isFalse);
      await click(tester, 'cancel');
      expect(a.cancels.single, orderedEquals(identity(42)));
      expect(a.starts, hasLength(1));
      expect(a.reads, isEmpty);
      expect(a.acknowledgements, isEmpty);
      await disposePage(tester);
    },
  );

  testWidgets(
    'remount after a detached start completion safely notifies a real rebuilding parent exactly once',
    (tester) async {
      final tasks = FakeTasks(), endpoints = FakeEndpoints();
      final gate = Completer<IoTaskSnapshot>();
      tasks.start = (_) => gate.future;
      late StateSetter setParent;
      var showing = true;
      var changes = 0;
      await tester.pumpWidget(
        StatefulBuilder(
          builder: (context, setState) {
            setParent = setState;
            return showing
                ? page(
                    tasks,
                    endpoints,
                    onChanged: () => setState(() => changes++),
                  )
                : const SizedBox.shrink();
          },
        ),
      );
      await settle(tester);
      await click(tester, 'start');
      expect(tasks.starts, hasLength(1));
      setParent(() => showing = false);
      await settle(tester);
      tasks.state = snapshot(submission: tasks.starts.single.submission);
      gate.complete(tasks.state);
      await settle(tester);
      expect(changes, 0);
      expect(tester.takeException(), isNull);
      setParent(() => showing = true);
      await settle(tester);
      expect(tester.takeException(), isNull);
      expect(changes, 1);
      expect(tasks.starts, hasLength(1));
      expect(tasks.reads, isEmpty);
      await click(tester, 'status-refresh');
      await tester.pump(const Duration(milliseconds: 1100));
      await settle(tester);
      expect(tester.takeException(), isNull);
      expect(changes, 1);
      await disposePage(tester);
    },
  );

  for (final locale in ['en', 'zh']) {
    testWidgets(
      '320px $locale remains usable for binary response and duplicate headers',
      (tester) async {
        tester.view.physicalSize = const Size(320, 1000);
        tester.view.devicePixelRatio = 1;
        addTearDown(tester.view.resetPhysicalSize);
        addTearDown(tester.view.resetDevicePixelRatio);
        final tasks = FakeTasks()
          ..state = snapshot(
            delivery: IoDeliveryPhase.ready,
            submission: identity(77),
          );
        await mount(tester, tasks, FakeEndpoints(), locale: locale);
        await click(tester, 'read');
        expect(find.textContaining('404'), findsWidgets);
        expect(find.textContaining('AAH/'), findsWidgets);
        expect(tester.takeException(), isNull);
        await disposePage(tester);
      },
    );
  }
}
