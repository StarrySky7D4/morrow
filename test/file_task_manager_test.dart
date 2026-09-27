import 'dart:async';
import 'dart:typed_data';

import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:morrow_studio/plugins/file_task_manager.dart';
import 'package:morrow_studio/plugins/file_task_models.dart';
import 'package:morrow_studio/plugins/file_task_session.dart';
import 'package:morrow_studio/plugins/io_task_models.dart';
import 'package:morrow_studio/plugins/plugin_library.dart';

class _Files implements FileTaskBackend {
  final starts = <FileTaskRequest>[];
  Future<IoTaskSnapshot> Function(FileTaskRequest)? onStart;

  @override
  Future<IoTaskSnapshot> startFile(FileTaskRequest request) async {
    starts.add(request);
    if (onStart != null) return onStart!(request);
    return _running(request);
  }

  @override
  dynamic noSuchMethod(Invocation invocation) =>
      throw UnimplementedError(invocation.memberName.toString());
}

class _Io implements WorkbenchIoTaskControl {
  int statuses = 0;
  int polls = 0;
  int cancels = 0;
  int repairs = 0;
  IoTaskSnapshot state = _local();
  Future<IoTaskSnapshot> Function()? onCancel;
  Future<IoTaskSnapshot> Function()? onRepair;

  @override
  Future<IoTaskSnapshot> ioStatus() async {
    statuses++;
    return state;
  }

  @override
  Future<IoTaskSnapshot> pollIo(Uint8List key) async {
    polls++;
    return state;
  }

  @override
  Future<IoTaskSnapshot> cancelIo(Uint8List key) async {
    cancels++;
    return onCancel == null ? state : onCancel!();
  }

  @override
  Future<IoTaskSnapshot> repairIo(Uint8List key) async {
    repairs++;
    return onRepair == null ? state : onRepair!();
  }

  @override
  dynamic noSuchMethod(Invocation invocation) =>
      throw UnimplementedError(invocation.memberName.toString());
}

IoTaskSnapshot _local() => IoTaskSnapshot(
  storage: IoStoragePhase.local,
  delivery: IoDeliveryPhase.absent,
  exit: null,
);

IoTaskSnapshot _running(FileTaskRequest request) => IoTaskSnapshot(
  key: Uint8List.fromList(List.filled(32, 7)),
  submission: request.submission,
  storage: IoStoragePhase.running,
  delivery: IoDeliveryPhase.pending,
  exit: null,
);

IoTaskSnapshot _exited(FileTaskRequest request, IoStoragePhase storage) =>
    IoTaskSnapshot(
      key: Uint8List.fromList(List.filled(32, 7)),
      submission: request.submission,
      storage: storage,
      delivery: IoDeliveryPhase.consumed,
      exit: const IoTaskExit(
        execution: IoJobError.none,
        disconnect: IoJobError.none,
        maintenance: IoJobError.none,
      ),
    );

FileTaskRequest _request() => FileTaskRequest(
  submission: Uint8List.fromList(List.filled(32, 3)),
  packageId: 'test.file-ui',
  packageDigest: Uint8List.fromList(List.filled(32, 1)),
  registryRevision: BigInt.one,
  handler: 'test.file-read.v1',
  selectedPath: r'C:\test\session.txt',
  maxBytes: BigInt.from(262144),
  timeoutMs: 10000,
);

Future<FileTaskSession> _startSession(_Files files, _Io io) async {
  final session = FileTaskSession.forBackend(files, io);
  await session.refresh();
  final request = _request();
  await session.start(request);
  io.state = _running(request);
  return session;
}

bool _enabled(WidgetTester tester, String id) =>
    tester
        .widget<OutlinedButton>(find.byKey(ValueKey('file-task-$id')))
        .onPressed !=
    null;

Future<void> _tap(WidgetTester tester, String id) async {
  final button = find.byKey(ValueKey('file-task-$id'));
  await tester.ensureVisible(button);
  await tester.tap(button);
  await tester.pump();
}

PluginLibraryEntry _plugin() => PluginLibraryEntry(
  id: 'test.file-ui',
  name: 'File test package',
  version: '1',
  digest: Uint8List.fromList(List.filled(32, 1)),
  enabled: true,
  builtin: false,
  available: true,
  declared: [],
  approved: [],
  dependencies: [],
  handlers: [],
  issue: '',
  declaredIo: const ['file-read'],
  approvedIo: const ['file-read'],
  ioHandlers: const ['test.file-read.v1'],
);

Widget _page(
  String locale,
  _Files files,
  _Io io, {
  Future<XFile?> Function()? pickFile,
  bool visible = true,
}) => MaterialApp(
  locale: Locale(locale),
  localizationsDelegates: AppLocalizations.localizationsDelegates,
  supportedLocales: AppLocalizations.supportedLocales,
  home: Scaffold(
    body: TickerMode(
      enabled: visible,
      child: SingleChildScrollView(
        child: FileTaskManager(
          backend: files,
          ioBackend: io,
          plugins: [_plugin()],
          registryRevision: BigInt.one,
          ink: Colors.black,
          muted: Colors.grey,
          line: Colors.grey,
          radius: BorderRadius.circular(8),
          pickFile: pickFile,
        ),
      ),
    ),
  ),
);

void main() {
  testWidgets('file task title and picker use each supported language', (
    tester,
  ) async {
    final files = _Files();
    final io = _Io();
    for (final locale in [
      'zh',
      'en',
      'ru',
      'fr',
      'de',
      'es',
      'ja',
      'ko',
      'pt',
    ]) {
      await tester.pumpWidget(
        _page(locale, files, io, pickFile: () async => null),
      );
      await tester.pumpAndSettle();
      final l = L10n.forLocale(Locale(locale));
      expect(find.text(l.pluginsFileTaskTitle), findsOneWidget);
      expect(find.text(l.pluginsFileTaskSelectFile), findsOneWidget);
      expect(find.text(l.pluginsFileTaskIdle), findsOneWidget);
    }
    expect(io.statuses, greaterThan(0));
    expect(files.starts, isEmpty);
  });

  testWidgets('cancelled picker keeps selection and does not submit', (
    tester,
  ) async {
    final files = _Files();
    final io = _Io();
    var picks = 0;
    Future<XFile?> pick() async =>
        ++picks == 1 ? XFile(r'C:\test\sample.txt', name: 'sample.txt') : null;

    await tester.pumpWidget(_page('en', files, io, pickFile: pick));
    await tester.pumpAndSettle();
    final pickButton = find.byKey(const ValueKey('file-task-pick'));
    await tester.ensureVisible(pickButton);
    await tester.tap(pickButton);
    await tester.pumpAndSettle();
    expect(
      find.text(
        L10n.forLocale(const Locale('en')).pluginsSelectedFile('sample.txt'),
      ),
      findsOneWidget,
    );

    await tester.ensureVisible(pickButton);
    await tester.tap(pickButton);
    await tester.pumpAndSettle();
    expect(picks, 2);
    expect(
      find.text(
        L10n.forLocale(const Locale('en')).pluginsSelectedFile('sample.txt'),
      ),
      findsOneWidget,
    );
    expect(files.starts, isEmpty);

    await tester.pumpWidget(_page('de', files, io, pickFile: pick));
    await tester.pumpAndSettle();
    expect(
      find.text(
        L10n.forLocale(const Locale('de')).pluginsSelectedFile('sample.txt'),
      ),
      findsOneWidget,
    );
    expect(files.starts, isEmpty);
  });

  testWidgets('leaving and returning to an active task never starts it again', (
    tester,
  ) async {
    final files = _Files();
    final io = _Io();
    await tester.pumpWidget(_page('en', files, io));
    await tester.pumpAndSettle();
    final session = await _startSession(files, io);
    await tester.pump();
    expect(files.starts, hasLength(1));
    expect(_enabled(tester, 'start'), isFalse);
    expect(_enabled(tester, 'verify'), isTrue);

    await tester.pumpWidget(const SizedBox.shrink());
    final pollsBeforeReturn = io.polls;
    await tester.pump(const Duration(seconds: 2));
    expect(io.polls, pollsBeforeReturn);
    await tester.pumpWidget(_page('en', files, io));
    await tester.pump();
    expect(identical(FileTaskSession.forBackend(files, io), session), isTrue);
    expect(files.starts, hasLength(1));
    expect(io.polls, greaterThan(pollsBeforeReturn));
    expect(_enabled(tester, 'start'), isFalse);
    expect(_enabled(tester, 'verify'), isTrue);
    expect(find.textContaining('session.txt'), findsOneWidget);
  });

  testWidgets('hidden task panel stops display polling and resumes on return', (
    tester,
  ) async {
    final files = _Files();
    final io = _Io();
    await tester.pumpWidget(_page('en', files, io));
    await tester.pumpAndSettle();
    await _startSession(files, io);
    await tester.pump();
    await tester.pump(const Duration(seconds: 1));
    expect(io.polls, greaterThan(0));

    await tester.pumpWidget(_page('en', files, io, visible: false));
    await tester.pump();
    final hiddenPolls = io.polls;
    await tester.pump(const Duration(seconds: 3));
    expect(io.polls, hiddenPolls);
    expect(files.starts, hasLength(1));

    await tester.pumpWidget(_page('en', files, io));
    await tester.pump();
    expect(io.polls, greaterThan(hiddenPolls));
    expect(files.starts, hasLength(1));
  });

  testWidgets('late picker reply from old backend cannot select a file', (
    tester,
  ) async {
    final oldFiles = _Files();
    final oldIo = _Io();
    final newFiles = _Files();
    final newIo = _Io();
    final oldPick = Completer<XFile?>();
    await tester.pumpWidget(
      _page('en', oldFiles, oldIo, pickFile: () => oldPick.future),
    );
    await tester.pumpAndSettle();
    await _tap(tester, 'pick');

    await tester.pumpWidget(
      _page('en', newFiles, newIo, pickFile: () async => null),
    );
    await tester.pump();
    oldPick.complete(XFile(r'C:\test\stale.txt', name: 'stale.txt'));
    await tester.pump();
    expect(find.textContaining('stale.txt'), findsNothing);
    expect(_enabled(tester, 'start'), isFalse);
    expect(_enabled(tester, 'pick'), isTrue);
    expect(oldFiles.starts, isEmpty);
    expect(newFiles.starts, isEmpty);
  });

  testWidgets(
    'unknown start cannot be replayed and only local status permits abandon',
    (tester) async {
      final files = _Files()
        ..onStart = (_) async => throw StateError('lost reply');
      final io = _Io();
      await tester.pumpWidget(
        _page(
          'en',
          files,
          io,
          pickFile: () async =>
              XFile(r'C:\test\sample.txt', name: 'sample.txt'),
        ),
      );
      await tester.pumpAndSettle();
      final dropdown = tester.widget<DropdownButton<String>>(
        find.byKey(const ValueKey('file-task-handler')),
      );
      dropdown.onChanged!(dropdown.items!.single.value);
      await tester.pump();
      await _tap(tester, 'pick');
      expect(_enabled(tester, 'start'), isTrue);
      await _tap(tester, 'start');
      expect(files.starts, hasLength(1));
      expect(_enabled(tester, 'start'), isFalse);
      expect(_enabled(tester, 'cancel'), isFalse);
      expect(_enabled(tester, 'repair'), isFalse);
      expect(_enabled(tester, 'abandon'), isFalse);

      await _tap(tester, 'refresh');
      expect(files.starts, hasLength(1));
      expect(_enabled(tester, 'abandon'), isTrue);
      expect(_enabled(tester, 'start'), isFalse);
      expect(_enabled(tester, 'cancel'), isFalse);
      await _tap(tester, 'abandon');
      expect(files.starts, hasLength(1));
      expect(_enabled(tester, 'start'), isTrue);
    },
  );

  testWidgets(
    'unknown cancellation is not repeated and follows observed exit',
    (tester) async {
      final files = _Files();
      final io = _Io();
      await tester.pumpWidget(_page('en', files, io));
      await tester.pumpAndSettle();
      await _startSession(files, io);
      await tester.pump();
      final pendingCancel = Completer<IoTaskSnapshot>();
      io.onCancel = () => pendingCancel.future;
      await _tap(tester, 'cancel');
      expect(io.cancels, 1);
      expect(_enabled(tester, 'cancel'), isFalse);
      expect(_enabled(tester, 'repair'), isFalse);
      pendingCancel.completeError(StateError('lost cancellation reply'));
      await tester.pump();
      expect(_enabled(tester, 'cancel'), isFalse);
      expect(_enabled(tester, 'repair'), isFalse);

      await _tap(tester, 'refresh');
      expect(io.cancels, 1);
      expect(_enabled(tester, 'cancel'), isTrue);
      expect(_enabled(tester, 'repair'), isFalse);

      io.state = _exited(_request(), IoStoragePhase.reclaimed);
      await _tap(tester, 'refresh');
      expect(io.cancels, 1);
      expect(_enabled(tester, 'cancel'), isFalse);
      expect(_enabled(tester, 'repair'), isFalse);
      expect(_enabled(tester, 'ack'), isTrue);
    },
  );

  testWidgets(
    'unknown repair waits for status and disables after reclamation',
    (tester) async {
      final files = _Files();
      final io = _Io();
      await tester.pumpWidget(_page('en', files, io));
      await tester.pumpAndSettle();
      await _startSession(files, io);
      io.state = _exited(_request(), IoStoragePhase.recoveryRequired);
      final session = FileTaskSession.forBackend(files, io);
      await session.refresh();
      await tester.pump();
      expect(_enabled(tester, 'repair'), isTrue);

      final pendingRepair = Completer<IoTaskSnapshot>();
      io.onRepair = () => pendingRepair.future;
      await _tap(tester, 'repair');
      expect(io.repairs, 1);
      expect(_enabled(tester, 'repair'), isFalse);
      pendingRepair.completeError(StateError('lost repair reply'));
      await tester.pump();
      expect(_enabled(tester, 'repair'), isFalse);

      await _tap(tester, 'refresh');
      expect(io.repairs, 1);
      expect(_enabled(tester, 'repair'), isTrue);

      io.state = _exited(_request(), IoStoragePhase.reclaimed);
      await _tap(tester, 'refresh');
      expect(io.repairs, 1);
      expect(_enabled(tester, 'repair'), isFalse);
      expect(_enabled(tester, 'ack'), isTrue);
      expect(_enabled(tester, 'cancel'), isFalse);
    },
  );
}
