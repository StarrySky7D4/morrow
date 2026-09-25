// Qualification entry only. Uses production Worker, identity and shared controller.
import 'dart:js_interop';
import 'dart:typed_data';
import 'package:file_selector/file_selector.dart';
import 'package:crypto/crypto.dart';
import 'package:flutter/material.dart';
import 'package:morrow_studio/main.dart' show Idea;
import 'package:morrow_studio/plugins/editor_draft.dart';
import 'package:morrow_studio/plugins/versioned_content.dart';
import 'package:morrow_studio/plugins/workbench_backend.dart';
import 'package:morrow_studio/plugins/workbench_channel_web.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';
import 'package:morrow_studio/plugins/studio_storage.dart';
import 'package:morrow_studio/attachments/attachment.dart';
import 'package:morrow_studio/media/texture_repository.dart';
import 'package:morrow_studio/media/texture_source.dart';
import 'package:morrow_studio/media/texture_storage_web.dart' as browser_media;

@JS('coreProbeResult')
external set result(JSString value);
void check(bool value, String message) {
  if (!value) throw StateError(message);
}

Future<void> rejects(Future<void> Function() action, String label) async {
  var failed = false;
  try {
    await action();
  } catch (_) {
    failed = true;
  }
  check(failed, '$label unexpectedly succeeded');
}

EditorDraftTextValue text(String value) => EditorDraftTextValue(
  text: value,
  selectionBase: value.length,
  selectionExtent: 0,
  affinity: 1,
  directional: true,
  composingStart: -1,
  composingEnd: -1,
);
Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  RustWorkbench? active;
  var stage = 'missing identity';
  try {
    await rejects(() async {
      final unexpected = await BrowserWorkbenchChannel.open(
        name: 'missing',
        create: false,
      );
      await unexpected.closeInput();
    }, 'open missing identity');
    stage = 'create local library';
    final channel = await BrowserWorkbenchChannel.open(
      name: 'channel-library',
      create: true,
    );
    final host = active = await RustWorkbench.connect(channel);
    check(
      host.writable,
      'actual guest unavailable: ${host.maintenanceWarning}',
    );
    stage = 'duplicate owner';
    await rejects(() async {
      final unexpected = await BrowserWorkbenchChannel.open(
        name: 'channel-library',
        create: false,
      );
      await unexpected.closeInput();
    }, 'duplicate owner');
    stage = 'content and stable tasks';
    final original = Uint8List(5 * 1024 * 1024 + 7);
    for (var i = 0; i < original.length; i++) {
      original[i] = (i * 31 + (i >> 8)) & 255;
    }
    final originalDigest = sha256.convert(original).toString();
    final selected = await IdeaAttachment.import(
      XFile.fromData(original, name: '本地原件.bin'),
    );
    await host.apply(
      PluginAction.create,
      Idea(
        '浏览器本地卡片',
        '正文😀',
        '进行中',
        Idea.icons[0],
        const Color(0xff8866aa),
        id: 'channel-card',
        stage: '计划中',
        todos: const ['same', 'same'],
        completed: const {'same'},
        attachments: [selected],
      ),
    );
    // Remove the picker-side copy before re-reading: only OPFS may supply the
    // committed attachment, including after an independent Worker opens it.
    await TextureRepository.remove(selected.source);
    final withAsset = (await host.loadWorkspaceContent()).single;
    final preview = withAsset.attachments.single;
    final bytes = await TextureRepository.resolve(preview.source);
    check(
      bytes.bytes?.length == original.length &&
          sha256.convert(bytes.bytes!).toString() == originalDigest,
      'committed attachment differs',
    );
    await rejects(() async {
      await channel.exportDeviceFile(
        'channel-card',
        'missing',
        'missing.bin',
        selected.source.kind,
      );
    }, 'missing attachment export');
    final control = host.versionedContent;
    final legacy = await control.read('channel-card');
    check(legacy.revision == BigInt.one, 'initial revision');
    final migrated = (await control.migrate(
      await control.planMigration(legacy.id),
    )).current;
    check(
      migrated.tasks.length == 2 &&
          migrated.tasks[0].id != migrated.tasks[1].id,
      'stable task IDs',
    );
    final edited = (await control.editTasks(
      'channel-task-edit',
      migrated.id,
      migrated.revision,
      TaskEditCommand.setCompletion(migrated.tasks[0].id, true),
    )).current;
    await rejects(() async {
      await control.editTasks(
        'channel-stale',
        migrated.id,
        migrated.revision,
        TaskEditCommand.setCompletion(migrated.tasks[1].id, false),
      );
    }, 'stale edit');
    stage = 'workspace query and preferences';
    final mediaSources = <TextureSource>[];
    for (final name in ['background.png', 'background.mp4', 'music.wav']) {
      mediaSources.add(
        await TextureRepository.importFile(
          XFile.fromData(Uint8List.fromList([11, 22, 33, 44]), name: name),
        ),
      );
    }
    final failures = <String>[];
    try {
      final ids = await host.query('概览', '全部', '', '最近添加');
      check(ids.contains(edited.id), 'created card missing from query');
    } catch (error, stack) {
      failures.add('query: $error\n$stack');
    }
    try {
      final storage = await RustStudioStorage.open(host);
      for (final background in mediaSources.take(2)) {
        await storage.write({
          ...storage.read(),
          'texture': background.toJson(),
          'music': {
            'tracks': [
              {
                'source': mediaSources[2].toJson(),
                'cover': mediaSources[0].toJson(),
              },
            ],
          },
        });
      }
      // A later unrelated edit must still commit while media remains selected.
      await rejects(
        () => storage.write({
          ...storage.read(),
          'texture': {
            ...mediaSources[0].toJson(),
            'location': 'blob:https://example.com/transient',
          },
        }),
        'transient media preference',
      );
      await storage.write({...storage.read(), 'theme': 'dark'});
      check(
        (await RustStudioStorage.open(host)).read()['theme'] == 'dark',
        'preferences not persisted',
      );
    } catch (error, stack) {
      failures.add('preferences: $error\n$stack');
    }
    if (failures.isNotEmpty) throw StateError(failures.join('\n'));
    stage = 'segmented draft';
    final body = List.filled(18000, '未完成😀\n').join();
    final draft = await host.editorDrafts.save(
      EditorDraftWriteRequest(
        cardId: edited.id,
        draftId: 'channel-draft',
        operation: 'channel-draft-save',
        expectedGeneration: BigInt.zero,
        sourceRevision: edited.revision,
        predecessorOperation: '',
        predecessorDigest: const [],
        assets: const [],
        values: EditorDraftValues(
          title: text('暂存标题'),
          description: text(body),
          hypothesis: text(''),
          conclusion: text(''),
          todos: text(''),
          category: '进行中',
          stage: '计划中',
        ),
      ),
    );
    stage = 'settings and approval';
    await host.saveUiLocale('en');
    final disabled = await host.configurePlugin(
      await host.pluginState(),
      false,
    );
    check(!disabled.enabled, 'plugin not disabled');
    stage = 'durable close';
    await host.close();
    active = null;
    await rejects(() async {
      await TextureRepository.resolve(preview.source);
    }, 'closed preview session');
    check(await channel.exitCode == 0, 'close did not confirm completion');
    stage = 'duplicate create';
    await rejects(() async {
      final unexpected = await BrowserWorkbenchChannel.open(
        name: 'channel-library',
        create: true,
      );
      await unexpected.closeInput();
    }, 'duplicate identity creation');
    stage = 'independent Worker reopen';
    final restored = active = await RustWorkbench.connect(
      await BrowserWorkbenchChannel.open(
        name: 'channel-library',
        create: false,
      ),
    );
    final record = await restored.versionedContent.read(edited.id);
    stage = 'reopened local media preferences';
    final mediaPreferences = (await RustStudioStorage.open(restored)).read();
    check(mediaPreferences['theme'] == 'dark', 'later appearance edit lost');
    final texture = TextureSource.fromJson(
      mediaPreferences['texture'] as Map<String, dynamic>,
    );
    final music = mediaPreferences['music'] as Map<String, dynamic>;
    final track = (music['tracks'] as List).single as Map<String, dynamic>;
    final recoveredSources = [
      texture,
      TextureSource.fromJson(track['source'] as Map<String, dynamic>),
      TextureSource.fromJson(track['cover'] as Map<String, dynamic>),
    ];
    check(
      texture.location == mediaSources[1].location,
      'video preference lost',
    );
    check(
      recoveredSources[1].location == mediaSources[2].location,
      'music preference lost',
    );
    check(
      recoveredSources[2].location == mediaSources[0].location,
      'cover preference lost',
    );
    for (final source in recoveredSources) {
      final blob = await browser_media.resolveBlob(source);
      final data = (await blob.arrayBuffer().toDart).toDart;
      check(
        sha256.convert(data.asUint8List()).toString() ==
            sha256.convert([11, 22, 33, 44]).toString(),
        'media original changed',
      );
    }
    stage = 'reopened attachment';
    final restoredIdea = (await restored.loadWorkspaceContent()).single;
    final restoredBytes = await TextureRepository.resolve(
      restoredIdea.attachments.single.source,
    );
    check(
      restoredBytes.bytes?.length == original.length &&
          sha256.convert(restoredBytes.bytes!).toString() == originalDigest,
      'OPFS attachment lost or changed on reopen',
    );
    check(
      record.revision == edited.revision && record.title == '浏览器本地卡片',
      'record changed on reopen',
    );
    check(
      record.tasks[0].id == migrated.tasks[0].id &&
          record.tasks[0].completion == VersionedTaskCompletion.complete,
      'task state lost',
    );
    final reopenedDraft = await restored.editorDrafts.read(
      edited.id,
      'channel-draft',
    );
    check(
      reopenedDraft != null &&
          reopenedDraft.generation == draft.generation &&
          reopenedDraft.request.values.description.text == body &&
          reopenedDraft.request.values.description.selectionBase == body.length,
      'draft/UTF-16 selection lost',
    );
    check(await restored.readUiLocale() == 'en', 'locale lost');
    check(!(await restored.pluginState()).enabled, 'approval lost');
    await restored.configurePlugin(await restored.pluginState(), true);
    await restored.close();
    active = null;
    result =
        'PASS: production Worker + device identity + Flutter shared controller; 5 MiB streamed local attachment, image/video/music/cover preferences and later edits persist across independent Worker reopen; stable tasks, drafts and approval'
            .toJS;
  } catch (error, stack) {
    result = 'FAIL: $stage: $error\n$stack'.toJS;
  } finally {
    try {
      await active?.close();
    } catch (_) {}
  }
}
