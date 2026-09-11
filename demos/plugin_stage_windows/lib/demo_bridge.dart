import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';
import 'package:morrow_core_client/ui.dart';
import 'package:morrow_plugin_ui/morrow_plugin_ui.dart';

/// One trusted child/session per selected bundled plugin; binary pipes only.
class DemoBridge {
  DemoBridge._(this._process) : _input = StreamIterator(_process.stdout) {
    _errors = _process.stderr.listen((_) {});
  }
  final Process _process;
  final StreamIterator<List<int>> _input;
  late final StreamSubscription<List<int>> _errors;
  final _buffer = <int>[];
  BigInt revision = BigInt.zero;
  BigInt _serial = BigInt.zero;
  bool _closed = false;
  late UiDocument document;
  static Future<DemoBridge> open(String folder, String language) async {
    if (!['rust', 'c', 'cpp'].contains(language)) {
      throw ArgumentError('Language');
    }
    final p = await Process.start('$folder/stage_demo_host.exe', [
      '$folder/plugins/$language.mplugin',
    ], workingDirectory: folder);
    final b = DemoBridge._(p);
    try {
      b.document = await b._receive().timeout(const Duration(seconds: 15));
      return b;
    } catch (_) {
      await b.close();
      rethrow;
    }
  }

  Future<Uint8List> _read(int count) async {
    while (_buffer.length < count) {
      if (!await _input.moveNext()) throw const FormatException('演示宿主已断开');
      _buffer.addAll(_input.current);
      if (_buffer.length > 131100) throw const FormatException('返回消息超限');
    }
    final bytes = Uint8List.fromList(_buffer.take(count).toList());
    _buffer.removeRange(0, count);
    return bytes;
  }

  Future<UiDocument> _receive() async {
    final header = await _read(5);
    final length = ByteData.sublistView(header, 1).getUint32(0, Endian.little);
    if (length > 65544) throw const FormatException('返回消息超限');
    final bytes = await _read(length);
    if (header[0] == 1) throw FormatException(utf8.decode(bytes));
    if (header[0] != 0 || bytes.length < 16) {
      throw const FormatException('无效界面响应');
    }
    final data = ByteData.sublistView(bytes);
    final next =
        BigInt.from(data.getUint32(0, Endian.little)) |
        (BigInt.from(data.getUint32(4, Endian.little)) << 32);
    if (next != revision + BigInt.one) throw const FormatException('界面版本不连续');
    final result = UiDocument.decode(Uint8List.sublistView(bytes, 8));
    revision = next;
    return result;
  }

  /// Caller serializes edits. Failure closes the session; no automatic retry.
  Future<UiDocument> edit(UiIntent intent) async {
    if (_closed) throw StateError('Closed demo session');
    _serial += BigInt.one;
    final bytes = UiEvent(
      view: 'demo-view',
      generation: (BigInt.one << 64) - BigInt.one,
      revision: revision,
      serial: _serial,
      node: intent.node,
      action: intent.action,
      kind: intent.kind,
      text: intent.text,
      checked: intent.checked,
    ).encode();
    final header = Uint8List(5);
    header[0] = 1;
    ByteData.sublistView(header, 1).setUint32(0, bytes.length, Endian.little);
    try {
      _process.stdin.add(header);
      _process.stdin.add(bytes);
      await _process.stdin.flush();
      document = await _receive().timeout(const Duration(seconds: 15));
      return document;
    } catch (_) {
      await close();
      rethrow;
    }
  }

  Future<void> close() async {
    if (_closed) return;
    _closed = true;
    try {
      _process.stdin.add([0, 0, 0, 0, 0]);
      await _process.stdin.flush();
      await _process.stdin.close();
    } catch (_) {}
    try {
      await _process.exitCode.timeout(const Duration(seconds: 2));
    } catch (_) {
      _process.kill();
    }
    await _input.cancel();
    await _errors.cancel();
  }
}
