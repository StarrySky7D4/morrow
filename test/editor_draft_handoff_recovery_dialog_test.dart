import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:morrow_studio/editor_draft_handoff_recovery_dialog.dart';
import 'package:morrow_studio/plugins/editor_draft.dart';
import 'package:morrow_studio/plugins/editor_draft_handoff_coordinator.dart';
import 'editor_draft_handoff_coordinator_test.dart'
    show HandoffControl, proposalRecord;

Future<void> home(
  WidgetTester t,
  EditorDraftHandoffCoordinator c, {
  bool Function()? current,
  bool Function()? writable,
  String locale = 'en',
  double scale = 1,
}) async {
  await t.pumpWidget(
    MaterialApp(
      locale: Locale(locale),
      supportedLocales: AppLocalizations.supportedLocales,
      localizationsDelegates: const [
        AppLocalizations.delegate,
        GlobalMaterialLocalizations.delegate,
        GlobalWidgetsLocalizations.delegate,
        GlobalCupertinoLocalizations.delegate,
      ],
      builder: (context, child) => MediaQuery(
        data: MediaQuery.of(
          context,
        ).copyWith(textScaler: TextScaler.linear(scale)),
        child: child!,
      ),
      home: Scaffold(
        body: Builder(
          builder: (context) => TextButton(
            key: const ValueKey('open-handoffs'),
            onPressed: () => showDialog<void>(
              context: context,
              builder: (_) => EditorDraftHandoffRecoveryDialog(
                coordinator: c,
                isCurrent: current ?? () => true,
                canWrite: writable ?? () => true,
              ),
            ),
            child: const Text('open'),
          ),
        ),
      ),
    ),
  );
  await tap(t, 'open-handoffs');
}

Future<void> tap(WidgetTester t, String key) async {
  final target = find.byKey(ValueKey(key));
  if (target.evaluate().isEmpty) {
    await t.scrollUntilVisible(
      target,
      160,
      scrollable: find
          .descendant(
            of: find.byType(CustomScrollView).first,
            matching: find.byType(Scrollable),
          )
          .first,
    );
  }
  await Scrollable.ensureVisible(t.element(target), alignment: .3);
  await t.pumpAndSettle();
  expect(target.hitTestable(), findsOneWidget);
  await t.tap(target);
  await t.pumpAndSettle();
}

void main() {
  testWidgets(
    'read-only discovery, inspection and reopen preserve workspace owner',
    (t) async {
      final control = HandoffControl();
      final c = EditorDraftHandoffCoordinator(
        control,
        isCurrent: () => true,
        canWrite: () => false,
      );
      await home(t, c, writable: () => false);
      expect(control.writes, 0);
      await tap(t, 'draft-handoff-inspect-child-op');
      expect(find.text('frozen'), findsOneWidget);
      expect(
        t
            .widget<FilledButton>(
              find.byKey(const ValueKey('draft-handoff-complete')),
            )
            .onPressed,
        isNull,
      );
      await tap(t, 'draft-handoff-close');
      await tap(t, 'open-handoffs');
      expect(c.selected, isNotNull);
      expect(control.writes, 0);
      await t.pumpWidget(const SizedBox());
      c.dispose();
    },
  );

  testWidgets(
    'each stage requires its own confirmation; cancelled prompt does not write',
    (t) async {
      final control = HandoffControl();
      var current = proposalRecord();
      control.onInspect = () async => current;
      control.onComplete = () async => current = proposalRecord(
        status: EditorDraftHandoffProposalStatus.childCommitted,
      );
      control.onRetire = () async => current = proposalRecord(
        status: EditorDraftHandoffProposalStatus.parentRetired,
      );
      final c = EditorDraftHandoffCoordinator(
        control,
        isCurrent: () => true,
        canWrite: () => true,
      );
      await home(t, c);
      await tap(t, 'draft-handoff-inspect-child-op');
      await tap(t, 'draft-handoff-complete');
      expect(control.writes, 0);
      await t.tap(find.text('Cancel').last);
      await t.pumpAndSettle();
      expect(control.writes, 0);
      await tap(t, 'draft-handoff-complete');
      await tap(t, 'draft-handoff-confirm');
      expect(control.writes, 1);
      expect(
        c.selected!.summary.status,
        EditorDraftHandoffProposalStatus.childCommitted,
      );
      expect(
        find.byKey(const ValueKey('draft-handoff-retire')),
        findsOneWidget,
      );
      await tap(t, 'draft-handoff-retire');
      expect(control.writes, 1);
      await tap(t, 'draft-handoff-confirm');
      expect(control.writes, 2);
      expect(
        c.selected!.summary.status,
        EditorDraftHandoffProposalStatus.parentRetired,
      );
      await t.pumpWidget(const SizedBox());
      c.dispose();
    },
  );

  testWidgets(
    'unknown survives dismiss and refresh; only exact inspection unlocks actions',
    (t) async {
      final control = HandoffControl();
      control.onComplete = () async =>
          throw const EditorDraftHandoffProposalFailure(
            cardId: 'card',
            parentDraftId: 'parent',
            childOperation: 'child-op',
            outcomeUnknown: true,
            cause: 'private path must not be shown',
          );
      final c = EditorDraftHandoffCoordinator(
        control,
        isCurrent: () => true,
        canWrite: () => true,
      );
      await home(t, c);
      await tap(t, 'draft-handoff-inspect-child-op');
      await tap(t, 'draft-handoff-complete');
      await tap(t, 'draft-handoff-confirm');
      expect(c.uncertain, isTrue);
      expect(find.textContaining('private path'), findsNothing);
      await tap(t, 'draft-handoff-close');
      await tap(t, 'open-handoffs');
      expect(c.uncertain, isTrue);
      expect(
        t
            .widget<FilledButton>(
              find.byKey(const ValueKey('draft-handoff-complete')),
            )
            .onPressed,
        isNull,
      );
      expect(control.writes, 1);
      await tap(t, 'draft-handoff-inspect-child-op');
      expect(c.uncertain, isFalse);
      expect(control.writes, 1);
      await t.pumpWidget(const SizedBox());
      c.dispose();
    },
  );

  testWidgets(
    'closing during a sent action keeps the workspace request alive',
    (t) async {
      final control = HandoffControl();
      final reply = Completer<EditorDraftHandoffProposalRecord>();
      control.onComplete = () => reply.future;
      final c = EditorDraftHandoffCoordinator(
        control,
        isCurrent: () => true,
        canWrite: () => true,
      );
      await home(t, c);
      await tap(t, 'draft-handoff-inspect-child-op');
      await tap(t, 'draft-handoff-complete');
      await t.tap(find.byKey(const ValueKey('draft-handoff-confirm')));
      await t.pump(const Duration(milliseconds: 400));
      expect(c.busy, isTrue);
      expect(control.writes, 1);
      await t.tap(find.byKey(const ValueKey('draft-handoff-close')));
      await t.pumpAndSettle();
      await t.tap(find.byKey(const ValueKey('open-handoffs')));
      await t.pump(const Duration(milliseconds: 400));
      expect(c.busy, isTrue);
      expect(control.writes, 1);
      reply.complete(
        proposalRecord(status: EditorDraftHandoffProposalStatus.childCommitted),
      );
      await t.pumpAndSettle();
      expect(
        c.lastConfirmed!.summary.status,
        EditorDraftHandoffProposalStatus.childCommitted,
      );
      expect(control.writes, 1);
      await t.pumpWidget(const SizedBox());
      c.dispose();
    },
  );

  testWidgets(
    'switching workspace while confirmation is open prevents dispatch',
    (t) async {
      final control = HandoffControl();
      var current = true;
      final c = EditorDraftHandoffCoordinator(
        control,
        isCurrent: () => current,
        canWrite: () => true,
      );
      await home(t, c, current: () => current);
      await tap(t, 'draft-handoff-inspect-child-op');
      await tap(t, 'draft-handoff-cancel');
      current = false;
      await tap(t, 'draft-handoff-confirm');
      expect(control.writes, 0);
      await t.pumpWidget(const SizedBox());
      c.dispose();
    },
  );

  testWidgets(
    'narrow German large text remains scrollable and does not overflow',
    (t) async {
      t.view.devicePixelRatio = 1;
      t.view.physicalSize = const Size(390, 844);
      addTearDown(t.view.reset);
      final control = HandoffControl();
      final c = EditorDraftHandoffCoordinator(
        control,
        isCurrent: () => true,
        canWrite: () => true,
      );
      await home(t, c, locale: 'de', scale: 1.6);
      await tap(t, 'draft-handoff-inspect-child-op');
      await tap(t, 'draft-handoff-complete');
      expect(t.takeException(), isNull);
      expect(control.writes, 0);
      await t.pumpWidget(const SizedBox());
      c.dispose();
    },
  );
}
