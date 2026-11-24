import 'dart:convert';
import 'dart:io';

import 'package:crypto/crypto.dart';
import 'package:flutter/foundation.dart' show debugPrintSynchronously;
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:morrow_studio/plugins/generated/host.capnp.dart' as wire;
import 'package:morrow_studio/plugins/plugin_content_task.dart';
import 'package:morrow_studio/plugins/plugin_library.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';

import 'external_plugin_native_test.dart'
    show entireCatalog, removeTestDirectory;

Finder key(String id) => find.byKey(ValueKey(id));

Future<void> waitFor(WidgetTester t, bool Function() ready, String step) async {
  final clock = Stopwatch()..start();
  while (!ready() && clock.elapsed < const Duration(seconds: 25)) {
    await t.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 20)),
    );
    await t.pump(const Duration(milliseconds: 100));
    final error = t.takeException();
    if (error != null) throw error;
  }
  expect(ready(), isTrue, reason: step);
}

Future<void> click(WidgetTester t, Finder target) async {
  FocusManager.instance.primaryFocus?.unfocus();
  t.testTextInput.hide();
  await t.pump(const Duration(milliseconds: 150));
  await t.ensureVisible(target);
  await t.pump(const Duration(milliseconds: 150));
  await t.tap(target);
  await t.pump(const Duration(milliseconds: 350));
}

void main() {
  final host = Platform.environment['MORROW_WORKBENCH_HOST'];
  final builtin = Platform.environment['MORROW_WORKBENCH_PACKAGE'];
  const hashes = {
    'rust': '5c178ed920a202b51d78442f7e399db7facdf88ecd064f298b30ca38809ee14d',
    'c': '43cba9cea5eac3b74021128d6be52f3acfc8c540440d6cc6ab7e0e5333f3a37b',
    'cpp': 'e9ad3f5ab5b561af39bc651bcb6739a98a730b1c1a42db0b6b8e4960964ee338',
  };
  for (final language in hashes.keys) {
    testWidgets(
      '$language content task requires card confirmation and preserves exact commits after reopen',
      (t) async {
        addTearDown(t.view.resetPhysicalSize);
        t.view.physicalSize = const Size(1320, 1100);
        t.view.devicePixelRatio = 1;
        addTearDown(t.view.resetDevicePixelRatio);
        final id = 'org.morrow.compat.$language.task';
        const card = 'reviewed-card';
        final path = File('sdk/compat/guest-v1-rc1/$language-task.mplugin')
            .absolute;
        Directory? root;
        RustWorkbench? product;
        late PluginLibraryEntry entry;
        late BigInt rev;
        var observedCommits = 0;
        Future<RustWorkbench> open() => RustWorkbench.open(
          executable: host!,
          package: builtin!,
          directory: root!,
          managed: true,
        );
        Future<void> refreshed() async {
          final page = await entireCatalog(product!);
          rev = page.revision;
          entry = page.entries.singleWhere((p) => p.id == id);
        }

        Future<PluginContentReceipt> lookup(
          String op, {
          String target = card,
        }) => product!.runPluginContent(
          entry,
          rev,
          PluginContentRequest(
            action: PluginContentAction.lookup,
            card: target,
            requestId: 'check-$op',
            operation: op,
          ),
        );
        try {
          await t.runAsync(() async {
            expect(
              sha256.convert(await path.readAsBytes()).toString(),
              hashes[language],
            );
            root = await Directory.systemTemp.createTemp(
              'morrow-external-content-$language-',
            );
            product = await open();
            final inspected = await product!.inspectPlugin(path.path);
            final candidate = inspected.entries.single;
            expect(candidate.contentTaskSupported, isTrue);
            await product!.importPlugin(
              path.path,
              candidate.digest,
              inspected.revision,
            );
            await refreshed();
            expect(entry.approved, isEmpty);
            await expectLater(lookup('no-approval'), throwsA(anything));
            await product!.configureExternal(
              entry,
              rev,
              PluginContentAction.values.map((a) => a.capability).toList(),
              true,
            );
            await refreshed();
          });
          expect(product, isNotNull);
          await t.pumpWidget(
            MaterialApp(
              locale: const Locale('en'),
              localizationsDelegates: const [
                AppLocalizations.delegate,
                ...GlobalMaterialLocalizations.delegates,
              ],
              supportedLocales: AppLocalizations.supportedLocales,
              home: Scaffold(
                body: SingleChildScrollView(
                  child: PluginLibrary(
                    backend: product!,
                    onChanged: () {
                      observedCommits++;
                    },
                    ink: Colors.black,
                    muted: Colors.grey,
                    line: Colors.grey,
                    radius: BorderRadius.circular(12),
                  ),
                ),
              ),
            ),
          );
          await waitFor(
            t,
            () => key('plugin-entry-$id').evaluate().isNotEmpty,
            'registered plugin',
          );
          await click(t, key('plugin-entry-$id'));
          await click(t, key('plugin-content-$id'));
          await t.enterText(key('content-card'), card);
          await t.enterText(key('content-title'), 'First card');
          await t.enterText(key('content-body'), 'First body');
          await click(t, key('content-review'));
          expect(find.textContaining('First body'), findsWidgets);
          await click(t, key('content-cancel'));
          expect(observedCommits, 0);
          await click(t, key('content-review'));
          await click(t, key('content-confirm'));
          String displayed() =>
              t.widget<SelectableText>(key('content-result')).data!;
          Future<void> result(String marker) => waitFor(
            t,
            () =>
                key('content-result').evaluate().isNotEmpty &&
                displayed().contains(marker),
            marker,
          );
          await result('Commit verified');
          String operationId() {
            final lines = displayed().split('\n');
            final label = lines.indexOf('Operation ID');
            expect(label, greaterThanOrEqualTo(0));
            final id = lines[label + 1];
            expect(id, matches(r'^query-[a-f0-9]{32}$'));
            return id;
          }

          final createId = operationId();
          expect(observedCommits, 1);
          Future<void> choose(String name) async {
            await click(t, key('content-action'));
            await click(t, find.text(name).last);
          }

          Future<void> approve() async {
            await click(t, key('content-review'));
            await click(t, key('content-confirm'));
          }

          await choose('Edit content');
          await t.enterText(key('content-title'), 'Updated card');
          await t.enterText(key('content-body'), '第二次编辑 😀');
          await t.enterText(key('content-preview'), 'Updated preview');
          await approve();
          await result('Revision  2');
          final editId = operationId();
          expect(editId, isNot(createId));
          expect(observedCommits, 2);
          await choose('Read content');
          await approve();
          await result('第二次编辑 😀');
          await choose('Query operation results');
          expect(
            t.widget<TextField>(key('content-operation')).controller!.text,
            editId,
          );
          await approve();
          await result(editId);
          await click(t, find.text('Close').last);
          await t.pumpWidget(const SizedBox.shrink());
          await t.runAsync(() async {
            await product!.close();
            expect(await product!.process.exitCode, 0);
            product = await open();
            await refreshed();
            final create = await lookup(createId), edit = await lookup(editId);
            expect(create.status, wire.PluginContentStatus.committed);
            expect(create.revision, BigInt.one);
            expect(edit.status, wire.PluginContentStatus.committed);
            expect(edit.revision, BigInt.two);
            expect(edit.event, isNot(create.event));
            final read = await product!.runPluginContent(
              entry,
              rev,
              PluginContentRequest(
                action: PluginContentAction.read,
                card: card,
                requestId: 'read-reopened',
                revision: BigInt.two,
              ),
            );
            expect(utf8.decode(read.bytes), '第二次编辑 😀');
            final wrong = await lookup(editId, target: 'another-card');
            expect(wrong.status, isNot(wire.PluginContentStatus.committed));
            final conflict = await product!.runPluginContent(
              entry,
              rev,
              PluginContentRequest(
                action: PluginContentAction.edit,
                card: card,
                requestId: 'stale-edit',
                revision: BigInt.one,
                title: 'Uncommitted',
                body: [1, 2],
              ),
            );
            expect(conflict.status, wire.PluginContentStatus.rejected);
            expect(
              (await lookup('stale-edit')).status,
              wire.PluginContentStatus.absent,
            );
            final stale = rev;
            await product!.configureExternal(entry, rev, [], true);
            await expectLater(
              product!.runPluginContent(
                entry,
                stale,
                PluginContentRequest(
                  action: PluginContentAction.read,
                  card: card,
                  requestId: 'stale-approval',
                  revision: BigInt.two,
                ),
              ),
              throwsA(anything),
            );
            await refreshed();
            await expectLater(lookup(editId), throwsA(anything));
            expect(entry.enabled, isTrue);
            expect(entry.approved, isEmpty);
            await product!.configureExternal(entry, rev, [], false);
            await product!.close();
            expect(await product!.process.exitCode, 0);
            product = await open();
            await refreshed();
            expect(entry.enabled, isFalse);
            expect(entry.approved, isEmpty);
            await expectLater(lookup(createId), throwsA(anything));
            await product!.close();
            expect(await product!.process.exitCode, 0);
            product = null;
            final record = jsonEncode({
              'language': language,
              'package_sha256': hashes[language],
              'create_operation': createId,
              'edit_operation': editId,
              'create_event': create.event,
              'edit_event': edit.event,
              'create_revision': 1,
              'edit_revision': 2,
              'commit_callbacks': observedCommits,
              'body_sha256': sha256.convert(read.bytes).toString(),
              'normal_exits': 3,
              'cancelled_before_dispatch': true,
              'revoked_after_reopen': true,
              'stale_edit_rejected': true,
            });
            debugPrintSynchronously('CARD_PRODUCT_PASS=$record');
          });
        } finally {
          await t.pumpWidget(const SizedBox.shrink());
          await t.runAsync(() async {
            if (product != null) await product!.close();
            if (root != null && root!.existsSync()) {
              await removeTestDirectory(root!);
            }
          });
        }
      },
      skip: !Platform.isWindows || host == null || builtin == null,
      timeout: const Timeout(Duration(minutes: 4)),
    );
  }
}
