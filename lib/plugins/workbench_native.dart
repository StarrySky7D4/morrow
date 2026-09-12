import 'studio_native.dart';
import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';
import 'package:crypto/crypto.dart';
import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:flutter/material.dart' show Color;
import '../main.dart' show Idea;
import '../attachments/attachment.dart';
import '../media/texture_source.dart';
import 'workbench_backend.dart';
import 'generated/host.capnp.dart' as host;
import 'generated/workbench.capnp.dart' as wire;
import 'generated/identity.dart' as contract;

class RustWorkbench implements WorkbenchBackend, WorkbenchProtectionBackup {
  RustWorkbench._(this.process, this.cache) {
    process.stdout.listen(_receive, onError: _fail, onDone: _ended);
    _stderrDone = process.stderr.listen((bytes) {
      final remaining = 4096 - _stderr.length;
      if (remaining > 0) _stderr.addAll(bytes.take(remaining));
    }).asFuture<void>();
  }
  final _stderr = <int>[];
  late final Future<void> _stderrDone;
  String? maintenanceWarning;
  Future<void> _ended() async {
    try {
      await _stderrDone;
    } catch (_) {}
    final code = await process.exitCode;
    final detail = utf8.decode(_stderr, allowMalformed: true).trim();
    _fail(StateError(detail.isEmpty ? '内容服务已退出 ($code)' : detail));
  }

  final Process process;
  final Directory cache;
  final _revisions = <String, int>{};
  final _assets = <String, IdeaAttachment>{};
  final _buffer = <int>[];
  Completer<Uint8List>? _response;
  Object? _failure;
  Future<void> _queue = Future.value();
  int _sequence = 0;
  @override
  late final RustStudioPlugin studio = RustStudioPlugin(this);
  @override
  bool writable = false;
  static Future<RustWorkbench> open({
    required String executable,
    required String package,
    required Directory directory,
    bool managed = false,
  }) async {
    await directory.create(recursive: true);
    final cache = Directory('${directory.path}/preview-cache');
    await cache.create(recursive: true);
    final process = await Process.start(executable, [
      if (managed) '--managed',
      managed ? directory.path : '${directory.path}/workbench.db',
      package,
    ]);
    final result = RustWorkbench._(process, cache);
    try {
      await result._call(host.Action.page, configure: (r) => r.limit = 1);
      return result;
    } catch (_) {
      await result.close();
      rethrow;
    }
  }

  @override
  Future<void> backupProtection(String destination) async {
    await _call(
      host.Action.backupProtection,
      configure: (r) => r.selectedPath = destination,
    );
  }

  @override
  Future<void> backupSnapshot(String destination) async {
    await _call(
      host.Action.backupSnapshot,
      requestTimeout: const Duration(minutes: 30),
      configure: (r) => r.selectedPath = destination,
    );
  }

  static Future<void> restoreSnapshot({
    required String executable,
    required String archive,
    required Directory destination,
  }) async {
    final result = await Process.run(executable, [
      '--restore-snapshot',
      archive,
      destination.path,
    ]);
    if (result.exitCode != 0) throw StateError(result.stderr.toString().trim());
  }

  static Future<void> activateLibrary({
    required String executable,
    required Directory root,
    required Directory selected,
  }) async {
    final result = await Process.run(executable, [
      '--activate-library',
      root.path,
      selected.path,
    ]);
    if (result.exitCode != 0) throw StateError(result.stderr.toString().trim());
  }

  /// The host verifies and restores protected bytes. Dart never loads secret material.
  static Future<void> restoreKey({
    required String executable,
    required Directory directory,
    required String selected,
    bool managed = false,
  }) async {
    final result = await Process.run(executable, [
      managed ? '--restore-active-key' : '--restore-key',
      managed ? directory.path : '${directory.path}/workbench.db',
      selected,
    ]);
    if (result.exitCode != 0) {
      final detail = result.stderr.toString().trim();
      throw StateError(detail.isEmpty ? '恢复未完成，请重新打开内容库核对。' : detail);
    }
  }

  void _fail(Object error) {
    _failure ??= error;
    final pending = _response;
    _response = null;
    if (pending != null && !pending.isCompleted) pending.completeError(error);
  }

  void _receive(List<int> bytes) {
    if (_failure != null) return;
    _buffer.addAll(bytes);
    if (_buffer.length < 4) return;
    final size = ByteData.sublistView(
      Uint8List.fromList(_buffer.take(4).toList()),
    ).getUint32(0, Endian.little);
    if (size == 0 || size > 128 * 1024 || _buffer.length > 128 * 1024 + 4) {
      _fail(const FormatException('内容服务消息过大'));
      return;
    }
    if (_buffer.length < size + 4) return;
    if (_buffer.length != size + 4 || _response == null) {
      _fail(const FormatException('内容服务响应顺序错误'));
      return;
    }
    final reply = Uint8List.fromList(_buffer.sublist(4));
    _buffer.clear();
    final pending = _response!;
    _response = null;
    pending.complete(reply);
  }

  static MessageReader readMessage(
    Uint8List bytes, {
    int maxBytes = 128 * 1024,
  }) {
    if (bytes.length < 8 || bytes.length > maxBytes) {
      throw const FormatException('消息长度');
    }
    final view = ByteData.sublistView(bytes);
    final count = view.getUint32(0, Endian.little) + 1;
    if (count > 512) throw const FormatException('分段数量');
    var total = ((count + 2) ~/ 2) * 8;
    if (total > bytes.length) throw const FormatException('分段头');
    for (var i = 0; i < count; i++) {
      total += view.getUint32(4 + i * 4, Endian.little) * 8;
      if (total > bytes.length) throw const FormatException('分段长度');
    }
    if (total != bytes.length) throw const FormatException('多余消息');
    return MessageReader.deserialize(
      bytes,
      MessageReaderOptions(
        traversalLimitInWords: maxBytes ~/ 8,
        nestingLimit: 20,
        maxSegments: 512,
      ),
    );
  }

  static bool _same(List<int>? a, List<int> b) =>
      a != null &&
      a.length == b.length &&
      List.generate(b.length, (i) => a[i] == b[i]).every((v) => v);
  Future<host.ResponseReader> _call(
    host.Action action, {
    void Function(host.RequestBuilder)? configure,
    Duration requestTimeout = const Duration(seconds: 60),
  }) {
    final completion = Completer<host.ResponseReader>();
    _queue = _queue.then((_) async {
      try {
        if (_failure != null) throw _failure!;
        final builder = MessageBuilder();
        final r = builder.initRoot(host.requestFactory);
        r.version = 1;
        r.digest = Uint8List.fromList(contract.hostDigest);
        r.action = action;
        configure?.call(r);
        final payload = builder.serialize();
        if (payload.length > 128 * 1024) throw const FormatException('请求内容过大');
        final header = ByteData(4)..setUint32(0, payload.length, Endian.little);
        final response = _response = Completer<Uint8List>();
        process.stdin.add(header.buffer.asUint8List());
        process.stdin.add(payload);
        await process.stdin.flush();
        final bytes = await response.future.timeout(
          requestTimeout,
          onTimeout: () {
            final error = TimeoutException('内容服务响应超时');
            _fail(error);
            process.kill();
            throw error;
          },
        );
        final reply = readMessage(bytes).getRoot(host.responseFactory);
        if (reply.version != 1 || !_same(reply.digest, contract.hostDigest)) {
          throw const FormatException('内容服务版本不匹配');
        }
        writable = !reply.readOnly;
        final notice = reply.maintenanceWarning ?? '';
        maintenanceWarning = notice.isEmpty ? null : notice;
        if ((reply.error ?? '').isNotEmpty) throw StateError(reply.error!);
        completion.complete(reply);
      } catch (e, stack) {
        completion.completeError(e, stack);
      }
    });
    return completion.future;
  }

  wire.ResponseReader _payload(Uint8List? bytes) {
    if (bytes == null) throw const FormatException('缺少内容响应');
    final r = readMessage(bytes).getRoot(wire.responseFactory);
    if (r.version != 1 || !_same(r.digest, contract.workbenchDigest)) {
      throw const FormatException('工作台插件版本不匹配');
    }
    return r;
  }

  Future<Idea> _idea(host.ResponseReader reply) async {
    final r = _payload(reply.payload).idea!;
    final id = r.id!;
    _revisions[id] = reply.revision;
    final attachments = <IdeaAttachment>[];
    for (final a in r.assets ?? <wire.AssetReader>[]) {
      final key = '$id/${a.id}';
      var item = _assets[key];
      if (item == null) {
        final extension = (a.name ?? '')
            .split('.')
            .last
            .toLowerCase()
            .replaceAll(RegExp('[^a-z0-9]'), '');
        final path = '${cache.path}/${id}_${a.id}.$extension';
        // A fresh host-verified extraction is used for each session. Existing
        // cache names are avoided; user exports are separate platform actions.
        final file = File(
          '$path.${DateTime.now().microsecondsSinceEpoch}.$extension',
        );
        await _call(
          host.Action.exportFile,
          configure: (out) {
            out.id = id;
            out.attachment = a.id;
            out.selectedPath = file.path;
          },
        );
        item = IdeaAttachment(
          source: TextureSource(
            location: file.path,
            name: a.name!,
            kind: TextureKind.values.byName(a.kind!),
            local: true,
          ),
          size: a.bytes,
          pluginId: a.id,
        );
        _assets[key] = item;
      }
      attachments.add(item);
    }
    return Idea(
      r.title ?? '',
      r.description ?? '',
      r.category!,
      Idea.icons[r.icon],
      Color(r.color),
      id: id,
      stage: r.stage,
      favorite: r.favorite,
      todos: [...?r.todos].whereType<String>().toList(),
      completed: {...?r.completed}.whereType<String>().toSet(),
      hypothesis: r.hypothesis ?? '',
      conclusion: r.conclusion ?? '',
      attachments: attachments,
    );
  }

  Future<List<Idea>> load() async {
    final ideas = <Idea>[];
    var cursor = '';
    do {
      final page = await _call(
        host.Action.page,
        configure: (r) {
          r.cursor = cursor;
          r.limit = 128;
        },
      );
      cursor = page.cursor ?? '';
      for (final id in page.ids ?? <String?>[]) {
        final record = await _call(
          host.Action.read,
          configure: (r) => r.id = id,
        );
        final data = _payload(record.payload).idea!;
        _revisions[id!] = record.revision;
        if (!data.deleted) ideas.add(await _idea(record));
      }
    } while (cursor.isNotEmpty);
    return ideas.reversed.toList();
  }

  void _writeIdea(
    wire.IdeaBuilder b,
    Idea idea,
    List<IdeaAttachment> attachments,
  ) {
    b.id = idea.id;
    b.title = idea.title;
    var description = idea.description;
    for (var i = 0; i < attachments.length; i++) {
      final old = idea.attachments[i].source;
      final target = 'attachment:${attachments[i].pluginId}';
      final names = {
        old.location,
        old.name,
        Uri.encodeComponent(old.location),
        Uri.encodeComponent(old.name),
      }.toList()..sort((a, b) => b.length.compareTo(a.length));
      for (final name in names) {
        description = description.replaceAll(
          RegExp('attachment:${RegExp.escape(name)}(?=[\\s)>]|\u0024)'),
          target,
        );
      }
    }
    b.description = description;
    b.category = idea.category;
    b.stage = idea.stage;
    b.hypothesis = idea.hypothesis;
    b.conclusion = idea.conclusion;
    b.favorite = idea.favorite;
    b.icon = Idea.icons.indexOf(idea.icon).clamp(0, 3);
    b.color = idea.color.toARGB32();
    final todos = b.initTodos(idea.todos.length);
    for (var i = 0; i < idea.todos.length; i++) {
      todos[i] = idea.todos[i];
    }
    final completed = idea.completed.toList();
    final done = b.initCompleted(completed.length);
    for (var i = 0; i < completed.length; i++) {
      done[i] = completed[i];
    }
    final assets = b.initAssets(attachments.length);
    for (var i = 0; i < attachments.length; i++) {
      final a = attachments[i];
      final out = assets[i];
      out.id = a.pluginId!;
      out.name = a.source.name;
      out.kind = a.source.kind.name;
      out.bytes = a.size;
    }
  }

  @override
  Future<Idea> apply(
    PluginAction action,
    Idea idea, {
    String text = '',
    bool flag = false,
  }) async {
    if (!writable) throw StateError('工作台插件不可用，已有内容仍可查看和导出。');
    final attachments = <IdeaAttachment>[];
    if (action == PluginAction.create || action == PluginAction.edit) {
      for (final a in idea.attachments) {
        if (a.pluginId != null) {
          attachments.add(a);
          continue;
        }
        final r = await _call(
          host.Action.importFile,
          configure: (r) {
            r.id = idea.id;
            r.selectedPath = a.source.location;
            r.name = a.source.name;
            r.kind = a.source.kind.name;
          },
        );
        final asset = _payload(r.payload).idea!.assets![0];
        final item = IdeaAttachment(
          source: a.source,
          size: asset.bytes,
          pluginId: asset.id,
        );
        attachments.add(item);
        _assets['${idea.id}/${asset.id}'] = item;
      }
    }
    final builder = MessageBuilder();
    final r = builder.initRoot(wire.requestFactory);
    r.version = 1;
    r.digest = Uint8List.fromList(contract.workbenchDigest);
    r.action = wire.Action.values.byName(action.name);
    r.text = text;
    r.flag = flag;
    if (action == PluginAction.create || action == PluginAction.edit) {
      _writeIdea(r.initProposed(), idea, attachments);
    }
    final bytes = builder.serialize();
    if (bytes.length > 65536) {
      throw const FormatException('当前插件消息容量不足，请缩短正文或改为附件。');
    }
    final result = await _call(
      host.Action.mutate,
      configure: (r) {
        r.id = idea.id;
        r.operation =
            'ui-${DateTime.now().microsecondsSinceEpoch}-${_sequence++}';
        r.revision = action == PluginAction.create
            ? 0
            : _revisions[idea.id] ?? 0;
        r.payload = bytes;
      },
    );
    return _idea(result);
  }

  @override
  Future<List<String>> query(
    String section,
    String filter,
    String text,
    String sort,
  ) async {
    final builder = MessageBuilder();
    final r = builder.initRoot(wire.requestFactory);
    r.version = 1;
    r.digest = Uint8List.fromList(contract.workbenchDigest);
    r.action = wire.Action.query;
    r.section = section;
    r.filter = filter;
    r.text = text;
    r.sort = sort;
    final result = await _call(
      host.Action.query,
      configure: (r) => r.payload = builder.serialize(),
    );
    return [...?result.ids].whereType<String>().toList();
  }

  Future<Uint8List> service(Uint8List bytes) async => (await _call(
    host.Action.service,
    configure: (r) => r.payload = bytes,
  )).payload!;
  Future<Uint8List> capture(Uint8List bytes) async => (await _call(
    host.Action.capture,
    configure: (r) => r.payload = bytes,
  )).payload!;
  static const maxPreferencesBytes = 4 * 1024 * 1024;
  static const _partBytes = 32768;
  Future<void> _preferencesQueue = Future.value();
  Future<T> _preferencesJob<T>(Future<T> Function() job) {
    final result = Completer<T>();
    _preferencesQueue = _preferencesQueue.then((_) async {
      try {
        result.complete(await job());
      } catch (error, stack) {
        result.completeError(error, stack);
      }
    });
    return result.future;
  }

  Future<void> _abortPreferences(String token) async {
    try {
      await _call(
        host.Action.abortPreferences,
        configure: (r) => r.transfer = token,
      );
    } catch (_) {
      /* A closed connection drops its bounded staging buffers. */
    }
  }

  Future<Uint8List?> readPreferences() => _preferencesJob(_readPreferences);
  Future<Uint8List?> _readPreferences() async {
    var response = await _call(host.Action.readPreferences);
    if (response.totalLength == 0 && (response.payload?.isEmpty ?? true)) {
      return null;
    }
    final token = response.transfer ?? '';
    final total = response.totalLength;
    final digest = response.sha256;
    try {
      if (token.isEmpty ||
          total <= 0 ||
          total > maxPreferencesBytes ||
          digest?.length != 32) {
        throw const FormatException('配置快照无效');
      }
      final result = BytesBuilder(copy: false);
      var offset = 0;
      while (true) {
        final part = response.payload;
        if (response.transfer != token ||
            response.totalLength != total ||
            response.offset != offset ||
            !_same(response.sha256, digest!) ||
            part == null ||
            part.isEmpty ||
            part.length > _partBytes ||
            part.length > total - offset) {
          throw const FormatException('配置片段不匹配');
        }
        result.add(part);
        offset += part.length;
        if (offset == total) break;
        response = await _call(
          host.Action.readPreferencesPart,
          configure: (r) {
            r.transfer = token;
            r.offset = offset;
          },
        );
      }
      final bytes = result.takeBytes();
      if (!_same(sha256.convert(bytes).bytes, digest)) {
        throw const FormatException('配置校验失败');
      }
      return bytes;
    } catch (_) {
      await _abortPreferences(token);
      rethrow;
    }
  }

  Future<Uint8List> savePreferences(Uint8List value) {
    // The caller may reuse its buffer while queued: freeze this proposal now.
    final bytes = Uint8List.fromList(value);
    return _preferencesJob(() async {
      if (bytes.isEmpty || bytes.length > maxPreferencesBytes) {
        throw const FormatException('配置超过 4 MiB，原设置保留');
      }
      final begin = await _call(
        host.Action.beginPreferences,
        configure: (r) {
          r.operation =
              'prefs-${DateTime.now().microsecondsSinceEpoch}-${_sequence++}';
          r.totalLength = bytes.length;
          r.sha256 = Uint8List.fromList(sha256.convert(bytes).bytes);
        },
      );
      final token = begin.transfer ?? '';
      if (token.isEmpty) throw const FormatException('缺少配置传输标识');
      try {
        for (var offset = 0; offset < bytes.length;) {
          final end = (offset + _partBytes).clamp(0, bytes.length);
          final reply = await _call(
            host.Action.appendPreferences,
            configure: (r) {
              r.transfer = token;
              r.offset = offset;
              r.payload = Uint8List.sublistView(bytes, offset, end);
            },
          );
          if (reply.transfer != token || reply.offset != end) {
            throw const FormatException('配置上传确认不匹配');
          }
          offset = end;
        }
        final committed = await _call(
          host.Action.finishPreferences,
          configure: (r) => r.transfer = token,
        );
        final stored = await _readPreferences();
        if (stored == null ||
            committed.totalLength != stored.length ||
            !_same(committed.sha256, sha256.convert(stored).bytes)) {
          throw const FormatException('配置提交后快照不匹配，请重新打开工作台确认');
        }
        return stored;
      } catch (_) {
        await _abortPreferences(token);
        rethrow;
      }
    });
  }

  Future<void> close() async {
    try {
      await process.stdin.close();
    } catch (_) {}
    try {
      await process.exitCode.timeout(const Duration(seconds: 5));
    } on TimeoutException {
      process.kill();
      await process.exitCode;
    }
  }
}
