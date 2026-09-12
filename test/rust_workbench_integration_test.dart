import 'dart:io';
import 'package:morrow_studio/content/rich_content.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/attachments/attachment.dart';
import 'package:morrow_studio/media/texture_source.dart';
import 'package:morrow_studio/plugins/workbench_backend.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';
import 'package:morrow_studio/plugins/studio_storage.dart';
import 'package:morrow_studio/music/lyrics_service.dart';

void main() {
  final executable = Platform.environment['MORROW_WORKBENCH_HOST'];
  final package = Platform.environment['MORROW_WORKBENCH_PACKAGE'];
  test(
    'real Rust guest, Flutter client, persistent records and preferences',
    () async {
      final directory = await Directory.systemTemp.createTemp(
        'morrow-plugin-parity-',
      );
      RustWorkbench? backend;
      try {
        backend = await RustWorkbench.open(
          executable: executable!,
          package: package!,
          directory: directory,
        );
        expect(backend.writable, isTrue);
        expect(await backend.load(), isEmpty);
        final file = File('${directory.path}/original.txt');
        await file.writeAsBytes([0, 1, 127, 255]);
        final idea = Idea(
          '插件记录',
          '**保留 Markdown**',
          '灵感',
          Idea.icons[0],
          const Color(0xff8866aa),
          id: 'integration-card',
          todos: ['阅读', '记录'],
          attachments: [
            IdeaAttachment(
              source: TextureSource(
                location: file.path,
                name: 'original.txt',
                kind: TextureKind.file,
                local: true,
              ),
              size: 4,
            ),
          ],
        );
        var saved = await backend.apply(PluginAction.create, idea);
        saved = await backend.apply(PluginAction.favorite, saved, flag: true);
        expect(saved.favorite, isTrue);
        saved = await backend.apply(PluginAction.toProject, saved);
        expect(saved.category, '进行中');
        saved = await backend.apply(PluginAction.stage, saved, text: '已完成');
        expect(saved.completed, {'阅读', '记录'});
        expect(await backend.query('小项目', '已完成', 'original', '最近添加'), [
          'integration-card',
        ]);
        await backend.apply(PluginAction.delete, saved);
        expect(await backend.query('概览', '全部', '', '最近添加'), isEmpty);
        saved = await backend.apply(PluginAction.restore, saved);
        expect(saved.favorite, isTrue);
        saved.description +=
            '\n[原件](attachment:${Uri.encodeComponent(file.path)})';
        saved = await backend.apply(PluginAction.edit, saved);
        expect(
          saved.description,
          contains('attachment:${saved.attachments.single.pluginId}'),
        );
        expect(
          saved.description,
          isNot(contains(Uri.encodeComponent(file.path))),
        );
        final storage = await RustStudioStorage.open(backend);
        await storage.write({
          ...storage.read(),
          'theme': 'custom',
          'glass': 'clear',
          'background': 'transparent',
          'themeColor': 0xff3355bb,
          'liquidCanvas': true,
          'windowRadius': 32.0,
          'grayscale': 0.25,
          'completed': ['喝水'],
          'music': {
            'tracks': [],
            'index': 0,
            'showLyrics': true,
            'onlineLyrics': false,
          },
        });

        // This aggregate crossed the old 64 KiB message limit. Each record
        // remains within the guest's independent validation budget.
        final tracks = List.generate(
          24,
          (i) => <String, dynamic>{
            'source': {
              'location': '${directory.path}/track-$i.mp3',
              'name': 'track-$i.mp3',
              'kind': 'audio',
              'local': true,
            },
            'lyrics': '[00:01]歌词 $i\n' * 1400,
            'trackTitle': '歌曲 $i',
            'artist': '测试',
          },
        );
        final large = <String, dynamic>{
          ...storage.read(),
          'canvasBlur': 12.0,
          'canvasOpacity': 0.2,
          'componentCustom': true,
          'componentMaterials': <String, dynamic>{
            'card:integration-card': <String, dynamic>{
              'enabled': true,
              'blur': 2.0,
              'opacity': 0.0,
              'color': 0xff44aa99,
            },
            'navigation': <String, dynamic>{
              'enabled': false,
              'blur': 18.0,
              'opacity': .6,
              'color': 0xff8899aa,
            },
          },
          'componentBlur': 3.0,
          'music': {'tracks': tracks, 'index': 23},
        };
        final frozen = encodePreferences(large);
        expect(frozen.length, greaterThan(65536));
        final firstSave = backend.savePreferences(frozen);
        frozen.fillRange(0, frozen.length, 0); // caller may reuse its buffer
        await firstSave;
        final writes = [
          storage.write({...large, 'canvasOpacity': 0.3}),
          storage.write({...large, 'canvasOpacity': 0.4}),
        ];
        await Future.wait(writes);
        expect(storage.read()['canvasOpacity'], 0.4);
        final beforeFailure = await backend.readPreferences();
        await expectLater(
          storage.write({
            ...large,
            'music': {
              'tracks': [
                ...tracks.take(23),
                {...tracks.last, 'trackDuration': -1.0},
              ],
              'index': 23,
            },
          }),
          throwsStateError,
        );
        expect(await backend.readPreferences(), beforeFailure);
        expect(storage.read()['canvasOpacity'], 0.4);

        final lines = await backend.studio.parseLyrics(
          '[offset:100]\n[00:01.20][00:02:345]中文',
        );
        expect(lines.map((v) => v.time.inMilliseconds), [1100, 2245]);
        final match = await backend.studio.matchLyrics(
          [
            const LyricMatch(
              title: 'A',
              artist: 'One',
              lyrics: '[00:01]ok',
              duration: 100,
              synced: true,
            ),
          ],
          'a',
          'one',
          100,
        );
        expect(match, 0);
        await backend.studio.validateImport('video', 150 * 1024 * 1024);
        await expectLater(
          backend.studio.validateImport('image', 25 * 1024 * 1024 + 1),
          throwsStateError,
        );
        final playback = await backend.studio.playback(
          'previous',
          count: 3,
          index: 0,
          playing: false,
          blocked: false,
          flag: true,
        );
        expect(playback.index, 2);
        expect(playback.playing, isTrue);
        final html =
            '<h2>标题</h2><p>Hello <b>重点</b> world <em>观察</em></p><table><tr><td rowspan="2">跨行</td><td>A</td></tr><tr><td>B</td></tr></table><script>alert(1)</script><a href="javascript:alert(1)">危险链接</a>';
        final rich = await backend.studio.capture('html', html);
        expect(rich.markdown, htmlToMarkdown(html).markdown);
        const xml =
            '<Workbook xmlns:ss="urn:schemas-microsoft-com:office:spreadsheet"><Worksheet><Table><Row><Cell><Data>名称</Data></Cell><Cell><Data>结果</Data></Cell></Row><Row><Cell><Data>合计</Data></Cell><Cell ss:Formula="=SUM(R[-1]C:R[-1]C)"><Data>42</Data></Cell></Row></Table></Worksheet></Workbook>';
        expect(
          (await backend.studio.capture('spreadsheet', xml)).markdown,
          spreadsheetToMarkdown(xml).markdown,
        );
        const rtf =
            r'{\rtf1\ansi\uc1 {\fonttbl secret}\u20013?\u25991?\par next{\*\objdata payload}}';
        expect(
          (await backend.studio.capture('rtf', rtf)).markdown,
          rtfToPlainText(rtf),
        );
        expect(
          (await backend.studio.capture('plain', 'A\tB\n1\t2')).markdown,
          plainTextToMarkdown('A\tB\n1\t2'),
        );
        final many = List.generate(
          30,
          (i) => LyricMatch(
            title: i == 10 ? 'matching' : 'other-$i',
            artist: 'Artist',
            lyrics: '长歌词' * 15000,
            duration: 100,
            synced: true,
          ),
        );
        expect(
          await backend.studio.matchLyrics(many, 'matching', 'Artist', 100),
          10,
        );
        await backend.close();
        backend = null;
        backend = await RustWorkbench.open(
          executable: executable,
          package: package,
          directory: directory,
        );
        final restored = await RustStudioStorage.open(backend);
        expect(restored.read()['themeColor'], 0xff3355bb);
        expect(restored.read()['liquidCanvas'], isTrue);
        expect(restored.read()['completed'], ['喝水']);
        expect(restored.read()['canvasOpacity'], 0.4);
        expect(restored.read()['componentCustom'], isTrue);
        expect(
          restored.read()['componentMaterials'],
          large['componentMaterials'],
        );
        final restoredTracks =
            (restored.read()['music'] as Map)['tracks'] as List;
        expect(restoredTracks, hasLength(24));
        expect(restoredTracks.last['lyrics'], tracks.last['lyrics']);
        final cards = await backend.load();
        expect(cards, hasLength(1));
        expect(cards.single.favorite, isTrue);
        expect(cards.single.description, saved.description);
        expect(
          await File(
            cards.single.attachments.single.source.location,
          ).readAsBytes(),
          [0, 1, 127, 255],
        );
      } finally {
        await backend?.close();
        await directory.delete(recursive: true);
      }
    },
    skip: executable == null || package == null
        ? 'Run tool/build_rust_workbench_windows.ps1 with the compiled Rust guest and host'
        : false,
    timeout: const Timeout(Duration(minutes: 2)),
  );
  test(
    'protected workbench reports missing keys and restores with the original key',
    () async {
      final directory = await Directory.systemTemp.createTemp('morrow-key-ui-');
      RustWorkbench? backend;
      try {
        backend = await RustWorkbench.open(
          executable: executable!,
          package: package!,
          directory: directory,
        );
        final saved = await backend.apply(
          PluginAction.create,
          Idea(
            '保留原记录',
            '原内容',
            '灵感',
            Idea.icons[0],
            const Color(0xff8866aa),
            id: 'retained-card',
          ),
        );
        await backend.close();
        backend = null;
        final key = File('${directory.path}/workbench.db.audit-key');
        final original = await key.readAsBytes();
        final retained = await key.rename('${directory.path}/retained-key');
        await expectLater(
          RustWorkbench.open(
            executable: executable,
            package: package,
            directory: directory,
          ),
          throwsA(
            isA<StateError>().having(
              (e) => e.toString(),
              'message',
              contains('保护密钥缺失'),
            ),
          ),
        );
        expect(await key.exists(), isFalse);
        final invalid = File('${directory.path}/invalid-backup');
        await invalid.writeAsString('not a protected file');
        await expectLater(
          RustWorkbench.restoreKey(
            executable: executable,
            directory: directory,
            selected: invalid.path,
          ),
          throwsStateError,
        );
        expect(await key.exists(), isFalse);
        await RustWorkbench.restoreKey(
          executable: executable,
          directory: directory,
          selected: retained.path,
        );
        expect(await retained.readAsBytes(), original);
        backend = await RustWorkbench.open(
          executable: executable,
          package: package,
          directory: directory,
        );
        expect(backend.maintenanceWarning, isNull);
        final restored = await backend.load();
        expect(restored.single.id, saved.id);
        expect(restored.single.description, saved.description);
        expect(await key.readAsBytes(), original);
      } finally {
        await backend?.close();
        await directory.delete(recursive: true);
      }
    },
    skip: executable == null || package == null || !Platform.isWindows,
    timeout: const Timeout(Duration(minutes: 1)),
  );
}
