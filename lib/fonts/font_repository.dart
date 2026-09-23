import 'package:crypto/crypto.dart';
import 'package:file_selector/file_selector.dart';
import 'package:flutter/services.dart';
import 'font_choice.dart';
import 'font_storage_native.dart'
    if (dart.library.js_interop) 'font_storage_web.dart'
    as storage;

class FontRepository {
  static const maxBytes = 20 * 1024 * 1024;
  // Flutter cannot unload registered fonts. Bound decoded source bytes and
  // registered families per process; a restart releases the engine resources.
  static final _loaded = <String, Future<void>>{};
  static int _loadedBytes = 0;
  static void validateBytes(Uint8List data) {
    if (data.length < 12 || data.length > maxBytes) {
      throw const FormatException('Invalid font size');
    }
    final view = ByteData.sublistView(data);
    final signature = view.getUint32(0);
    if (signature != 0x00010000 && signature != 0x4f54544f) {
      throw const FormatException('Expected TTF or OTF');
    }
    final tables = view.getUint16(4);
    if (tables == 0 || tables > 256 || 12 + tables * 16 > data.length) {
      throw const FormatException('Invalid font tables');
    }
    for (var i = 0; i < tables; i++) {
      final offset = view.getUint32(12 + i * 16 + 8);
      final length = view.getUint32(12 + i * 16 + 12);
      if (offset > data.length || length > data.length - offset) {
        throw const FormatException('Invalid font table range');
      }
    }
  }

  static Future<void> _register(FontChoice choice, Uint8List bytes) async {
    validateBytes(bytes);
    if (sha256.convert(bytes).toString() != choice.asset) {
      throw const FormatException('Font digest mismatch');
    }
    if (_loaded[choice.asset] case final pending?) return pending;
    if (_loaded.length >= 8 || _loadedBytes + bytes.length > 64 * 1024 * 1024) {
      throw const FormatException('Font session limit');
    }
    _loadedBytes += bytes.length;
    final loader = FontLoader(choice.resolvedFamily!)
      ..addFont(Future.value(ByteData.sublistView(bytes)));
    // Keep failed registrations charged too; the native engine may retain data.
    final pending = loader.load();
    _loaded[choice.asset] = pending;
    await pending;
  }

  static Future<void> load(
    FontChoice choice, {
    String? libraryDirectory,
  }) async {
    choice.validate();
    if (!choice.imported) return;
    if (_loaded[choice.asset] case final pending?) return pending;
    await _register(
      choice,
      await storage.read(
        choice.asset,
        maxBytes,
        libraryDirectory: libraryDirectory,
      ),
    );
  }

  static Future<FontChoice> importFile(XFile file) async {
    if (await file.length() > maxBytes) {
      throw const FormatException('Font too large');
    }
    final bytes = await file.readAsBytes();
    validateBytes(bytes);
    final choice = FontChoice(
      asset: sha256.convert(bytes).toString(),
      name: file.name.split(RegExp(r'[/\\]')).last,
    );
    choice.validate();
    await _register(choice, bytes);
    await storage.store(choice.asset, bytes);
    return choice;
  }
}
