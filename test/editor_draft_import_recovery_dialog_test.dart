import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:morrow_studio/editor_draft_import_recovery_dialog.dart';
import 'package:morrow_studio/plugins/editor_draft_import.dart';
import 'package:morrow_studio/plugins/editor_draft_import_decision_coordinator.dart';

EditorDraftImportDecision decision(
  EditorDraftImportDecisionStatus status, {
  String name = 'photo.png',
  String operation = 'abandon-photo',
}) {
  final request = EditorDraftImportRequest(
    cardId: 'card-one',
    draftId: 'draft-one',
    operation: 'import-$operation',
    expectedGeneration: BigInt.one,
    name: name,
    kind: 'image',
    bytes: BigInt.from(4),
    sha256: List<int>.filled(32, 7),
  );
  return EditorDraftImportDecision(
    request: request,
    operation: operation,
    expectedGeneration: BigInt.one,
    status: status,
    currentGeneration: BigInt.one,
    mainActive: true,
    stagingRevision: BigInt.two,
    decisionRevision: status == EditorDraftImportDecisionStatus.cancelled
        ? BigInt.two
        : BigInt.one,
    committedRevision: status == EditorDraftImportDecisionStatus.committed
        ? BigInt.from(3)
        : BigInt.zero,
  );
}

EditorDraftImportSnapshot receipt(EditorDraftImportDecision value) =>
    EditorDraftImportSnapshot(
      kind: EditorDraftImportResultKind.decision,
      cardId: value.request.cardId,
      draftId: value.request.draftId,
      operation: value.operation,
      expectedGeneration: value.expectedGeneration,
      importOperation: value.request.operation,
      currentGeneration: value.currentGeneration,
      mainActive: value.mainActive,
      stagingRevision: value.stagingRevision,
      exportBytes: BigInt.zero,
      decision: value,
    );

final class RecoveryControl implements EditorDraftImportControl {
  RecoveryControl(this.entries);
  List<EditorDraftImportDecision> entries;
  final events = <String>[];
  Completer<void>? discoveryGate;
  Completer<void>? inspectGate;
  bool failRefreshAfterAction = false;
  EditorDraftImportDecision? inspectOverride;
  bool failDiscovery = false;

  @override
  Future<List<EditorDraftImportDecision>> discoverDecisions() async {
    events.add('discover');
    await discoveryGate?.future;
    if (failDiscovery) throw StateError('private path and operation secret');
    return List.unmodifiable(entries);
  }

  @override
  Future<EditorDraftImportSnapshot> inspectDecision(
    EditorDraftImportAbandon intent,
  ) async {
    events.add('inspect:${intent.operation}');
    await inspectGate?.future;
    final match = entries.singleWhere(
      (entry) => entry.operation == intent.operation,
    );
    return receipt(inspectOverride ?? match);
  }

  @override
  Future<EditorDraftImportSnapshot> abandon(
    EditorDraftImportAbandon intent,
  ) async {
    events.add('abandon:${intent.operation}');
    final index = entries.indexWhere(
      (entry) => entry.operation == intent.operation,
    );
    entries[index] = decision(
      EditorDraftImportDecisionStatus.committed,
      name: entries[index].request.name,
      operation: intent.operation,
    );
    if (failRefreshAfterAction) failDiscovery = true;
    return receipt(entries[index]);
  }

  @override
  Future<EditorDraftImportSnapshot> cancelDecision(
    EditorDraftImportAbandon intent,
  ) async {
    events.add('cancel:${intent.operation}');
    final index = entries.indexWhere(
      (entry) => entry.operation == intent.operation,
    );
    entries[index] = decision(
      EditorDraftImportDecisionStatus.cancelled,
      name: entries[index].request.name,
      operation: intent.operation,
    );
    if (failRefreshAfterAction) failDiscovery = true;
    return receipt(entries[index]);
  }

  @override
  Future<EditorDraftImportSnapshot> begin(EditorDraftImportRequest request) =>
      throw UnimplementedError();
  @override
  Future<EditorDraftImportSnapshot> complete(
    EditorDraftImportRequest request, {
    String selectedPath = '',
  }) => throw UnimplementedError();
  @override
  Future<EditorDraftImportSnapshot> inspect(
    String cardId,
    String draftId,
    String importOperation,
  ) => throw UnimplementedError();
  @override
  Future<EditorDraftImportSnapshot> list(String cardId, String draftId) =>
      throw UnimplementedError();
  @override
  Future<EditorDraftImportSnapshot> export(
    String cardId,
    String draftId,
    BigInt currentGeneration,
    String importOperation,
    String selectedPath,
  ) => throw UnimplementedError();
  @override
  Future<EditorDraftImportSnapshot> prepareDecision(
    EditorDraftImportAbandon intent,
  ) => throw UnimplementedError();
  @override
  Future<EditorDraftImportScopePage> listDecisionScopes({
    String cursor = '',
    int limit = 32,
  }) => throw UnimplementedError();
  @override
  Future<EditorDraftImportDecisionPage> listDecisions(
    String cardId,
    String draftId, {
    String cursor = '',
    int limit = 32,
  }) => throw UnimplementedError();
  @override
  Future<EditorDraftImportSnapshot> reconcile(String cardId, String draftId) =>
      throw UnimplementedError();
}

Future<void> openDialog(
  WidgetTester tester,
  RecoveryControl control, {
  bool writable = true,
  bool Function()? isCurrent,
  String locale = 'en',
  bool settle = true,
}) async {
  final current = isCurrent ?? () => true;
  final coordinator = EditorDraftImportDecisionCoordinator(
    control,
    isCurrent: current,
  );
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
              builder: (_) => EditorDraftImportRecoveryDialog(
                coordinator: coordinator,
                writable: writable,
                isCurrent: current,
              ),
            ),
            child: const Text('open decisions'),
          ),
        ),
      ),
    ),
  );
  await tester.tap(find.text('open decisions'));
  if (settle) {
    await tester.pumpAndSettle();
  } else {
    await tester.pump();
  }
}

void main() {
  testWidgets(
    'discovery and close do not submit; all four states are visible',
    (tester) async {
      final control = RecoveryControl([
        decision(EditorDraftImportDecisionStatus.pending),
        decision(
          EditorDraftImportDecisionStatus.committed,
          name: 'done.png',
          operation: 'abandon-done',
        ),
        decision(
          EditorDraftImportDecisionStatus.cancelled,
          name: 'kept.png',
          operation: 'abandon-kept',
        ),
        decision(
          EditorDraftImportDecisionStatus.conflict,
          name: 'changed.png',
          operation: 'abandon-changed',
        ),
      ]);
      await openDialog(tester, control, writable: false);
      expect(find.text('photo.png'), findsOneWidget);
      expect(find.text('done.png'), findsOneWidget);
      expect(find.text('kept.png'), findsOneWidget);
      expect(find.text('changed.png'), findsOneWidget);
      expect(
        find.text('Completed: this attachment import was abandoned.'),
        findsOneWidget,
      );
      expect(
        find.text(
          'Cancelled: the original abandonment request will no longer run.',
        ),
        findsOneWidget,
      );
      expect(
        tester
            .widget<OutlinedButton>(
              find.byKey(
                const ValueKey('draft-import-recovery-retry-abandon-photo'),
              ),
            )
            .onPressed,
        isNull,
      );
      expect(
        tester
            .widget<TextButton>(
              find.byKey(
                const ValueKey('draft-import-recovery-cancel-abandon-changed'),
              ),
            )
            .onPressed,
        isNull,
      );
      await tester.tap(find.text('Close'));
      await tester.pumpAndSettle();
      expect(control.events, ['discover']);
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets('pending retries only observed operation after reinspection', (
    tester,
  ) async {
    final control = RecoveryControl([
      decision(EditorDraftImportDecisionStatus.pending),
    ]);
    await openDialog(tester, control);
    await tester.tap(
      find.byKey(const ValueKey('draft-import-recovery-retry-abandon-photo')),
    );
    await tester.pumpAndSettle();
    expect(
      control.events,
      containsAllInOrder([
        'discover',
        'inspect:abandon-photo',
        'abandon:abandon-photo',
        'discover',
      ]),
    );
    expect(control.events.where((e) => e == 'discover'), hasLength(2));
    expect(
      control.entries.single.status,
      EditorDraftImportDecisionStatus.committed,
    );
    expect(
      find.byKey(const ValueKey('draft-import-recovery-retry-abandon-photo')),
      findsNothing,
    );
    expect(tester.takeException(), isNull);
  });

  testWidgets(
    'conflict can only cancel after explicit non-deleting confirmation',
    (tester) async {
      final control = RecoveryControl([
        decision(EditorDraftImportDecisionStatus.conflict),
      ]);
      await openDialog(tester, control);
      expect(
        find.byKey(const ValueKey('draft-import-recovery-retry-abandon-photo')),
        findsNothing,
      );
      final cancel = find.byKey(
        const ValueKey('draft-import-recovery-cancel-abandon-photo'),
      );
      await tester.tap(cancel);
      await tester.pumpAndSettle();
      expect(
        find.textContaining('original abandonment request will no longer run'),
        findsOneWidget,
      );
      expect(
        find.textContaining('does not delete the attachment'),
        findsOneWidget,
      );
      await tester.tap(find.text('Cancel').last);
      await tester.pumpAndSettle();
      expect(control.events, ['discover']);
      await tester.tap(cancel);
      await tester.pumpAndSettle();
      await tester.tap(
        find.byKey(const ValueKey('draft-import-recovery-cancel-confirm')),
      );
      await tester.pumpAndSettle();
      expect(
        control.events,
        containsAllInOrder([
          'discover',
          'inspect:abandon-photo',
          'cancel:abandon-photo',
          'discover',
        ]),
      );
      expect(control.events.where((e) => e == 'discover'), hasLength(2));
      expect(
        control.entries.single.status,
        EditorDraftImportDecisionStatus.cancelled,
      );
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets('changed identity cannot act and raw error is not displayed', (
    tester,
  ) async {
    final control =
        RecoveryControl([decision(EditorDraftImportDecisionStatus.pending)])
          ..inspectOverride = decision(
            EditorDraftImportDecisionStatus.pending,
            operation: 'foreign-abandon',
          );
    await openDialog(tester, control);
    await tester.tap(
      find.byKey(const ValueKey('draft-import-recovery-retry-abandon-photo')),
    );
    await tester.pumpAndSettle();
    expect(control.events, contains('inspect:abandon-photo'));
    expect(control.events.where((e) => e.startsWith('abandon:')), isEmpty);
    expect(
      find.textContaining('Could not review this decision'),
      findsOneWidget,
    );
    expect(find.textContaining('foreign-abandon'), findsNothing);
    control.failDiscovery = true;
    await tester.tap(find.text('Retry').last);
    await tester.pumpAndSettle();
    expect(find.textContaining('private path'), findsNothing);
    expect(tester.takeException(), isNull);
  });

  testWidgets('confirmed action with failed refresh never repeats the write', (
    tester,
  ) async {
    final control = RecoveryControl([
      decision(EditorDraftImportDecisionStatus.pending),
    ])..failRefreshAfterAction = true;
    await openDialog(tester, control);
    await tester.tap(
      find.byKey(const ValueKey('draft-import-recovery-retry-abandon-photo')),
    );
    await tester.pumpAndSettle();
    expect(
      find.textContaining('Action confirmed, but the list has not refreshed'),
      findsOneWidget,
    );
    expect(
      control.events.where((e) => e == 'abandon:abandon-photo'),
      hasLength(1),
    );
    final staleRetry = find.byKey(
      const ValueKey('draft-import-recovery-retry-abandon-photo'),
    );
    expect(tester.widget<OutlinedButton>(staleRetry).onPressed, isNull);
    final refresh = find.byKey(const ValueKey('draft-import-recovery-refresh'));
    expect(tester.widget<TextButton>(refresh).onPressed, isNotNull);
    await tester.tap(refresh);
    await tester.pumpAndSettle();
    expect(tester.widget<OutlinedButton>(staleRetry).onPressed, isNull);
    expect(
      control.events.where((e) => e == 'abandon:abandon-photo'),
      hasLength(1),
    );
    control.failDiscovery = false;
    expect(tester.widget<TextButton>(refresh).onPressed, isNotNull);
    await tester.tap(refresh);
    await tester.pumpAndSettle();
    expect(
      control.entries.single.status,
      EditorDraftImportDecisionStatus.committed,
    );
    expect(
      control.events.where((e) => e == 'abandon:abandon-photo'),
      hasLength(1),
    );
    expect(
      find.textContaining('Action confirmed, but the list has not refreshed'),
      findsNothing,
    );
    expect(tester.takeException(), isNull);
  });

  testWidgets('workspace replacement before reinspection blocks the write', (
    tester,
  ) async {
    final gate = Completer<void>();
    final control = RecoveryControl([
      decision(EditorDraftImportDecisionStatus.pending),
    ])..inspectGate = gate;
    var current = true;
    await openDialog(tester, control, isCurrent: () => current);
    await tester.tap(
      find.byKey(const ValueKey('draft-import-recovery-retry-abandon-photo')),
    );
    await tester.pump();
    current = false;
    gate.complete();
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 50));
    expect(control.events, contains('inspect:abandon-photo'));
    expect(control.events.where((e) => e.startsWith('abandon:')), isEmpty);
    expect(tester.takeException(), isNull);
  });

  testWidgets('late discovery from a replaced workspace is ignored', (
    tester,
  ) async {
    final gate = Completer<void>();
    final control = RecoveryControl([
      decision(EditorDraftImportDecisionStatus.pending),
    ])..discoveryGate = gate;
    var current = true;
    await openDialog(tester, control, isCurrent: () => current, settle: false);
    expect(find.text('photo.png'), findsNothing);
    current = false;
    gate.complete();
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 50));
    expect(find.text('photo.png'), findsNothing);
    expect(control.events, ['discover']);
    expect(tester.takeException(), isNull);
  });

  testWidgets('narrow viewport scrolls long decision list without overflow', (
    tester,
  ) async {
    tester.view.physicalSize = const Size(320, 560);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    for (final locale in [
      'en',
      'zh',
      'ru',
      'fr',
      'de',
      'es',
      'ja',
      'ko',
      'pt',
    ]) {
      final control = RecoveryControl([
        decision(EditorDraftImportDecisionStatus.pending),
        for (var i = 0; i < 12; i++)
          decision(
            EditorDraftImportDecisionStatus.cancelled,
            name: 'attachment-$i.png',
            operation: 'abandon-$i',
          ),
      ]);
      await openDialog(tester, control, locale: locale);
      expect(find.byType(SingleChildScrollView), findsWidgets);
      expect(tester.takeException(), isNull, reason: 'recovery list $locale');
      final cancel = find.byKey(
        const ValueKey('draft-import-recovery-cancel-abandon-photo'),
      );
      await tester.ensureVisible(cancel);
      await tester.tap(cancel);
      await tester.pumpAndSettle();
      expect(
        find.byKey(const ValueKey('draft-import-recovery-cancel-confirm')),
        findsOneWidget,
      );
      expect(
        tester.takeException(),
        isNull,
        reason: 'cancel confirmation $locale',
      );
      expect(control.events.where((e) => e.startsWith('cancel:')), isEmpty);
      await tester.pumpWidget(const SizedBox());
      await tester.pumpAndSettle();
    }
  });
}
