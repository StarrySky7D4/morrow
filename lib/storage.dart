import 'dart:convert';

import 'package:shared_preferences/shared_preferences.dart';

/// One versioned snapshot keeps appearance and content in sync.
abstract class StudioStorage {
  Map<String, dynamic>? read();
  Future<void> write(Map<String, dynamic> data);
}

class MemoryStorage implements StudioStorage {
  Map<String, dynamic>? data;
  @override
  Map<String, dynamic>? read() => data;
  @override
  Future<void> write(Map<String, dynamic> data) async {
    this.data = jsonDecode(jsonEncode(data)) as Map<String, dynamic>;
  }
}

class LocalStorage implements StudioStorage {
  LocalStorage(this.preferences);
  final SharedPreferences preferences;
  static const key = 'daemon.studio.v1';
  Future<void> _pending = Future.value();

  @override
  Map<String, dynamic>? read() {
    final raw = preferences.getString(key);
    if (raw == null) return null;
    final data = jsonDecode(raw) as Map<String, dynamic>;
    if (data['version'] != 1) {
      throw const FormatException('Unsupported data version');
    }
    return data;
  }

  @override
  Future<void> write(Map<String, dynamic> data) {
    final encoded = jsonEncode(data);
    // Serialize rapid toggles so an older write cannot overwrite the latest one.
    final write = _pending.then((_) async {
      if (!await preferences.setString(key, encoded)) {
        throw StateError('Local save failed');
      }
    });
    _pending = write.catchError((Object _) {});
    return write;
  }
}
