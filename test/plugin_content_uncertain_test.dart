import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:morrow_studio/plugins/generated/host.capnp.dart' as wire;
import 'package:morrow_studio/plugins/plugin_content_dialog.dart';
import 'package:morrow_studio/plugins/plugin_content_task.dart';
import 'package:morrow_studio/plugins/plugin_library.dart';

class LostReply implements PluginContentBackend {
  final submitted = <PluginContentRequest>[];
  @override
  Future<PluginContentReceipt> runPluginContent(
    PluginLibraryEntry entry,
    BigInt revision,
    PluginContentRequest task,
  ) async {
    submitted.add(task);
    if (task.action == PluginContentAction.create) {
      throw StateError('response lost');
    }
    return PluginContentReceipt(
      wire.PluginContentStatus.absent,
      task.card,
      task.requestId,
      task.operation,
      BigInt.zero,
      '',
      Uint8List(0),
      Uint8List(0),
      BigInt.zero,
      BigInt.zero,
      '',
    );
  }
}

void main() {
  testWidgets(
    'a stale confirmation does not run; a lost write keeps its original card and operation',
    (tester) async {
      final backend = LostReply();
      final entry = PluginLibraryEntry(
        id: 'org.example.card',
        name: 'Card plugin',
        version: '1',
        digest: Uint8List(32),
        enabled: true,
        builtin: false,
        available: true,
        declared: ['create-content', 'operation'],
        approved: ['create-content', 'operation'],
        dependencies: [],
        handlers: [],
        issue: '',
        contentTaskSupported: true,
      );
      var current = true;
      await tester.pumpWidget(
        MaterialApp(
          locale: const Locale('en'),
          localizationsDelegates: const [
            AppLocalizations.delegate,
            ...GlobalMaterialLocalizations.delegates,
          ],
          supportedLocales: AppLocalizations.supportedLocales,
          home: PluginContentDialog(
            backend: backend,
            entry: entry,
            revision: BigInt.one,
            isCurrent: () => current,
            onCommitted: () => fail('unconfirmed commit'),
          ),
        ),
      );
      Finder k(String key) => find.byKey(ValueKey(key));
      Future<void> tap(Finder f) async {
        await tester.ensureVisible(f);
        await tester.tap(f);
        await tester.pumpAndSettle();
      }

      await tester.enterText(k('content-card'), 'card-a');
      await tap(k('content-review'));
      current = false;
      await tap(k('content-confirm'));
      expect(backend.submitted, isEmpty);
      current = true;
      await tap(k('content-review'));
      await tap(k('content-confirm'));
      expect(backend.submitted, hasLength(1));
      final original = backend.submitted.single;
      expect(
        tester.widget<SelectableText>(k('content-unknown')).data,
        contains(original.requestId),
      );
      expect(
        tester.widget<FilledButton>(k('content-review')).onPressed,
        isNull,
      );
      await tap(k('content-action'));
      await tap(find.text('Query operation results').last);
      expect(
        tester.widget<TextField>(k('content-operation')).controller!.text,
        original.requestId,
      );
      await tap(k('content-review'));
      await tap(k('content-confirm'));
      expect(backend.submitted.map((r) => r.action), [
        PluginContentAction.create,
        PluginContentAction.lookup,
      ]);
      expect(backend.submitted.last.card, original.card);
      expect(backend.submitted.last.operation, original.requestId);
      expect(
        tester.widget<SelectableText>(k('content-result')).data,
        contains('No commit is recorded'),
      );
      expect(
        tester.widget<SelectableText>(k('content-unknown')).data,
        contains(original.requestId),
      );
    },
  );
}
