import 'dart:async';
import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:morrow_studio/editor_recovery_dialog.dart';
import 'package:morrow_studio/plugins/editor_recovery.dart';
import 'package:morrow_studio/plugins/versioned_content.dart';

EditorRecovery entry({
  EditorRecoveryStatus status = EditorRecoveryStatus.pending,
  List<int> digest = const [1, 2, 3],
}) => EditorRecovery(
  id: 'card',
  title: 'Recovered card',
  operation: 'captured-edit',
  digest: digest,
  sourceRevision: BigInt.from(2),
  currentRevision: BigInt.from(status == EditorRecoveryStatus.pending ? 2 : 3),
  status: status,
);

VersionedMutationResult result() => VersionedMutationResult(
  receipt: VersionedCommitReceipt(
    id: 'card',
    operation: 'captured-edit',
    revision: BigInt.from(3),
    repeated: false,
  ),
  current: VersionedContentRecord(
    id: 'card',
    title: 'Recovered card',
    revision: BigInt.from(3),
    formatVersion: 2,
    description: 'captured',
    category: '',
    stage: '',
    hypothesis: '',
    conclusion: '',
    favorite: false,
    assets: const [],
    icon: 0,
    color: 0,
    deleted: false,
    deletedAt: BigInt.zero,
    todos: const [],
    completed: const [],
    tasks: const [],
    origin: null,
    retiredTaskIds: const [],
    projectedStage: '',
    completeCount: 0,
    incompleteCount: 0,
    ambiguousCount: 0,
  ),
);

class RecoveryControl implements WorkbenchEditorRecovery {
  RecoveryControl(this.current);
  EditorRecovery? current;
  final events = <String>[];
  bool failPresentation = false;
  int ackFailures = 0;
  bool changeDigestOnReinspect = false;
  Completer<void>? reinspectionGate;

  Future<void> present(String id) async {
    events.add('present:$id');
    if (failPresentation) throw StateError('presentation failed');
  }

  @override
  Future<List<EditorRecovery>> inspectEditorRecoveries({String? id}) async {
    events.add(id == null ? 'inspect' : 'inspect:$id');
    if (id != null) await reinspectionGate?.future;
    final value = current;
    if (value == null || (id != null && id != value.id)) return [];
    if (id != null && changeDigestOnReinspect) {
      current = entry(digest: const [9, 9, 9]);
      return [current!];
    }
    return [value];
  }

  @override
  Future<VersionedMutationResult> resumeEditorRecovery(
    EditorRecovery observed,
  ) async {
    events.add('resume');
    expect(observed.operation, current?.operation);
    current = entry(status: EditorRecoveryStatus.committed);
    return result();
  }

  @override
  Future<void> acknowledgeEditorRecovery(EditorRecovery observed) async {
    events.add('ack');
    expect(observed.operation, current?.operation);
    if (ackFailures > 0) {
      ackFailures--;
      throw StateError('ack reply lost');
    }
    current = null;
  }

  @override
  Future<void> abandonEditorRecovery(EditorRecovery observed) async {
    events.add('abandon');
    expect(observed.operation, current?.operation);
    expect(observed.digest, current?.digest);
    current = null;
  }
}

Future<void> openDialog(
  WidgetTester tester,
  RecoveryControl control, {
  bool writable = true,
  bool Function()? isCurrent,
  String locale = 'en',
}) async {
  await tester.pumpWidget(
    MaterialApp(
      locale: Locale(locale),
      supportedLocales: AppLocalizations.supportedLocales,
      localizationsDelegates: const [
        AppLocalizations.delegate,
        GlobalMaterialLocalizations.delegate,
        GlobalWidgetsLocalizations.delegate,
        GlobalCupertinoLocalizations.delegate,
      ],
      home: Scaffold(
        body: Builder(
          builder: (context) => TextButton(
            onPressed: () => showDialog<void>(
              context: context,
              builder: (_) => EditorRecoveryDialog(
                control: control,
                writable: writable,
                presentCurrent: control.present,
                isCurrent: isCurrent ?? () => true,
              ),
            ),
            child: const Text('open recovery'),
          ),
        ),
      ),
    ),
  );
  await tester.tap(find.text('open recovery'));
  await tester.pumpAndSettle();
}

Finder get resolveButton =>
    find.byKey(const ValueKey('editor-recovery-resolve-card'));

void main() {
  testWidgets('inspection and dismissal leave a pending proposal untouched', (
    tester,
  ) async {
    final control = RecoveryControl(entry());
    await openDialog(tester, control);
    expect(control.events, ['inspect']);
    expect(control.current?.status, EditorRecoveryStatus.pending);
    await tester.tap(find.text('Cancel').last);
    await tester.pumpAndSettle();
    expect(control.events, ['inspect']);
    expect(control.current, isNotNull);
    expect(tester.takeException(), isNull);
  });

  testWidgets('committed recovery presents and acknowledges without resume', (
    tester,
  ) async {
    final control = RecoveryControl(
      entry(status: EditorRecoveryStatus.committed),
    );
    await openDialog(tester, control);
    await tester.tap(resolveButton);
    await tester.pumpAndSettle();
    expect(control.events, [
      'inspect',
      'inspect:card',
      'present:card',
      'ack',
      'inspect',
    ]);
    expect(control.current, isNull);
    expect(tester.takeException(), isNull);
  });

  testWidgets(
    'pending recovery resumes only on explicit choice, then presents before ack',
    (tester) async {
      final control = RecoveryControl(entry());
      await openDialog(tester, control);
      expect(control.events, ['inspect']);
      await tester.tap(resolveButton);
      await tester.pumpAndSettle();
      expect(control.events, [
        'inspect',
        'inspect:card',
        'resume',
        'present:card',
        'ack',
        'inspect',
      ]);
      expect(control.current, isNull);
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets('presentation failure retains committed proposal for review', (
    tester,
  ) async {
    final control = RecoveryControl(entry())..failPresentation = true;
    await openDialog(tester, control);
    await tester.tap(resolveButton);
    await tester.pumpAndSettle();
    expect(control.events, [
      'inspect',
      'inspect:card',
      'resume',
      'present:card',
      'inspect',
    ]);
    expect(control.current?.status, EditorRecoveryStatus.committed);
    expect(resolveButton, findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets('changed digest prevents resume and acknowledgement', (
    tester,
  ) async {
    final control = RecoveryControl(entry())..changeDigestOnReinspect = true;
    await openDialog(tester, control);
    await tester.tap(resolveButton);
    await tester.pumpAndSettle();
    expect(control.events, ['inspect', 'inspect:card', 'inspect']);
    expect(control.current?.digest, [9, 9, 9]);
    expect(tester.takeException(), isNull);
  });

  for (final (status, writable) in [
    (EditorRecoveryStatus.conflict, true),
    (EditorRecoveryStatus.pending, false),
  ]) {
    testWidgets('$status with writable=$writable cannot mutate', (
      tester,
    ) async {
      final control = RecoveryControl(entry(status: status));
      await openDialog(tester, control, writable: writable);
      expect(tester.widget<OutlinedButton>(resolveButton).onPressed, isNull);
      expect(control.events, ['inspect']);
      expect(control.current, isNotNull);
      expect(tester.takeException(), isNull);
    });
  }

  testWidgets(
    'workspace change during reinspection blocks all recovery mutation',
    (tester) async {
      var isCurrent = true;
      final control = RecoveryControl(entry())
        ..reinspectionGate = Completer<void>();
      await openDialog(tester, control, isCurrent: () => isCurrent);
      await tester.tap(resolveButton);
      await tester.pump();
      expect(control.events, contains('inspect:card'));
      isCurrent = false;
      control.reinspectionGate!.complete();
      await tester.pumpAndSettle();
      expect(control.events.where((event) => event == 'resume'), isEmpty);
      expect(control.events.where((event) => event == 'present:card'), isEmpty);
      expect(control.events.where((event) => event == 'ack'), isEmpty);
      expect(control.current?.status, EditorRecoveryStatus.pending);
      expect(tester.takeException(), isNull);
    },
  );
  testWidgets('lost ack reply retries without resending committed business', (
    tester,
  ) async {
    final control = RecoveryControl(entry())..ackFailures = 1;
    await openDialog(tester, control);
    await tester.tap(resolveButton);
    await tester.pumpAndSettle();
    expect(control.current?.status, EditorRecoveryStatus.committed);
    expect(control.events.where((event) => event == 'resume'), hasLength(1));
    expect(control.events.where((event) => event == 'ack'), hasLength(1));

    await tester.tap(resolveButton);
    await tester.pumpAndSettle();
    expect(control.events.where((event) => event == 'resume'), hasLength(1));
    expect(control.events.where((event) => event == 'ack'), hasLength(2));
    expect(control.current, isNull);
    expect(tester.takeException(), isNull);
  });

  testWidgets('cancelled discard preserves the pending proposal', (
    tester,
  ) async {
    final control = RecoveryControl(entry());
    await openDialog(tester, control);
    await tester.tap(
      find.byKey(const ValueKey('editor-recovery-abandon-card')),
    );
    await tester.pump();
    expect(
      find.byKey(const ValueKey('editor-recovery-abandon-confirm')),
      findsOneWidget,
    );
    await tester.tap(find.text('Cancel').last);
    await tester.pumpAndSettle();
    expect(control.events.where((event) => event == 'abandon'), isEmpty);
    expect(control.current?.status, EditorRecoveryStatus.pending);
    expect(tester.takeException(), isNull);
  });

  testWidgets(
    'confirmed discard clears pending without resume or presentation',
    (tester) async {
      final control = RecoveryControl(entry());
      await openDialog(tester, control);
      await tester.tap(
        find.byKey(const ValueKey('editor-recovery-abandon-card')),
      );
      await tester.pump();
      await tester.tap(
        find.byKey(const ValueKey('editor-recovery-abandon-confirm')),
      );
      await tester.pumpAndSettle();
      expect(control.events.where((event) => event == 'abandon'), hasLength(1));
      expect(control.events.where((event) => event == 'resume'), isEmpty);
      expect(control.events.where((event) => event == 'present:card'), isEmpty);
      expect(control.events.where((event) => event == 'ack'), isEmpty);
      expect(control.current, isNull);
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets('committed recovery has no discard action', (tester) async {
    final control = RecoveryControl(
      entry(status: EditorRecoveryStatus.committed),
    );
    await openDialog(tester, control);
    expect(
      find.byKey(const ValueKey('editor-recovery-abandon-card')),
      findsNothing,
    );
    expect(control.current, isNotNull);
    expect(tester.takeException(), isNull);
  });

  testWidgets('discard confirmation refuses a newer proposal', (tester) async {
    final control = RecoveryControl(entry());
    await openDialog(tester, control);
    await tester.tap(
      find.byKey(const ValueKey('editor-recovery-abandon-card')),
    );
    await tester.pump();
    control.current = entry(digest: const [9, 9, 9]);
    await tester.tap(
      find.byKey(const ValueKey('editor-recovery-abandon-confirm')),
    );
    await tester.pumpAndSettle();
    expect(control.events.where((event) => event == 'abandon'), isEmpty);
    expect(control.current?.digest, [9, 9, 9]);
    expect(tester.takeException(), isNull);
  });

  testWidgets('recovery and discard confirmation fit nine locales at 360px', (
    tester,
  ) async {
    tester.view.devicePixelRatio = 1;
    tester.view.physicalSize = const Size(360, 640);
    addTearDown(tester.view.reset);
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
      await tester.pumpWidget(const SizedBox());
      final control = RecoveryControl(entry());
      await openDialog(tester, control, locale: locale);
      expect(find.byType(EditorRecoveryDialog), findsOneWidget);
      expect(
        tester.takeException(),
        isNull,
        reason: 'recovery dialog: $locale',
      );
      final abandon = find.byKey(
        const ValueKey('editor-recovery-abandon-card'),
      );
      await tester.ensureVisible(abandon);
      await tester.pump();
      await tester.tap(abandon);
      await tester.pump();
      expect(find.byType(AlertDialog), findsNWidgets(2));
      expect(
        tester.takeException(),
        isNull,
        reason: 'discard confirmation: $locale',
      );
      await tester.tap(
        find.byKey(const ValueKey('editor-recovery-abandon-confirm')),
      );
      await tester.pumpAndSettle();
      expect(control.events.where((event) => event == 'abandon'), hasLength(1));
      expect(tester.takeException(), isNull, reason: 'after discard: $locale');
    }
  });
}
