import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/agent_wrapper_control.dart';
import 'package:morrow_studio/plugins/agent_wrapper_models.dart';
import 'package:morrow_studio/plugins/agent_wrapper_library.dart';
import 'agent_wrapper_test_helpers.dart';

final class Backend implements AgentWrapperControl {
  final calls = <String>[];
  AgentWrapperApproval? approved;
  AgentWrapperReview? inspected;
  bool enabled = false, unknown = false, failApproval = false;
  BigInt catalog = BigInt.one;
  List<AgentWrapperReview> packages = [review()];
  @override
  bool get supportsAgentWrappers => true;
  @override
  bool get wrapperOutcomeUnknown => unknown;
  AgentWrapperRevisions get current => revisions(catalog: catalog);
  AgentWrapperResult result() => AgentWrapperResult(revisions: current);
  @override
  Future<AgentWrapperResult> wrapperState() async {
    calls.add('state');
    return result();
  }

  @override
  Future<AgentWrapperResult> wrapperPage(
    AgentWrapperRevisions revisions, {
    String cursor = '',
  }) async {
    calls.add('page');
    return AgentWrapperResult(
      revisions: current,
      page: AgentWrapperPage(
        revisions: current,
        entries: [
          for (final p in packages)
            entry(package: p, approved: approved, enabled: enabled),
        ],
      ),
    );
  }

  @override
  Future<AgentWrapperResult> inspectWrapper(
    String path,
    AgentWrapperRevisions revisions,
  ) async {
    calls.add('inspect');
    return AgentWrapperResult(
      revisions: current,
      review: inspected ?? review(),
    );
  }

  @override
  Future<AgentWrapperResult> installWrapper(
    String path,
    AgentWrapperReview review,
    AgentWrapperRevisions revisions,
  ) async {
    calls.add('install');
    catalog += BigInt.one;
    return AgentWrapperResult(revisions: current, review: review);
  }

  @override
  Future<AgentWrapperResult> selectWrapperBase(
    AgentWrapperReview review,
    AgentWrapperRevisions revisions,
  ) async {
    calls.add('baseSelect');
    catalog += BigInt.one;
    return result();
  }

  @override
  Future<AgentWrapperResult> enableWrapperBase(
    AgentWrapperReview review,
    AgentWrapperRevisions revisions,
    bool enabled,
  ) async {
    calls.add('baseEnable');
    catalog += BigInt.one;
    return result();
  }

  @override
  Future<AgentWrapperResult> selectWrapper(
    AgentWrapperReview review,
    AgentWrapperRevisions revisions,
  ) async {
    calls.add('select');
    catalog += BigInt.one;
    return result();
  }

  @override
  Future<AgentWrapperResult> approveWrapper(
    AgentWrapperReview review,
    AgentWrapperRevisions revisions,
    AgentWrapperApproval approval,
  ) async {
    calls.add('approve');
    if (failApproval) {
      unknown = true;
      throw const AgentWrapperFailure(AgentWrapperFailureKind.unknown);
    }
    approved = approval;
    enabled = false;
    catalog += BigInt.one;
    return result();
  }

  @override
  Future<AgentWrapperResult> enableWrapper(
    AgentWrapperReview review,
    AgentWrapperRevisions revisions,
    bool enabled,
  ) async {
    calls.add('enable');
    this.enabled = enabled;
    catalog += BigInt.one;
    return result();
  }

  @override
  Future<AgentWrapperResult> removeWrapper(
    AgentWrapperReview review,
    AgentWrapperRevisions revisions,
  ) async {
    calls.add('remove');
    catalog += BigInt.one;
    return result();
  }
}

Widget panel(Backend backend, {Future<String?> Function()? pick}) =>
    MaterialApp(
      home: Scaffold(
        body: SingleChildScrollView(
          child: Padding(
            padding: const EdgeInsets.all(16),
            child: AgentWrapperLibrary(
              backend: backend,
              onChanged: () {},
              ink: Colors.black,
              muted: Colors.grey,
              line: Colors.grey,
              radius: BorderRadius.circular(11),
              pickPackage: pick,
            ),
          ),
        ),
      ),
    );
Future<void> tap(WidgetTester tester, String key) async {
  final finder = find.byKey(ValueKey(key));
  await tester.ensureVisible(finder);
  await tester.tap(finder);
  await tester.pumpAndSettle();
}

Future<void> showPackage(WidgetTester tester, Backend backend) async {
  await tester.pumpWidget(panel(backend));
  await tester.pumpAndSettle();
  await tap(tester, 'agent-page');
  await tap(tester, 'agent-wrapper-${review().key}');
}

void main() {
  testWidgets(
    'file review shows all declared capabilities read only without activation',
    (tester) async {
      final original = review();
      final backend = Backend()
        ..inspected = AgentWrapperReview(
          id: original.id,
          version: original.version,
          wrapperSha256: original.wrapperSha256,
          baseSha256: original.baseSha256,
          sessionSchema: original.sessionSchema,
          processSchema: original.processSchema,
          declaration: AgentWrapperApproval(
            sessionBits: 31,
            processBits: 127,
            sessions: original.declaration.sessions,
            executionDomain: original.declaration.executionDomain,
          ),
        );
      await tester.pumpWidget(
        panel(backend, pick: () async => 'C:/synthetic/full.magent'),
      );
      await tester.pumpAndSettle();
      await tap(tester, 'agent-pick');
      final preview = find.byKey(const ValueKey('agent-preview'));
      for (final label in [
        'Read sessions (required)',
        'Write sessions',
        'Propose execution',
        'Use approved execution',
        'Retire execution',
        'Read output',
        'Read process events',
        'Write input',
        'Close input',
        'Interrupt',
        'Terminate',
        'Resize terminal',
      ]) {
        expect(
          find.descendant(of: preview, matching: find.textContaining(label)),
          findsOneWidget,
        );
      }
      expect(
        find.descendant(
          of: preview,
          matching: find.textContaining('capabilities (read only)'),
        ),
        findsNWidgets(2),
      );
      expect(find.byType(CheckboxListTile), findsNothing);
      expect(backend.calls, ['state', 'inspect']);
      expect(backend.approved, isNull);
      expect(backend.enabled, false);
    },
  );
  testWidgets(
    'approve sends one request and separate activation requires explicit fresh read',
    (tester) async {
      final backend = Backend();
      await showPackage(tester, backend);
      final enable = find.byKey(ValueKey('agent-enable-${review().key}'));
      expect(tester.widget<OutlinedButton>(enable).onPressed, isNull);
      await tap(tester, 'agent-approve-${review().key}');
      expect(backend.calls, ['state', 'page', 'approve']);
      expect(backend.enabled, false);
      expect(tester.widget<OutlinedButton>(enable).onPressed, isNull);
      await tap(tester, 'agent-page');
      await tap(tester, 'agent-enable-${review().key}');
      expect(backend.calls, ['state', 'page', 'approve', 'page', 'enable']);
      expect(backend.enabled, true);
    },
  );
  testWidgets(
    'Unknown does not retry or unlock after reading the same owner facts',
    (tester) async {
      final backend = Backend()..failApproval = true;
      await showPackage(tester, backend);
      await tap(tester, 'agent-approve-${review().key}');
      expect(backend.calls.where((c) => c == 'approve').length, 1);
      await tap(tester, 'agent-state');
      await tap(tester, 'agent-page');
      expect(
        tester
            .widget<OutlinedButton>(
              find.byKey(ValueKey('agent-approve-${review().key}')),
            )
            .onPressed,
        isNull,
      );
      expect(backend.calls.where((c) => c == 'approve').length, 1);
      expect(
        find.textContaining('Reading facts does not unlock'),
        findsOneWidget,
      );
    },
  );
  testWidgets(
    'inspection and installation neither select nor approve nor activate',
    (tester) async {
      final backend = Backend();
      await tester.pumpWidget(
        panel(backend, pick: () async => 'C:/synthetic/full.magent'),
      );
      await tester.pumpAndSettle();
      await tap(tester, 'agent-pick');
      expect(backend.calls, ['state', 'inspect']);
      await tap(tester, 'agent-install');
      expect(backend.calls, ['state', 'inspect', 'install']);
      expect(backend.approved, isNull);
      expect(backend.enabled, false);
    },
  );
  testWidgets(
    'same base and id retain distinct wrapper keys on a narrow screen',
    (tester) async {
      tester.view.physicalSize = const Size(360, 800);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      final backend = Backend()..packages = [review(), review(seed: 2)];
      await tester.pumpWidget(panel(backend));
      await tester.pumpAndSettle();
      await tap(tester, 'agent-page');
      expect(
        find.byKey(ValueKey('agent-wrapper-${review().key}')),
        findsOneWidget,
      );
      expect(
        find.byKey(ValueKey('agent-wrapper-${review(seed: 2).key}')),
        findsOneWidget,
      );
      await tap(tester, 'agent-wrapper-${review().key}');
      expect(tester.takeException(), isNull);
    },
  );
}
